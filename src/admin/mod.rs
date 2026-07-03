mod auth;

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use bcrypt::{hash, verify, DEFAULT_COST};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tracing::info;

use crate::acl::AclService;
use crate::db;
use crate::monitor::{self, SharedMonitor};
use crate::monitor::web as monitor_web;

pub use auth::AdminAuth;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AdminConfig {
    #[serde(default = "default_admin_listen")]
    pub listen: String,
    #[serde(default = "default_admin_password")]
    pub default_password: String,
    #[serde(default = "default_static_dir")]
    pub static_dir: String,
}

fn default_admin_listen() -> String {
    "127.0.0.1:8091".to_owned()
}

fn default_admin_password() -> String {
    "admin123".to_owned()
}

fn default_static_dir() -> String {
    "admin-web/dist".to_owned()
}

impl Default for AdminConfig {
    fn default() -> Self {
        Self {
            listen: default_admin_listen(),
            default_password: default_admin_password(),
            static_dir: default_static_dir(),
        }
    }
}

#[derive(Clone)]
struct AppState {
    acl: Arc<AclService>,
    auth: AdminAuth,
    monitor: Option<SharedMonitor>,
}

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct LoginResponse {
    token: String,
    username: String,
}

#[derive(Deserialize)]
struct CreateMqttUserRequest {
    username: String,
    password: String,
    #[serde(default = "default_enabled")]
    enabled: bool,
}

#[derive(Deserialize)]
struct UpdateMqttUserRequest {
    password: Option<String>,
    #[serde(default = "default_enabled")]
    enabled: bool,
}

#[derive(Deserialize)]
struct CreateTopicRequest {
    topic: String,
    #[serde(default)]
    description: String,
}

#[derive(Deserialize)]
struct UpdateTopicRequest {
    topic: String,
    #[serde(default)]
    description: String,
}

#[derive(Deserialize)]
struct CreateAclRequest {
    username: String,
    topic_pattern: String,
    #[serde(default)]
    can_subscribe: bool,
    #[serde(default)]
    can_publish: bool,
}

#[derive(Deserialize)]
struct UpdateAclRequest {
    topic_pattern: String,
    #[serde(default)]
    can_subscribe: bool,
    #[serde(default)]
    can_publish: bool,
}

fn default_enabled() -> bool {
    true
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_owned)
}

fn token_from_query(uri: &axum::http::Uri) -> Option<String> {
    uri.query().and_then(|query| {
        query.split('&').find_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            if key == "token" {
                Some(value.to_owned())
            } else {
                None
            }
        })
    })
}

async fn require_auth(
    State(state): State<AppState>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let token = bearer_token(request.headers())
        .or_else(|| token_from_query(request.uri()));
    let Some(token) = token else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if state.auth.validate(&token).is_none() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    next.run(request).await
}

async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, StatusCode> {
    let user = db::get_admin_user(state.acl.pool(), &body.username)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let Some(user) = user else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    if !verify(&body.password, &user.password_hash).unwrap_or(false) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let token = state.auth.create_session(&user.username);
    Ok(Json(LoginResponse {
        token,
        username: user.username,
    }))
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> StatusCode {
    if let Some(token) = bearer_token(&headers) {
        state.auth.revoke(&token);
    }
    StatusCode::NO_CONTENT
}

async fn me(State(state): State<AppState>, headers: HeaderMap) -> Result<Json<LoginResponse>, StatusCode> {
    let token = bearer_token(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    let username = state.auth.validate(&token).ok_or(StatusCode::UNAUTHORIZED)?;
    Ok(Json(LoginResponse { token, username }))
}

async fn list_mqtt_users(State(state): State<AppState>) -> Result<Json<Vec<db::MqttUserRow>>, StatusCode> {
    db::list_mqtt_users(state.acl.pool())
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn create_mqtt_user(
    State(state): State<AppState>,
    Json(body): Json<CreateMqttUserRequest>,
) -> Result<(StatusCode, Json<db::MqttUserRow>), StatusCode> {
    let password_hash = hash(&body.password, DEFAULT_COST).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let user = db::create_mqtt_user(state.acl.pool(), &body.username, &password_hash, body.enabled)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    state.acl.reload().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(user)))
}

async fn update_mqtt_user(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Json(body): Json<UpdateMqttUserRequest>,
) -> Result<Json<db::MqttUserRow>, StatusCode> {
    let password_hash = match body.password {
        Some(p) if !p.is_empty() => Some(
            hash(&p, DEFAULT_COST).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        ),
        _ => None,
    };
    let user = db::update_mqtt_user(
        state.acl.pool(),
        id,
        password_hash.as_deref(),
        body.enabled,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;
    state.acl.reload().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(user))
}

async fn delete_mqtt_user(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<StatusCode, StatusCode> {
    let deleted = db::delete_mqtt_user(state.acl.pool(), id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !deleted {
        return Err(StatusCode::NOT_FOUND);
    }
    state.acl.reload().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_topics(State(state): State<AppState>) -> Result<Json<Vec<db::MqttTopicRow>>, StatusCode> {
    db::list_mqtt_topics(state.acl.pool())
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn create_topic(
    State(state): State<AppState>,
    Json(body): Json<CreateTopicRequest>,
) -> Result<(StatusCode, Json<db::MqttTopicRow>), StatusCode> {
    let topic = db::create_mqtt_topic(state.acl.pool(), &body.topic, &body.description)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    state.acl.reload().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(topic)))
}

async fn update_topic(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Json(body): Json<UpdateTopicRequest>,
) -> Result<Json<db::MqttTopicRow>, StatusCode> {
    let topic = db::update_mqtt_topic(state.acl.pool(), id, &body.topic, &body.description)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    state.acl.reload().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(topic))
}

async fn delete_topic(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<StatusCode, StatusCode> {
    let deleted = db::delete_mqtt_topic(state.acl.pool(), id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !deleted {
        return Err(StatusCode::NOT_FOUND);
    }
    state.acl.reload().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_acls(State(state): State<AppState>) -> Result<Json<Vec<db::AclRuleRow>>, StatusCode> {
    db::list_topic_acls(state.acl.pool())
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn create_acl(
    State(state): State<AppState>,
    Json(body): Json<CreateAclRequest>,
) -> Result<(StatusCode, Json<db::AclRuleRow>), StatusCode> {
    let rule = db::create_topic_acl(
        state.acl.pool(),
        &body.username,
        &body.topic_pattern,
        body.can_subscribe,
        body.can_publish,
    )
    .await
    .map_err(|_| StatusCode::BAD_REQUEST)?;
    state.acl.reload().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(rule)))
}

async fn update_acl(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Json(body): Json<UpdateAclRequest>,
) -> Result<Json<db::AclRuleRow>, StatusCode> {
    let rule = db::update_topic_acl(
        state.acl.pool(),
        id,
        &body.topic_pattern,
        body.can_subscribe,
        body.can_publish,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;
    state.acl.reload().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rule))
}

async fn delete_acl(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<StatusCode, StatusCode> {
    let deleted = db::delete_topic_acl(state.acl.pool(), id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !deleted {
        return Err(StatusCode::NOT_FOUND);
    }
    state.acl.reload().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn reload_acl(State(state): State<AppState>) -> Result<StatusCode, StatusCode> {
    state.acl.reload().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn monitor_stats(State(state): State<AppState>) -> Result<Json<monitor::MonitorStats>, StatusCode> {
    let monitor = state.monitor.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(monitor_web::stats(monitor).await)
}

async fn monitor_messages(
    State(state): State<AppState>,
    query: Query<monitor_web::MessagesQuery>,
) -> Result<Json<Vec<monitor::MessageRecord>>, StatusCode> {
    let monitor = state.monitor.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(monitor_web::messages(monitor, query).await)
}

async fn monitor_connections(
    State(state): State<AppState>,
) -> Result<Json<Vec<monitor::ConnectionSnapshot>>, StatusCode> {
    let monitor = state.monitor.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(monitor_web::connections(monitor).await)
}

async fn monitor_subscriptions(
    State(state): State<AppState>,
) -> Result<Json<Vec<monitor::SubscriptionSnapshot>>, StatusCode> {
    let monitor = state.monitor.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(monitor_web::subscriptions(monitor).await)
}

async fn monitor_events(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, StatusCode> {
    let monitor = state.monitor.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(monitor_web::events(monitor).await)
}

pub async fn serve(
    acl: Arc<AclService>,
    cfg: AdminConfig,
    monitor: Option<SharedMonitor>,
) -> Result<()> {
    let state = AppState {
        acl,
        auth: AdminAuth::new(),
        monitor,
    };

    let mut protected = Router::new()
        .route("/api/auth/me", get(me))
        .route("/api/auth/logout", post(logout))
        .route("/api/mqtt-users", get(list_mqtt_users).post(create_mqtt_user))
        .route(
            "/api/mqtt-users/:id",
            put(update_mqtt_user).delete(delete_mqtt_user),
        )
        .route("/api/topics", get(list_topics).post(create_topic))
        .route("/api/topics/:id", put(update_topic).delete(delete_topic))
        .route("/api/acls", get(list_acls).post(create_acl))
        .route("/api/acls/:id", put(update_acl).delete(delete_acl))
        .route("/api/acl/reload", post(reload_acl));

    if state.monitor.is_some() {
        protected = protected
            .route("/api/monitor/stats", get(monitor_stats))
            .route("/api/monitor/messages", get(monitor_messages))
            .route("/api/monitor/connections", get(monitor_connections))
            .route("/api/monitor/subscriptions", get(monitor_subscriptions))
            .route("/api/monitor/events", get(monitor_events));
    }

    let protected = protected
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth))
        .with_state(state.clone());

    let app = Router::new()
        .route("/api/auth/login", post(login))
        .merge(protected)
        .fallback_service(ServeDir::new(&cfg.static_dir))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state);

    let addr: SocketAddr = cfg.listen.parse().context("admin listen 地址无效")?;
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("绑定 admin 端口 {addr} 失败"))?;
    info!("管理后台已启动: http://{addr}（含连接监控）");
    axum::serve(listener, app)
        .await
        .context("admin 服务异常退出")?;
    Ok(())
}
