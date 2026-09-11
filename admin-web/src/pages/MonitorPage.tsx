import { useEffect, useRef, useState } from "react";
import {
  api,
  ConnectionInfo,
  MessageRecord,
  MonitorStats,
  SubscriptionInfo,
  SystemSnapshot,
  getToken,
} from "../api";
import { DetailModal } from "../components/DetailModal";
import {
  IconClock,
  IconConnection,
  IconCpu,
  IconDisk,
  IconMemory,
  IconMessage,
  IconSubscribe,
} from "../components/Icons";

function fmtTime(ts: number) {
  return new Date(ts).toLocaleString("zh-CN", { hour12: false });
}

function fmtBytes(n: number) {
  if (!n || n <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = n;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i += 1;
  }
  return `${v.toFixed(v >= 10 || i === 0 ? 0 : 1)} ${units[i]}`;
}

function fmtUptime(secs: number) {
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  if (d > 0) return `${d}天 ${h}小时`;
  if (h > 0) return `${h}小时 ${m}分`;
  return `${m} 分钟`;
}

function usageClass(pct: number) {
  if (pct >= 90) return "danger";
  if (pct >= 70) return "warning";
  return "ok";
}

type DetailKind = "connections" | "subscriptions" | "messages_all" | "messages_recent" | null;

export function MonitorPage() {
  const [stats, setStats] = useState<MonitorStats | null>(null);
  const [system, setSystem] = useState<SystemSnapshot | null>(null);
  const [messages, setMessages] = useState<MessageRecord[]>([]);
  const [live, setLive] = useState(false);
  const [autoScroll, setAutoScroll] = useState(true);
  const [detail, setDetail] = useState<DetailKind>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [connections, setConnections] = useState<ConnectionInfo[]>([]);
  const [subscriptions, setSubscriptions] = useState<SubscriptionInfo[]>([]);
  const [detailMessages, setDetailMessages] = useState<MessageRecord[]>([]);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let cancelled = false;

    async function bootstrap() {
      const [s, msgs, sys] = await Promise.all([
        api.monitorStats(),
        api.monitorMessages({ limit: 200 }),
        api.monitorSystem(),
      ]);
      if (cancelled) return;
      setStats(s);
      setMessages(msgs);
      setSystem(sys);
    }

    bootstrap().catch(() => {
      if (!cancelled) setLive(false);
    });

    const token = getToken();
    if (!token) return;

    const es = new EventSource(
      `/api/monitor/events?token=${encodeURIComponent(token)}`,
    );

    es.onopen = () => setLive(true);
    es.onerror = () => setLive(false);
    es.onmessage = (ev) => {
      const data = JSON.parse(ev.data);
      if (data.type === "stats") setStats(data.stats);
      if (data.type === "message") {
        setMessages((prev) => [data.message, ...prev].slice(0, 200));
      }
    };

    const sysTimer = window.setInterval(() => {
      api.monitorSystem().then(setSystem).catch(() => {});
    }, 3000);

    return () => {
      cancelled = true;
      es.close();
      window.clearInterval(sysTimer);
    };
  }, []);

  useEffect(() => {
    if (autoScroll && scrollRef.current) {
      scrollRef.current.scrollTop = 0;
    }
  }, [messages, autoScroll]);

  async function openDetail(kind: DetailKind) {
    if (!kind) return;
    setDetail(kind);
    setDetailLoading(true);
    try {
      if (kind === "connections") {
        setConnections(await api.monitorConnections());
      } else if (kind === "subscriptions") {
        setSubscriptions(await api.monitorSubscriptions());
      } else if (kind === "messages_all") {
        setDetailMessages(await api.monitorMessages({ limit: 500 }));
      } else if (kind === "messages_recent") {
        setDetailMessages(await api.monitorMessages({ recent: true, limit: 500 }));
      }
    } finally {
      setDetailLoading(false);
    }
  }

  const detailTitle =
    detail === "connections"
      ? "当前连接详情"
      : detail === "subscriptions"
        ? "订阅详情"
        : detail === "messages_all"
          ? "累计消息完整列表"
          : detail === "messages_recent"
            ? "近 1 分钟消息"
            : "";

  return (
    <>
      <div className="monitor-toolbar">
        <span className={`live-badge${live ? "" : " off"}`}>
          <span className="live-dot" />
          {live ? "SSE 已连接" : "SSE 断开"}
        </span>
      </div>

      <div className="panel system-panel">
        <div className="panel-head">
          <h3>系统资源</h3>
          <span className="system-meta">
            {system?.hostname || "-"} · 运行 {fmtUptime(system?.uptime_secs ?? 0)} · 负载{" "}
            {(system?.load_avg_1 ?? 0).toFixed(2)} / {(system?.load_avg_5 ?? 0).toFixed(2)} /{" "}
            {(system?.load_avg_15 ?? 0).toFixed(2)}
          </span>
        </div>
        <div className="system-grid">
          <div className="resource-card">
            <div className="resource-head">
              <span className="stat-icon primary">
                <IconCpu />
              </span>
              <div>
                <div className="stat-label">CPU</div>
                <div className="stat-value">{(system?.cpu_usage ?? 0).toFixed(1)}%</div>
              </div>
            </div>
            <div className={`usage-bar ${usageClass(system?.cpu_usage ?? 0)}`}>
              <span style={{ width: `${Math.min(100, system?.cpu_usage ?? 0)}%` }} />
            </div>
            <div className="resource-foot">
              {system?.cpu_cores ?? 0} 核 · 进程 {(system?.process_cpu ?? 0).toFixed(1)}%
            </div>
            {!!system?.cpu_per_core?.length && (
              <div className="core-bars">
                {system.cpu_per_core.map((pct, idx) => (
                  <div key={idx} className="core-bar" title={`CPU${idx}: ${pct.toFixed(1)}%`}>
                    <span style={{ height: `${Math.min(100, pct)}%` }} />
                  </div>
                ))}
              </div>
            )}
          </div>

          <div className="resource-card">
            <div className="resource-head">
              <span className="stat-icon success">
                <IconMemory />
              </span>
              <div>
                <div className="stat-label">内存</div>
                <div className="stat-value">{(system?.memory_usage ?? 0).toFixed(1)}%</div>
              </div>
            </div>
            <div className={`usage-bar ${usageClass(system?.memory_usage ?? 0)}`}>
              <span style={{ width: `${Math.min(100, system?.memory_usage ?? 0)}%` }} />
            </div>
            <div className="resource-foot">
              {fmtBytes(system?.memory_used_bytes ?? 0)} / {fmtBytes(system?.memory_total_bytes ?? 0)}
              {" · "}
              进程 {fmtBytes(system?.process_memory_bytes ?? 0)}
            </div>
            {(system?.swap_total_bytes ?? 0) > 0 && (
              <div className="resource-foot muted">
                Swap {fmtBytes(system?.swap_used_bytes ?? 0)} / {fmtBytes(system?.swap_total_bytes ?? 0)}
              </div>
            )}
          </div>

          <div className="resource-card">
            <div className="resource-head">
              <span className="stat-icon warning">
                <IconDisk />
              </span>
              <div>
                <div className="stat-label">磁盘</div>
                <div className="stat-value">{(system?.disk_usage ?? 0).toFixed(1)}%</div>
              </div>
            </div>
            <div className={`usage-bar ${usageClass(system?.disk_usage ?? 0)}`}>
              <span style={{ width: `${Math.min(100, system?.disk_usage ?? 0)}%` }} />
            </div>
            <div className="resource-foot">
              {fmtBytes(system?.disk_used_bytes ?? 0)} / {fmtBytes(system?.disk_total_bytes ?? 0)}
            </div>
          </div>
        </div>
      </div>

      <section className="stat-grid">
        <button
          type="button"
          className="stat-card-valex"
          title="查看连接列表"
          onClick={() => openDetail("connections")}
        >
          <div className="stat-icon primary">
            <IconConnection />
          </div>
          <div>
            <div className="stat-label">当前连接</div>
            <div className="stat-value">{stats?.total_connections ?? 0}</div>
          </div>
        </button>
        <button
          type="button"
          className="stat-card-valex"
          title="查看订阅列表"
          onClick={() => openDetail("subscriptions")}
        >
          <div className="stat-icon success">
            <IconSubscribe />
          </div>
          <div>
            <div className="stat-label">活跃订阅</div>
            <div className="stat-value">{stats?.total_subscriptions ?? 0}</div>
          </div>
        </button>
        <button
          type="button"
          className="stat-card-valex"
          title="查看全部消息"
          onClick={() => openDetail("messages_all")}
        >
          <div className="stat-icon warning">
            <IconMessage />
          </div>
          <div>
            <div className="stat-label">累计消息</div>
            <div className="stat-value">{stats?.total_messages ?? 0}</div>
          </div>
        </button>
        <button
          type="button"
          className="stat-card-valex"
          title="查看近 1 分钟消息"
          onClick={() => openDetail("messages_recent")}
        >
          <div className="stat-icon info">
            <IconClock />
          </div>
          <div>
            <div className="stat-label">近 1 分钟</div>
            <div className="stat-value">{stats?.messages_last_minute ?? 0}</div>
          </div>
        </button>
      </section>

      <section className="monitor-layout">
        <div className="panel">
          <div className="panel-head">
            <h3>实时消息</h3>
            <div className="actions">
              <label className="checkbox-row">
                <input
                  type="checkbox"
                  checked={autoScroll}
                  onChange={(e) => setAutoScroll(e.target.checked)}
                />
                自动滚动
              </label>
              <button
                type="button"
                className="btn secondary sm"
                onClick={() => setMessages([])}
              >
                清空视图
              </button>
            </div>
          </div>
          <div className="scroll-box" ref={scrollRef}>
            <table>
              <thead>
                <tr>
                  <th>时间</th>
                  <th>Topic</th>
                  <th>Payload</th>
                  <th>大小</th>
                </tr>
              </thead>
              <tbody>
                {messages.length === 0 ? (
                  <tr>
                    <td colSpan={4} className="empty">
                      暂无消息
                    </td>
                  </tr>
                ) : (
                  messages.map((msg) => (
                    <tr key={msg.id}>
                      <td>{fmtTime(Number(msg.ts))}</td>
                      <td className="topic">{msg.topic}</td>
                      <td className="payload">{msg.payload}</td>
                      <td>{msg.payload_bytes} B</td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        </div>

        <div className="panel">
          <div className="panel-head">
            <h3>热门 Topic</h3>
          </div>
          <ul className="topics-list">
            {!stats?.top_topics?.length ? (
              <li className="empty">暂无数据</li>
            ) : (
              stats.top_topics.map((item) => (
                <li key={item.topic}>
                  <span className="topic">{item.topic}</span>
                  <span>{item.count}</span>
                </li>
              ))
            )}
          </ul>
        </div>
      </section>

      <DetailModal
        title={detailTitle}
        open={detail !== null}
        onClose={() => setDetail(null)}
        wide
      >
        {detailLoading ? (
          <p className="empty">加载中…</p>
        ) : detail === "connections" ? (
          <table>
            <thead>
              <tr>
                <th>ID</th>
                <th>Client ID</th>
                <th>用户名</th>
                <th>Clean</th>
                <th>订阅 Topic</th>
              </tr>
            </thead>
            <tbody>
              {connections.length === 0 ? (
                <tr>
                  <td colSpan={5} className="empty">
                    暂无连接
                  </td>
                </tr>
              ) : (
                connections.map((c) => (
                  <tr key={c.connection_id}>
                    <td>{c.connection_id}</td>
                    <td>{c.client_id}</td>
                    <td>{c.username || "-"}</td>
                    <td>{c.clean ? "是" : "否"}</td>
                    <td className="topic">
                      {c.subscriptions.length ? c.subscriptions.join(", ") : "-"}
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        ) : detail === "subscriptions" ? (
          <table>
            <thead>
              <tr>
                <th>Topic Filter</th>
                <th>订阅客户端</th>
                <th>数量</th>
              </tr>
            </thead>
            <tbody>
              {subscriptions.length === 0 ? (
                <tr>
                  <td colSpan={3} className="empty">
                    暂无订阅
                  </td>
                </tr>
              ) : (
                subscriptions.map((s) => (
                  <tr key={s.filter}>
                    <td className="topic">{s.filter}</td>
                    <td>{s.subscribers.join(", ") || "-"}</td>
                    <td>{s.subscribers.length}</td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        ) : (
          <div className="scroll-box modal-scroll">
            <table>
              <thead>
                <tr>
                  <th>ID</th>
                  <th>时间</th>
                  <th>Topic</th>
                  <th>Payload</th>
                  <th>大小</th>
                </tr>
              </thead>
              <tbody>
                {detailMessages.length === 0 ? (
                  <tr>
                    <td colSpan={5} className="empty">
                      暂无消息
                    </td>
                  </tr>
                ) : (
                  detailMessages.map((msg) => (
                    <tr key={msg.id}>
                      <td>{msg.id}</td>
                      <td>{fmtTime(Number(msg.ts))}</td>
                      <td className="topic">{msg.topic}</td>
                      <td className="payload">{msg.payload}</td>
                      <td>{msg.payload_bytes} B</td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        )}
      </DetailModal>
    </>
  );
}
