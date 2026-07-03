pub mod web;

use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use rumqttd::local::{LinkError, LinkRx};
use rumqttd::meters::MetersLink;
use rumqttd::{Meter, Notification};
pub use rumqttd::{ConnectionSnapshot, SubscriptionSnapshot};
use serde::Serialize;
use tokio::sync::{broadcast, RwLock};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct MonitorConfig {
    #[serde(default = "default_listen")]
    pub listen: SocketAddr,
    #[serde(default = "default_max_messages")]
    pub max_messages: usize,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            listen: default_listen(),
            max_messages: default_max_messages(),
        }
    }
}

fn default_listen() -> SocketAddr {
    "127.0.0.1:8090".parse().expect("valid addr")
}

fn default_max_messages() -> usize {
    500
}

#[derive(Clone, Serialize)]
pub struct MessageRecord {
    pub id: u64,
    pub ts: u128,
    pub topic: String,
    pub payload: String,
    pub payload_bytes: usize,
    pub qos: u8,
    pub retain: bool,
}

#[derive(Clone, Serialize, Default)]
pub struct MonitorStats {
    pub total_connections: usize,
    pub total_subscriptions: usize,
    pub total_messages: u64,
    pub messages_last_minute: u64,
    pub top_topics: Vec<TopicCount>,
    pub updated_at: u128,
}

#[derive(Clone, Serialize)]
pub struct TopicCount {
    pub topic: String,
    pub count: usize,
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MonitorEvent {
    Message { message: MessageRecord },
    Stats { stats: MonitorStats },
}

pub struct MonitorState {
    messages: RwLock<VecDeque<MessageRecord>>,
    stats: RwLock<MonitorStats>,
    connections: RwLock<Vec<ConnectionSnapshot>>,
    subscriptions: RwLock<Vec<SubscriptionSnapshot>>,
    topic_counts: RwLock<HashMap<String, usize>>,
    recent_ts: RwLock<VecDeque<u128>>,
    message_total: AtomicU64,
    max_messages: usize,
    tx: broadcast::Sender<MonitorEvent>,
}

impl MonitorState {
    pub fn new(max_messages: usize) -> Arc<Self> {
        let (tx, _) = broadcast::channel(1024);
        Arc::new(Self {
            messages: RwLock::new(VecDeque::with_capacity(max_messages.min(1024))),
            stats: RwLock::new(MonitorStats::default()),
            connections: RwLock::new(Vec::new()),
            subscriptions: RwLock::new(Vec::new()),
            topic_counts: RwLock::new(HashMap::new()),
            recent_ts: RwLock::new(VecDeque::with_capacity(4096)),
            message_total: AtomicU64::new(0),
            max_messages,
            tx,
        })
    }

    pub fn spawn_collector(self: &Arc<Self>, mut link_rx: LinkRx, meters: MetersLink) {
        let state = Arc::clone(self);
        thread::Builder::new()
            .name("himqtt-monitor".into())
            .spawn(move || {
                if let Err(e) = collector_loop(state, &mut link_rx, &meters) {
                    tracing::error!("monitor collector stopped: {e}");
                }
            })
            .expect("spawn monitor collector");
    }

    pub async fn list_messages(&self, recent_only: bool, limit: usize) -> Vec<MessageRecord> {
        let limit = limit.min(self.max_messages).max(1);
        let cutoff = now_millis().saturating_sub(60_000);
        let messages = self.messages.read().await;
        let mut out: Vec<MessageRecord> = messages
            .iter()
            .rev()
            .filter(|msg| !recent_only || msg.ts >= cutoff)
            .take(limit)
            .cloned()
            .collect();
        out.sort_by(|a, b| b.ts.cmp(&a.ts));
        out
    }
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn payload_to_string(bytes: &[u8]) -> String {
    if bytes.len() > 4096 {
        let preview = String::from_utf8_lossy(&bytes[..4096]);
        return format!("{preview}… [{len} bytes]", len = bytes.len());
    }
    String::from_utf8_lossy(bytes).into_owned()
}

async fn push_message(state: &Arc<MonitorState>, topic: String, payload: Vec<u8>, qos: u8, retain: bool) {
    let id = state.message_total.fetch_add(1, Ordering::Relaxed) + 1;
    let ts = now_millis();
    let payload_bytes = payload.len();
    let record = MessageRecord {
        id,
        ts,
        topic: topic.clone(),
        payload: payload_to_string(&payload),
        payload_bytes,
        qos,
        retain,
    };

    {
        let mut messages = state.messages.write().await;
        if messages.len() >= state.max_messages {
            messages.pop_front();
        }
        messages.push_back(record.clone());
    }

    {
        let mut counts = state.topic_counts.write().await;
        *counts.entry(topic).or_insert(0) += 1;
    }

    {
        let mut recent = state.recent_ts.write().await;
        recent.push_back(ts);
        while recent.front().is_some_and(|t| *t < ts.saturating_sub(60_000)) {
            recent.pop_front();
        }
    }

    let _ = state.tx.send(MonitorEvent::Message {
        message: record,
    });
}

async fn refresh_stats(
    state: &Arc<MonitorState>,
    connections: &[ConnectionSnapshot],
    subscriptions: &[SubscriptionSnapshot],
) {
    let ts = now_millis();
    let messages_last_minute = state.recent_ts.read().await.len() as u64;
    let top_topics = {
        let counts = state.topic_counts.read().await;
        let mut items: Vec<_> = counts
            .iter()
            .map(|(topic, count)| TopicCount {
                topic: topic.clone(),
                count: *count,
            })
            .collect();
        items.sort_by(|a, b| b.count.cmp(&a.count));
        items.truncate(8);
        items
    };

    let total_subscriptions = connections.iter().map(|c| c.subscriptions.len()).sum();

    let stats = MonitorStats {
        total_connections: connections.len(),
        total_subscriptions,
        total_messages: state.message_total.load(Ordering::Relaxed),
        messages_last_minute,
        top_topics,
        updated_at: ts,
    };

    *state.stats.write().await = stats.clone();
    *state.connections.write().await = connections.to_vec();
    *state.subscriptions.write().await = subscriptions.to_vec();
    let _ = state.tx.send(MonitorEvent::Stats { stats });
}

fn collector_loop(
    state: Arc<MonitorState>,
    link_rx: &mut LinkRx,
    meters: &MetersLink,
) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("monitor tokio runtime")?;

    let mut connections = Vec::new();
    let mut subscriptions = Vec::new();
    let mut last_meter_poll = Instant::now();

    loop {
        if last_meter_poll.elapsed().as_secs() >= 1 {
            if let Ok(batch) = meters.recv() {
                for meter in batch {
                    match meter {
                        Meter::Connections(list) => connections = list,
                        Meter::Subscriptions(list) => subscriptions = list,
                        Meter::Router(_, router) if connections.is_empty() => {
                            let _ = router.total_connections;
                        }
                        _ => {}
                    }
                }
            }
            runtime.block_on(refresh_stats(&state, &connections, &subscriptions));
            last_meter_poll = Instant::now();
        }

        match link_rx.recv_deadline(Instant::now() + std::time::Duration::from_millis(200)) {
            Ok(Some(Notification::Forward(forward))) => {
                let publish = forward.publish;
                runtime.block_on(push_message(
                    &state,
                    String::from_utf8_lossy(&publish.topic).into_owned(),
                    publish.payload.to_vec(),
                    0,
                    publish.retain,
                ));
            }
            Ok(Some(_)) => {}
            Ok(None) => {}
            Err(LinkError::RecvTimeout(_)) => {}
            Err(e) => return Err(anyhow::anyhow!("monitor link error: {e}")),
        }
    }
}

pub type SharedMonitor = Arc<MonitorState>;
