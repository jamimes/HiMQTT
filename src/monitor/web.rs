use std::convert::Infallible;

use axum::{
    extract::Query,
    response::{
        sse::{Event, KeepAlive, Sse},
        Html,
    },
    Json,
};
use futures_util::stream::{self, Stream};
use serde::Deserialize;
use tokio::sync::broadcast;

use super::{
    ConnectionSnapshot, MessageRecord, MonitorEvent, MonitorStats, SharedMonitor,
    SubscriptionSnapshot,
};

#[derive(Deserialize)]
pub struct MessagesQuery {
    #[serde(default)]
    recent: bool,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    500
}

pub async fn stats(state: &SharedMonitor) -> Json<MonitorStats> {
    Json(state.stats.read().await.clone())
}

pub async fn messages(
    state: &SharedMonitor,
    Query(query): Query<MessagesQuery>,
) -> Json<Vec<MessageRecord>> {
    Json(state.list_messages(query.recent, query.limit).await)
}

pub async fn connections(state: &SharedMonitor) -> Json<Vec<ConnectionSnapshot>> {
    Json(state.connections.read().await.clone())
}

pub async fn subscriptions(state: &SharedMonitor) -> Json<Vec<SubscriptionSnapshot>> {
    Json(state.subscriptions.read().await.clone())
}

pub async fn events(
    state: &SharedMonitor,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.tx.subscribe();
    let stream = stream::unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(MonitorEvent::Message { message }) => {
                    let json = serde_json::json!({ "type": "message", "message": message });
                    return Some((Ok(Event::default().data(json.to_string())), rx));
                }
                Ok(MonitorEvent::Stats { stats }) => {
                    let json = serde_json::json!({ "type": "stats", "stats": stats });
                    return Some((Ok(Event::default().data(json.to_string())), rx));
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => return None,
            }
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[allow(dead_code)]
pub async fn index() -> Html<&'static str> {
    Html(include_str!("../../static/monitor.html"))
}
