use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPoolOptions, PgPool};
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct AdminUserRow {
    pub id: i32,
    pub username: String,
    pub password_hash: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct MqttUserCategoryRow {
    pub id: i32,
    pub name: String,
    pub description: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct MqttUserRow {
    pub id: i32,
    pub username: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub enabled: bool,
    pub category_id: Option<i32>,
    pub category_name: Option<String>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub last_login_ip: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct MqttTopicRow {
    pub id: i32,
    pub topic: String,
    pub description: String,
    pub owner_username: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct AclRuleRow {
    pub id: i32,
    pub username: String,
    pub topic_pattern: String,
    pub can_subscribe: bool,
    pub can_publish: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultTopicTemplate {
    pub topic: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultAclTemplate {
    pub topic_pattern: String,
    #[serde(default)]
    pub can_subscribe: bool,
    #[serde(default)]
    pub can_publish: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserDefaultsSettings {
    #[serde(default)]
    pub default_topics: Vec<DefaultTopicTemplate>,
    #[serde(default)]
    pub default_acls: Vec<DefaultAclTemplate>,
}

impl Default for UserDefaultsSettings {
    fn default() -> Self {
        Self {
            default_topics: vec![],
            default_acls: vec![],
        }
    }
}

const MQTT_USER_SELECT: &str = r#"
    SELECT
        u.id,
        u.username,
        u.password_hash,
        u.enabled,
        u.category_id,
        c.name AS category_name,
        u.last_login_at,
        u.last_login_ip,
        u.created_at,
        u.updated_at
    FROM mqtt_users u
    LEFT JOIN mqtt_user_categories c ON c.id = u.category_id
"#;

pub async fn connect(database_url: &str, max_connections: u32) -> Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(database_url)
        .await
        .context("连接 PostgreSQL 失败")
}

pub async fn migrate(pool: &PgPool) -> Result<()> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .context("数据库迁移失败")
}

pub async fn get_admin_user(pool: &PgPool, username: &str) -> Result<Option<AdminUserRow>> {
    let row = sqlx::query_as::<_, AdminUserRow>(
        "SELECT id, username, password_hash, created_at FROM admin_users WHERE username = $1",
    )
    .bind(username)
    .fetch_optional(pool)
    .await
    .context("查询 admin 用户失败")?;
    Ok(row)
}

pub async fn create_admin_user(pool: &PgPool, username: &str, password_hash: &str) -> Result<()> {
    sqlx::query("INSERT INTO admin_users (username, password_hash) VALUES ($1, $2)")
        .bind(username)
        .bind(password_hash)
        .execute(pool)
        .await
        .context("创建 admin 用户失败")?;
    Ok(())
}

pub async fn list_categories(pool: &PgPool) -> Result<Vec<MqttUserCategoryRow>> {
    let rows = sqlx::query_as::<_, MqttUserCategoryRow>(
        "SELECT id, name, description, created_at FROM mqtt_user_categories ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .context("查询用户分类失败")?;
    Ok(rows)
}

pub async fn create_category(
    pool: &PgPool,
    name: &str,
    description: &str,
) -> Result<MqttUserCategoryRow> {
    let row = sqlx::query_as::<_, MqttUserCategoryRow>(
        r#"
        INSERT INTO mqtt_user_categories (name, description)
        VALUES ($1, $2)
        RETURNING id, name, description, created_at
        "#,
    )
    .bind(name)
    .bind(description)
    .fetch_one(pool)
    .await
    .context("创建用户分类失败")?;
    Ok(row)
}

pub async fn update_category(
    pool: &PgPool,
    id: i32,
    name: &str,
    description: &str,
) -> Result<Option<MqttUserCategoryRow>> {
    let row = sqlx::query_as::<_, MqttUserCategoryRow>(
        r#"
        UPDATE mqtt_user_categories
        SET name = $2, description = $3
        WHERE id = $1
        RETURNING id, name, description, created_at
        "#,
    )
    .bind(id)
    .bind(name)
    .bind(description)
    .fetch_optional(pool)
    .await
    .context("更新用户分类失败")?;
    Ok(row)
}

pub async fn delete_category(pool: &PgPool, id: i32) -> Result<bool> {
    let result = sqlx::query("DELETE FROM mqtt_user_categories WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .context("删除用户分类失败")?;
    Ok(result.rows_affected() > 0)
}

pub async fn list_mqtt_users(pool: &PgPool) -> Result<Vec<MqttUserRow>> {
    let sql = format!("{MQTT_USER_SELECT} ORDER BY u.id");
    let rows = sqlx::query_as::<_, MqttUserRow>(&sql)
        .fetch_all(pool)
        .await
        .context("查询 mqtt_users 失败")?;
    Ok(rows)
}

pub async fn get_mqtt_user(pool: &PgPool, username: &str) -> Result<Option<MqttUserRow>> {
    let sql = format!("{MQTT_USER_SELECT} WHERE u.username = $1");
    let row = sqlx::query_as::<_, MqttUserRow>(&sql)
        .bind(username)
        .fetch_optional(pool)
        .await
        .context("查询 mqtt_user 失败")?;
    Ok(row)
}

pub async fn get_mqtt_user_by_id(pool: &PgPool, id: i32) -> Result<Option<MqttUserRow>> {
    let sql = format!("{MQTT_USER_SELECT} WHERE u.id = $1");
    let row = sqlx::query_as::<_, MqttUserRow>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await
        .context("按 id 查询 mqtt_user 失败")?;
    Ok(row)
}

pub async fn create_mqtt_user(
    pool: &PgPool,
    username: &str,
    password_hash: &str,
    enabled: bool,
    category_id: Option<i32>,
) -> Result<MqttUserRow> {
    let row = sqlx::query_as::<_, MqttUserRow>(
        r#"
        WITH inserted AS (
            INSERT INTO mqtt_users (username, password_hash, enabled, category_id)
            VALUES ($1, $2, $3, $4)
            RETURNING *
        )
        SELECT
            i.id,
            i.username,
            i.password_hash,
            i.enabled,
            i.category_id,
            c.name AS category_name,
            i.last_login_at,
            i.last_login_ip,
            i.created_at,
            i.updated_at
        FROM inserted i
        LEFT JOIN mqtt_user_categories c ON c.id = i.category_id
        "#,
    )
    .bind(username)
    .bind(password_hash)
    .bind(enabled)
    .bind(category_id)
    .fetch_one(pool)
    .await
    .context("创建 mqtt_user 失败")?;
    Ok(row)
}

pub async fn update_mqtt_user(
    pool: &PgPool,
    id: i32,
    password_hash: Option<&str>,
    enabled: bool,
    category_id: Option<Option<i32>>,
) -> Result<Option<MqttUserRow>> {
    // category_id: None = 不改; Some(None) = 清空; Some(Some(id)) = 设置
    let row = match (password_hash, category_id) {
        (Some(hash), Some(cat)) => {
            sqlx::query_as::<_, MqttUserRow>(
                r#"
                WITH updated AS (
                    UPDATE mqtt_users
                    SET password_hash = $2, enabled = $3, category_id = $4, updated_at = NOW()
                    WHERE id = $1
                    RETURNING *
                )
                SELECT
                    u.id, u.username, u.password_hash, u.enabled, u.category_id,
                    c.name AS category_name, u.last_login_at, u.last_login_ip,
                    u.created_at, u.updated_at
                FROM updated u
                LEFT JOIN mqtt_user_categories c ON c.id = u.category_id
                "#,
            )
            .bind(id)
            .bind(hash)
            .bind(enabled)
            .bind(cat)
            .fetch_optional(pool)
            .await
        }
        (Some(hash), None) => {
            sqlx::query_as::<_, MqttUserRow>(
                r#"
                WITH updated AS (
                    UPDATE mqtt_users
                    SET password_hash = $2, enabled = $3, updated_at = NOW()
                    WHERE id = $1
                    RETURNING *
                )
                SELECT
                    u.id, u.username, u.password_hash, u.enabled, u.category_id,
                    c.name AS category_name, u.last_login_at, u.last_login_ip,
                    u.created_at, u.updated_at
                FROM updated u
                LEFT JOIN mqtt_user_categories c ON c.id = u.category_id
                "#,
            )
            .bind(id)
            .bind(hash)
            .bind(enabled)
            .fetch_optional(pool)
            .await
        }
        (None, Some(cat)) => {
            sqlx::query_as::<_, MqttUserRow>(
                r#"
                WITH updated AS (
                    UPDATE mqtt_users
                    SET enabled = $2, category_id = $3, updated_at = NOW()
                    WHERE id = $1
                    RETURNING *
                )
                SELECT
                    u.id, u.username, u.password_hash, u.enabled, u.category_id,
                    c.name AS category_name, u.last_login_at, u.last_login_ip,
                    u.created_at, u.updated_at
                FROM updated u
                LEFT JOIN mqtt_user_categories c ON c.id = u.category_id
                "#,
            )
            .bind(id)
            .bind(enabled)
            .bind(cat)
            .fetch_optional(pool)
            .await
        }
        (None, None) => {
            sqlx::query_as::<_, MqttUserRow>(
                r#"
                WITH updated AS (
                    UPDATE mqtt_users
                    SET enabled = $2, updated_at = NOW()
                    WHERE id = $1
                    RETURNING *
                )
                SELECT
                    u.id, u.username, u.password_hash, u.enabled, u.category_id,
                    c.name AS category_name, u.last_login_at, u.last_login_ip,
                    u.created_at, u.updated_at
                FROM updated u
                LEFT JOIN mqtt_user_categories c ON c.id = u.category_id
                "#,
            )
            .bind(id)
            .bind(enabled)
            .fetch_optional(pool)
            .await
        }
    }
    .context("更新 mqtt_user 失败")?;
    Ok(row)
}

pub async fn delete_mqtt_user(pool: &PgPool, id: i32) -> Result<bool> {
    let result = sqlx::query("DELETE FROM mqtt_users WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .context("删除 mqtt_user 失败")?;
    Ok(result.rows_affected() > 0)
}

pub async fn record_mqtt_user_login(
    pool: &PgPool,
    username: &str,
    ip: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE mqtt_users
        SET last_login_at = NOW(), last_login_ip = $2, updated_at = NOW()
        WHERE username = $1
        "#,
    )
    .bind(username)
    .bind(ip)
    .execute(pool)
    .await
    .context("更新最后登录信息失败")?;
    Ok(())
}

pub fn render_template(template: &str, username: &str) -> String {
    template.replace("{username}", username)
}

pub async fn apply_user_defaults(
    pool: &PgPool,
    username: &str,
    defaults: &UserDefaultsSettings,
) -> Result<()> {
    for topic in &defaults.default_topics {
        let rendered = render_template(&topic.topic, username);
        let desc = render_template(&topic.description, username);
        let _ = sqlx::query(
            r#"
            INSERT INTO mqtt_topics (topic, description, owner_username)
            VALUES ($1, $2, $3)
            ON CONFLICT (topic) DO UPDATE
            SET description = EXCLUDED.description,
                owner_username = COALESCE(mqtt_topics.owner_username, EXCLUDED.owner_username)
            "#,
        )
        .bind(&rendered)
        .bind(&desc)
        .bind(username)
        .execute(pool)
        .await
        .context("应用默认 Topic 失败")?;
    }

    for acl in &defaults.default_acls {
        let pattern = render_template(&acl.topic_pattern, username);
        let _ = sqlx::query(
            r#"
            INSERT INTO topic_acl (username, topic_pattern, can_subscribe, can_publish)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (username, topic_pattern) DO UPDATE
            SET can_subscribe = EXCLUDED.can_subscribe,
                can_publish = EXCLUDED.can_publish
            "#,
        )
        .bind(username)
        .bind(&pattern)
        .bind(acl.can_subscribe)
        .bind(acl.can_publish)
        .execute(pool)
        .await
        .context("应用默认 ACL 失败")?;
    }

    Ok(())
}

pub async fn get_user_defaults(pool: &PgPool) -> Result<UserDefaultsSettings> {
    let value: Option<JsonValue> = sqlx::query_scalar(
        "SELECT value FROM system_settings WHERE key = 'user_defaults'",
    )
    .fetch_optional(pool)
    .await
    .context("读取系统设置失败")?;

    match value {
        Some(v) => serde_json::from_value(v).context("解析 user_defaults 失败"),
        None => Ok(UserDefaultsSettings::default()),
    }
}

pub async fn set_user_defaults(
    pool: &PgPool,
    settings: &UserDefaultsSettings,
) -> Result<UserDefaultsSettings> {
    let value = serde_json::to_value(settings).context("序列化 user_defaults 失败")?;
    sqlx::query(
        r#"
        INSERT INTO system_settings (key, value, updated_at)
        VALUES ('user_defaults', $1, NOW())
        ON CONFLICT (key) DO UPDATE
        SET value = EXCLUDED.value, updated_at = NOW()
        "#,
    )
    .bind(value)
    .execute(pool)
    .await
    .context("保存系统设置失败")?;
    Ok(settings.clone())
}

pub async fn list_mqtt_topics(pool: &PgPool) -> Result<Vec<MqttTopicRow>> {
    let rows = sqlx::query_as::<_, MqttTopicRow>(
        "SELECT id, topic, description, owner_username, created_at FROM mqtt_topics ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .context("查询 mqtt_topics 失败")?;
    Ok(rows)
}

pub async fn list_mqtt_topics_for_user(
    pool: &PgPool,
    username: &str,
) -> Result<Vec<MqttTopicRow>> {
    let rows = sqlx::query_as::<_, MqttTopicRow>(
        r#"
        SELECT id, topic, description, owner_username, created_at
        FROM mqtt_topics
        WHERE owner_username = $1
        ORDER BY id
        "#,
    )
    .bind(username)
    .fetch_all(pool)
    .await
    .context("查询用户 Topic 失败")?;
    Ok(rows)
}

pub async fn create_mqtt_topic(
    pool: &PgPool,
    topic: &str,
    description: &str,
    owner_username: Option<&str>,
) -> Result<MqttTopicRow> {
    let row = sqlx::query_as::<_, MqttTopicRow>(
        r#"
        INSERT INTO mqtt_topics (topic, description, owner_username)
        VALUES ($1, $2, $3)
        RETURNING id, topic, description, owner_username, created_at
        "#,
    )
    .bind(topic)
    .bind(description)
    .bind(owner_username)
    .fetch_one(pool)
    .await
    .context("创建 mqtt_topic 失败")?;
    Ok(row)
}

pub async fn update_mqtt_topic(
    pool: &PgPool,
    id: i32,
    topic: &str,
    description: &str,
    owner_username: Option<&str>,
) -> Result<Option<MqttTopicRow>> {
    let row = sqlx::query_as::<_, MqttTopicRow>(
        r#"
        UPDATE mqtt_topics
        SET topic = $2, description = $3, owner_username = $4
        WHERE id = $1
        RETURNING id, topic, description, owner_username, created_at
        "#,
    )
    .bind(id)
    .bind(topic)
    .bind(description)
    .bind(owner_username)
    .fetch_optional(pool)
    .await
    .context("更新 mqtt_topic 失败")?;
    Ok(row)
}

pub async fn delete_mqtt_topic(pool: &PgPool, id: i32) -> Result<bool> {
    let result = sqlx::query("DELETE FROM mqtt_topics WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .context("删除 mqtt_topic 失败")?;
    Ok(result.rows_affected() > 0)
}

pub async fn list_topic_acls(pool: &PgPool) -> Result<Vec<AclRuleRow>> {
    let rows = sqlx::query_as::<_, AclRuleRow>(
        r#"
        SELECT id, username, topic_pattern, can_subscribe, can_publish, created_at
        FROM topic_acl ORDER BY id
        "#,
    )
    .fetch_all(pool)
    .await
    .context("查询 topic_acl 失败")?;
    Ok(rows)
}

pub async fn list_topic_acls_for_user(
    pool: &PgPool,
    username: &str,
) -> Result<Vec<AclRuleRow>> {
    let rows = sqlx::query_as::<_, AclRuleRow>(
        r#"
        SELECT id, username, topic_pattern, can_subscribe, can_publish, created_at
        FROM topic_acl
        WHERE username = $1
        ORDER BY id
        "#,
    )
    .bind(username)
    .fetch_all(pool)
    .await
    .context("查询用户 ACL 失败")?;
    Ok(rows)
}

pub async fn create_topic_acl(
    pool: &PgPool,
    username: &str,
    topic_pattern: &str,
    can_subscribe: bool,
    can_publish: bool,
) -> Result<AclRuleRow> {
    let row = sqlx::query_as::<_, AclRuleRow>(
        r#"
        INSERT INTO topic_acl (username, topic_pattern, can_subscribe, can_publish)
        VALUES ($1, $2, $3, $4)
        RETURNING id, username, topic_pattern, can_subscribe, can_publish, created_at
        "#,
    )
    .bind(username)
    .bind(topic_pattern)
    .bind(can_subscribe)
    .bind(can_publish)
    .fetch_one(pool)
    .await
    .context("创建 topic_acl 失败")?;
    Ok(row)
}

pub async fn update_topic_acl(
    pool: &PgPool,
    id: i32,
    topic_pattern: &str,
    can_subscribe: bool,
    can_publish: bool,
) -> Result<Option<AclRuleRow>> {
    let row = sqlx::query_as::<_, AclRuleRow>(
        r#"
        UPDATE topic_acl
        SET topic_pattern = $2, can_subscribe = $3, can_publish = $4
        WHERE id = $1
        RETURNING id, username, topic_pattern, can_subscribe, can_publish, created_at
        "#,
    )
    .bind(id)
    .bind(topic_pattern)
    .bind(can_subscribe)
    .bind(can_publish)
    .fetch_optional(pool)
    .await
    .context("更新 topic_acl 失败")?;
    Ok(row)
}

pub async fn delete_topic_acl(pool: &PgPool, id: i32) -> Result<bool> {
    let result = sqlx::query("DELETE FROM topic_acl WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .context("删除 topic_acl 失败")?;
    Ok(result.rows_affected() > 0)
}
