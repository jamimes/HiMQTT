use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{postgres::PgPoolOptions, PgPool};

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct AdminUserRow {
    pub id: i32,
    pub username: String,
    pub password_hash: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct MqttUserRow {
    pub id: i32,
    pub username: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct MqttTopicRow {
    pub id: i32,
    pub topic: String,
    pub description: String,
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
    sqlx::query(
        "INSERT INTO admin_users (username, password_hash) VALUES ($1, $2)",
    )
    .bind(username)
    .bind(password_hash)
    .execute(pool)
    .await
    .context("创建 admin 用户失败")?;
    Ok(())
}

pub async fn list_mqtt_users(pool: &PgPool) -> Result<Vec<MqttUserRow>> {
    let rows = sqlx::query_as::<_, MqttUserRow>(
        "SELECT id, username, password_hash, enabled, created_at, updated_at FROM mqtt_users ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .context("查询 mqtt_users 失败")?;
    Ok(rows)
}

pub async fn get_mqtt_user(pool: &PgPool, username: &str) -> Result<Option<MqttUserRow>> {
    let row = sqlx::query_as::<_, MqttUserRow>(
        "SELECT id, username, password_hash, enabled, created_at, updated_at FROM mqtt_users WHERE username = $1",
    )
    .bind(username)
    .fetch_optional(pool)
    .await
    .context("查询 mqtt_user 失败")?;
    Ok(row)
}

pub async fn create_mqtt_user(
    pool: &PgPool,
    username: &str,
    password_hash: &str,
    enabled: bool,
) -> Result<MqttUserRow> {
    let row = sqlx::query_as::<_, MqttUserRow>(
        r#"
        INSERT INTO mqtt_users (username, password_hash, enabled)
        VALUES ($1, $2, $3)
        RETURNING id, username, password_hash, enabled, created_at, updated_at
        "#,
    )
    .bind(username)
    .bind(password_hash)
    .bind(enabled)
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
) -> Result<Option<MqttUserRow>> {
    let row = if let Some(hash) = password_hash {
        sqlx::query_as::<_, MqttUserRow>(
            r#"
            UPDATE mqtt_users
            SET password_hash = $2, enabled = $3, updated_at = NOW()
            WHERE id = $1
            RETURNING id, username, password_hash, enabled, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(hash)
        .bind(enabled)
        .fetch_optional(pool)
        .await
    } else {
        sqlx::query_as::<_, MqttUserRow>(
            r#"
            UPDATE mqtt_users
            SET enabled = $2, updated_at = NOW()
            WHERE id = $1
            RETURNING id, username, password_hash, enabled, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(enabled)
        .fetch_optional(pool)
        .await
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

pub async fn list_mqtt_topics(pool: &PgPool) -> Result<Vec<MqttTopicRow>> {
    let rows = sqlx::query_as::<_, MqttTopicRow>(
        "SELECT id, topic, description, created_at FROM mqtt_topics ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .context("查询 mqtt_topics 失败")?;
    Ok(rows)
}

pub async fn create_mqtt_topic(
    pool: &PgPool,
    topic: &str,
    description: &str,
) -> Result<MqttTopicRow> {
    let row = sqlx::query_as::<_, MqttTopicRow>(
        r#"
        INSERT INTO mqtt_topics (topic, description)
        VALUES ($1, $2)
        RETURNING id, topic, description, created_at
        "#,
    )
    .bind(topic)
    .bind(description)
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
) -> Result<Option<MqttTopicRow>> {
    let row = sqlx::query_as::<_, MqttTopicRow>(
        r#"
        UPDATE mqtt_topics SET topic = $2, description = $3 WHERE id = $1
        RETURNING id, topic, description, created_at
        "#,
    )
    .bind(id)
    .bind(topic)
    .bind(description)
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
