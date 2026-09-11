mod matcher;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use rumqttd::AclOperation;
use sqlx::PgPool;
use tracing::{info, warn};

use crate::db::{self, AclRuleRow, MqttUserRow};

#[derive(Debug, Clone)]
pub struct AclConfig {
    pub enforce_registered_topics: bool,
    pub cache_refresh_secs: u64,
}

#[derive(Debug, Default)]
struct AclCache {
    mqtt_users: HashMap<String, MqttUserRow>,
    registered_topics: Vec<String>,
    rules_by_user: HashMap<String, Vec<AclRuleRow>>,
    loaded_at: Option<Instant>,
}

#[derive(Clone)]
pub struct AclService {
    pool: PgPool,
    config: AclConfig,
    cache: Arc<RwLock<AclCache>>,
}

impl AclService {
    pub fn new(pool: PgPool, config: AclConfig) -> Self {
        Self {
            pool,
            config,
            cache: Arc::new(RwLock::new(AclCache::default())),
        }
    }

    pub async fn init(&self) -> Result<()> {
        self.reload().await?;
        Ok(())
    }

    pub fn spawn_refresh_task(self: &Arc<Self>) {
        let this = Arc::clone(self);
        let interval = Duration::from_secs(this.config.cache_refresh_secs.max(5));
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                if let Err(e) = this.reload().await {
                    warn!("ACL 缓存刷新失败: {e:#}");
                }
            }
        });
    }

    pub async fn reload(&self) -> Result<()> {
        let users = db::list_mqtt_users(&self.pool).await?;
        let topics = db::list_mqtt_topics(&self.pool).await?;
        let rules = db::list_topic_acls(&self.pool).await?;

        let mut mqtt_users = HashMap::new();
        for user in users {
            mqtt_users.insert(user.username.clone(), user);
        }

        let mut rules_by_user: HashMap<String, Vec<AclRuleRow>> = HashMap::new();
        for rule in rules {
            rules_by_user
                .entry(rule.username.clone())
                .or_default()
                .push(rule);
        }

        let registered_topics: Vec<String> = topics.into_iter().map(|t| t.topic).collect();

        {
            let mut cache = self.cache.write().unwrap();
            cache.mqtt_users = mqtt_users;
            cache.registered_topics = registered_topics;
            cache.rules_by_user = rules_by_user;
            cache.loaded_at = Some(Instant::now());
        }

        info!("ACL 缓存已刷新");
        Ok(())
    }

    pub async fn verify_mqtt_password(&self, username: &str, password: &str) -> bool {
        if let Ok(cache) = self.cache.read() {
            if let Some(user) = cache.mqtt_users.get(username) {
                if user.enabled {
                    return bcrypt::verify(password, &user.password_hash).unwrap_or(false);
                }
            }
        }

        match db::get_mqtt_user(&self.pool, username).await {
            Ok(Some(user)) if user.enabled => {
                bcrypt::verify(password, &user.password_hash).unwrap_or(false)
            }
            _ => false,
        }
    }

    pub async fn record_last_login(&self, username: &str, ip: &str) -> Result<()> {
        db::record_mqtt_user_login(&self.pool, username, ip).await
    }

    pub fn check_acl(&self, username: &str, topic_or_filter: &str, op: AclOperation) -> bool {
        let cache = match self.cache.read() {
            Ok(c) => c,
            Err(_) => return false,
        };

        if username.is_empty() {
            return false;
        }

        if self.config.enforce_registered_topics {
            let allowed_topic = cache.registered_topics.iter().any(|registered| {
                match op {
                    AclOperation::Subscribe => {
                        matcher::registered_covers(registered, topic_or_filter)
                            || matcher::subscribe_allowed(registered, topic_or_filter)
                    }
                    AclOperation::Publish => matcher::registered_covers(registered, topic_or_filter),
                }
            });
            if !allowed_topic {
                return false;
            }
        }

        let Some(rules) = cache.rules_by_user.get(username) else {
            return false;
        };

        rules.iter().any(|rule| match op {
            AclOperation::Subscribe => {
                rule.can_subscribe && matcher::subscribe_allowed(&rule.topic_pattern, topic_or_filter)
            }
            AclOperation::Publish => {
                rule.can_publish && matcher::publish_allowed(&rule.topic_pattern, topic_or_filter)
            }
        })
    }

    pub fn acl_handler(service: Arc<AclService>) -> impl Fn(&str, &str, AclOperation) -> bool + Send + Sync + 'static
    {
        move |username, topic, op| service.check_acl(username, topic, op)
    }

    pub async fn ensure_seed_data(&self, default_admin_password: &str) -> Result<()> {
        let admin_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM admin_users")
                .fetch_one(&self.pool)
                .await
                .context("查询 admin_users 失败")?;

        if admin_count == 0 {
            let hash = bcrypt::hash(default_admin_password, bcrypt::DEFAULT_COST)
                .context("hash admin 密码失败")?;
            db::create_admin_user(&self.pool, "admin", &hash).await?;
            info!("已创建默认管理账号 admin");
        }

        Ok(())
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}
