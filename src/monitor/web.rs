use std::convert::Infallible;

use axum::{
    extract::State,
    response::{
        sse::{Event, KeepAlive, Sse},
        Html,
    },
    routing::get,
    Json, Router,
};
use futures_util::stream::{self, Stream};
use tokio::sync::broadcast;

use super::{MessageRecord, MonitorEvent, MonitorStats, SharedMonitor};

pub fn router(state: SharedMonitor) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/api/stats", get(stats))
        .route("/api/messages", get(messages))
        .route("/api/events", get(events))
        .with_state(state)
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../../static/monitor.html"))
}

async fn stats(State(state): State<SharedMonitor>) -> Json<MonitorStats> {
    Json(state.stats.read().await.clone())
}

async fn messages(State(state): State<SharedMonitor>) -> Json<Vec<MessageRecord>> {
    Json(state.messages.read().await.iter().cloned().collect())
}

async fn events(
    State(state): State<SharedMonitor>,
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
