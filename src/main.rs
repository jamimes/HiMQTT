use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use config::FileFormat;
use rumqttd::Broker;
use tracing::trace;

static DEFAULT_CONFIG: &str = include_str!("../config/himqtt.toml");

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

    let mut config_builder = config::Config::builder();

    if cli.config.exists() {
        config_builder =
            config_builder.add_source(config::File::with_name(cli.config.to_str().unwrap()));
    } else {
        eprintln!(
            "配置文件 {} 不存在，使用内置默认配置",
            cli.config.display()
        );
        config_builder =
            config_builder.add_source(config::File::from_str(DEFAULT_CONFIG, FileFormat::Toml));
    }

    let mut configs: rumqttd::Config = config_builder
        .build()
        .context("读取配置失败")?
        .try_deserialize()
        .context("解析配置失败")?;

    if let Some(console_config) = configs.console.as_mut() {
        console_config.set_filter_reload_handle(reload_handle);
    }

    validate_config(&configs);

    let mut broker = Broker::new(configs);
    broker
        .start()
        .map_err(|e| anyhow::anyhow!("MQTT 服务器启动失败: {e}"))?;

    Ok(())
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
