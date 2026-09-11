use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

use anyhow::{Context, Result};
use clap::Parser;
use config::FileFormat;
use rumqttd::Broker;
use tracing::trace;

mod acl;
mod admin;
mod db;
mod monitor;

static DEFAULT_CONFIG: &str = include_str!("../config/himqtt.toml");

#[derive(Debug, Clone, serde::Deserialize)]
struct DatabaseSettings {
    url: String,
    #[serde(default = "default_db_max_connections")]
    max_connections: u32,
}

fn default_db_max_connections() -> u32 {
    5
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
struct AclSettings {
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default = "default_true")]
    enforce_registered_topics: bool,
    #[serde(default = "default_cache_refresh")]
    cache_refresh_secs: u64,
}

fn default_true() -> bool {
    true
}

fn default_cache_refresh() -> u64 {
    10
}

#[derive(Debug, Clone, serde::Deserialize)]
struct AppSettings {
    #[serde(default)]
    monitor: monitor::MonitorConfig,
    #[serde(default)]
    database: Option<DatabaseSettings>,
    #[serde(default)]
    acl: AclSettings,
    #[serde(default)]
    admin: admin::AdminConfig,
}

#[derive(Parser)]
#[command(name = "himqtt")]
#[command(version)]
#[command(about = "HiMQTT — 基于 Rust 的高性能 MQTT 服务器")]
struct Cli {
    /// 配置文件路径（TOML）
    #[arg(short, long, default_value = "config/himqtt.toml")]
    config: PathBuf,

    /// 日志级别：-v info，-vv debug，-vvv trace
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count)]
    verbose: u8,

    /// 启动时不打印横幅
    #[arg(short, long)]
    quiet: bool,

    /// 禁用监控 Web 页面
    #[arg(long)]
    no_monitor: bool,

    /// 禁用管理后台
    #[arg(long)]
    no_admin: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Parser)]
enum Command {
    /// 将默认配置写入 stdout
    GenerateConfig,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if matches!(cli.command, Some(Command::GenerateConfig)) {
        print!("{DEFAULT_CONFIG}");
        return Ok(());
    }

    if !cli.quiet {
        print_banner();
    }

    let level = match cli.verbose {
        0 => "rumqttd=warn,himqtt=warn",
        1 => "rumqttd=info,himqtt=info",
        2 => "rumqttd=debug,himqtt=debug",
        _ => "rumqttd=trace,himqtt=trace",
    };

    let builder = tracing_subscriber::fmt()
        .pretty()
        .with_line_number(false)
        .with_file(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_env_filter(level)
        .with_filter_reloading();

    let reload_handle = builder.reload_handle();
    builder
        .try_init()
        .expect("初始化日志订阅器失败");

    let config_path = if cli.config.exists() {
        cli.config.to_str().unwrap().to_string()
    } else {
        eprintln!(
            "配置文件 {} 不存在，使用内置默认配置",
            cli.config.display()
        );
        String::new()
    };

    let (mut configs, app_settings) = load_config(&config_path)?;

    if let Some(console_config) = configs.console.as_mut() {
        console_config.set_filter_reload_handle(reload_handle);
    }

    validate_config(&configs);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("创建 tokio runtime 失败")?;

    let acl_service = if app_settings.acl.enabled {
        let db_cfg = app_settings
            .database
            .as_ref()
            .context("启用 ACL 时必须在配置中设置 [database]")?;
        let pool = rt.block_on(db::connect(&db_cfg.url, db_cfg.max_connections))?;
        rt.block_on(db::migrate(&pool))?;
        let acl = Arc::new(AclService::from_settings(pool, &app_settings.acl));
        rt.block_on(acl.init())?;
        rt.block_on(acl.ensure_seed_data(&app_settings.admin.default_password))?;

        let auth_acl = Arc::clone(&acl);
        install_auth_handlers(&mut configs, auth_acl);
        configs.set_acl_handler(acl::AclService::acl_handler(Arc::clone(&acl)));

        Some(acl)
    } else {
        None
    };

    let mut broker = Broker::new(configs);

    let monitor_state = if !cli.no_monitor {
        let meters = broker.meters().context("创建 meters 链路失败")?;
        let (mut link_tx, link_rx) = broker.link("himqtt-monitor").context("创建监控链路失败")?;
        link_tx
            .subscribe("#")
            .context("监控订阅 # 失败")?;

        let state = monitor::MonitorState::new(app_settings.monitor.max_messages);
        state.spawn_collector(link_rx, meters);
        Some(state)
    } else {
        None
    };

    thread::Builder::new()
        .name("himqtt-broker".into())
        .spawn(move || {
            if let Err(e) = broker.start() {
                tracing::error!("MQTT 服务器退出: {e}");
            }
        })
        .context("启动 broker 线程失败")?;

    rt.block_on(async move {
        if let Some(acl) = &acl_service {
            acl.spawn_refresh_task();
        }

        let mut tasks = Vec::new();

        if !cli.no_admin {
            if let Some(acl) = acl_service {
                let admin_cfg = app_settings.admin.clone();
                let monitor = monitor_state;
                tasks.push(tokio::spawn(async move {
                    admin::serve(acl, admin_cfg, monitor).await
                }));
            } else {
                tracing::warn!("ACL 未启用，管理后台不会启动");
            }
        } else if monitor_state.is_some() {
            tracing::warn!("已禁用管理后台，连接监控不可用（监控已并入管理后台）");
        }

        if tasks.is_empty() {
            tokio::signal::ctrl_c().await.context("等待退出信号失败")?;
            return Ok(());
        }

        let (result, _, _) = futures_util::future::select_all(tasks).await;
        result.context("后台任务 join 失败")?
    })?;

    Ok(())
}

use acl::{AclConfig, AclService};

fn install_auth_handlers(configs: &mut rumqttd::Config, acl: Arc<AclService>) {
    if let Some(v4) = configs.v4.as_mut() {
        for server in v4.values_mut() {
            let acl = Arc::clone(&acl);
            server.set_auth_handler(move |_client_id, username, password, peer| {
                let acl = Arc::clone(&acl);
                async move {
                    let ok = acl.verify_mqtt_password(&username, &password).await;
                    if ok {
                        let ip = peer_ip_only(&peer);
                        if let Err(e) = acl.record_last_login(&username, &ip).await {
                            tracing::warn!("记录最后登录失败: {e:#}");
                        }
                    }
                    ok
                }
            });
        }
    }
    if let Some(v5) = configs.v5.as_mut() {
        for server in v5.values_mut() {
            let acl = Arc::clone(&acl);
            server.set_auth_handler(move |_client_id, username, password, peer| {
                let acl = Arc::clone(&acl);
                async move {
                    let ok = acl.verify_mqtt_password(&username, &password).await;
                    if ok {
                        let ip = peer_ip_only(&peer);
                        if let Err(e) = acl.record_last_login(&username, &ip).await {
                            tracing::warn!("记录最后登录失败: {e:#}");
                        }
                    }
                    ok
                }
            });
        }
    }
    if let Some(ws) = configs.ws.as_mut() {
        for server in ws.values_mut() {
            let acl = Arc::clone(&acl);
            server.set_auth_handler(move |_client_id, username, password, peer| {
                let acl = Arc::clone(&acl);
                async move {
                    let ok = acl.verify_mqtt_password(&username, &password).await;
                    if ok {
                        let ip = peer_ip_only(&peer);
                        if let Err(e) = acl.record_last_login(&username, &ip).await {
                            tracing::warn!("记录最后登录失败: {e:#}");
                        }
                    }
                    ok
                }
            });
        }
    }
}

fn peer_ip_only(peer: &str) -> String {
    // SocketAddr Display is "ip:port" (IPv6 in brackets)
    if let Ok(addr) = peer.parse::<std::net::SocketAddr>() {
        return addr.ip().to_string();
    }
    peer.to_owned()
}

impl AclService {
    fn from_settings(pool: sqlx::PgPool, settings: &AclSettings) -> Self {
        Self::new(
            pool,
            AclConfig {
                enforce_registered_topics: settings.enforce_registered_topics,
                cache_refresh_secs: settings.cache_refresh_secs,
            },
        )
    }
}

fn load_config(path: &str) -> Result<(rumqttd::Config, AppSettings)> {
    let mut builder = config::Config::builder();
    if path.is_empty() {
        builder = builder.add_source(config::File::from_str(DEFAULT_CONFIG, FileFormat::Toml));
    } else {
        builder = builder.add_source(config::File::with_name(path));
    }

    let settings = builder
        .build()
        .context("读取配置失败")?
        .try_deserialize::<AppSettings>()
        .context("解析配置失败")?;

    let mut broker_builder = config::Config::builder();
    if path.is_empty() {
        broker_builder = broker_builder.add_source(config::File::from_str(DEFAULT_CONFIG, FileFormat::Toml));
    } else {
        broker_builder = broker_builder.add_source(config::File::with_name(path));
    }

    let configs: rumqttd::Config = broker_builder
        .build()
        .context("读取 broker 配置失败")?
        .try_deserialize()
        .context("解析 broker 配置失败")?;

    Ok((configs, settings))
}

fn validate_config(configs: &rumqttd::Config) {
    if let Some(v4) = &configs.v4 {
        for (name, server_setting) in v4 {
            if let Some(tls_config) = &server_setting.tls {
                if !tls_config.validate_paths() {
                    panic!("v4.{name} 的 TLS 证书路径无效");
                }
                trace!("已验证 v4.{name} 的 TLS 证书路径");
            }
        }
    }

    if let Some(v5) = &configs.v5 {
        for (name, server_setting) in v5 {
            if let Some(tls_config) = &server_setting.tls {
                if !tls_config.validate_paths() {
                    panic!("v5.{name} 的 TLS 证书路径无效");
                }
                trace!("已验证 v5.{name} 的 TLS 证书路径");
            }
        }
    }

    if let Some(ws) = &configs.ws {
        for (name, server_setting) in ws {
            if let Some(tls_config) = &server_setting.tls {
                if !tls_config.validate_paths() {
                    panic!("ws.{name} 的 TLS 证书路径无效");
                }
                trace!("已验证 ws.{name} 的 TLS 证书路径");
            }
        }
    }
}

fn print_banner() {
    const BANNER: &str = r"
  _    _ _ __  __  _____ _____ ____
 | |  | (_)  \/  |/ ____|_   _|  _ \
 | |__| | | |\/| | |__    | | | |_) |
 |  __  | | |  | |  __|   | | |  __/
 |_|  |_|_|_|  |_|_|      |_| |_|

  Rust MQTT Broker for Ubuntu / Linux
";
    println!("{BANNER}");
}
