#!/usr/bin/env python3
"""HiMQTT 压测：先建连，再统一开闸高吞吐 pub/sub。"""

from __future__ import annotations

import argparse
import json
import statistics
import sys
import threading
import time
import urllib.request
from dataclasses import dataclass, field

sys.path.insert(0, __file__.rsplit("/", 1)[0])
from mqtt_local_acl_test import mqtt_connect_v4_auth, mqtt_publish_v4  # noqa: E402
from mqtt_smoke_test import (  # noqa: E402
    TcpTransport,
    connack_ok,
    mqtt_pingreq,
    mqtt_subscribe_v4,
    parse_publish_payload,
)

HOST = "127.0.0.1"
PORT = 1883
USER = "stresstest"
PASS = "stress123"
CONNECT_TIMEOUT = 60.0


@dataclass
class Counters:
    lock: threading.Lock = field(default_factory=threading.Lock)
    published: int = 0
    received: int = 0
    publish_errors: int = 0
    latencies_ms: list[float] = field(default_factory=list)

    def add_latency(self, ms: float) -> None:
        with self.lock:
            self.latencies_ms.append(ms)
            if len(self.latencies_ms) > 8000:
                del self.latencies_ms[:4000]


def admin_login() -> str:
    data = json.dumps({"username": "admin", "password": "admin123"}).encode()
    req = urllib.request.Request(
        f"http://{HOST}:8091/api/auth/login",
        data=data,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=15) as resp:
        return json.loads(resp.read().decode())["token"]


def admin_get(path: str, token: str):
    req = urllib.request.Request(
        f"http://{HOST}:8091{path}",
        headers={"Authorization": f"Bearer {token}"},
    )
    with urllib.request.urlopen(req, timeout=10) as resp:
        return json.loads(resp.read().decode())


def connect(client_id: str) -> TcpTransport:
    last: Exception | None = None
    for attempt in range(1, 5):
        try:
            conn = TcpTransport(HOST, PORT)
            conn.sock.settimeout(CONNECT_TIMEOUT)
            conn.send_mqtt(mqtt_connect_v4_auth(client_id, USER, PASS))
            ptype, body = conn.recv_mqtt(timeout=CONNECT_TIMEOUT)
            if not connack_ok(ptype, body, "v4"):
                conn.close()
                raise RuntimeError(f"CONNACK fail type={ptype}")
            return conn
        except Exception as e:
            last = e
            time.sleep(min(3.0, 0.6 * attempt))
    raise RuntimeError(f"{client_id}: {last}")


def drain(conn: TcpTransport, timeout: float) -> int:
    count = 0
    deadline = time.time() + timeout
    while time.time() < deadline:
        remain = max(0.01, deadline - time.time())
        try:
            ptype, _body = conn.recv_mqtt(timeout=remain)
        except Exception:
            break
        if ptype == 3:
            count += 1
    return count


def pct(values: list[float], p: float) -> float:
    if not values:
        return 0.0
    s = sorted(values)
    i = min(len(s) - 1, max(0, int(round((p / 100.0) * (len(s) - 1)))))
    return s[i]


def main() -> int:
    ap = argparse.ArgumentParser(description="HiMQTT MQTT 压测")
    ap.add_argument("--publishers", type=int, default=10)
    ap.add_argument("--subscribers", type=int, default=5)
    ap.add_argument("--rate", type=float, default=100.0, help="每个 publisher 每秒消息数")
    ap.add_argument("--duration", type=float, default=20.0)
    ap.add_argument("--payload", type=int, default=128)
    ap.add_argument("--topic", default="stress/bench")
    args = ap.parse_args()

    expected = int(args.publishers * args.rate * args.duration)
    print("=" * 64)
    print("HiMQTT 压测（先建连，再开闸）")
    print(
        f"pubs={args.publishers} subs={args.subscribers} "
        f"rate={args.rate}/s/pub duration={args.duration}s payload={args.payload}B"
    )
    print(f"topic={args.topic}  理论消息≈{expected}")
    print("=" * 64)

    token = ""
    try:
        token = admin_login()
        before = admin_get("/api/monitor/system", token)
        print(
            f"[before] CPU {before.get('cpu_usage', 0):.1f}%  "
            f"MEM {before.get('memory_usage', 0):.1f}%  "
            f"RSS={before.get('process_memory_bytes', 0)/1024/1024:.1f}MB"
        )
    except Exception as e:
        print(f"[warn] 系统监控不可用: {e}")

    print("阶段1: 建立订阅连接…")
    subs: list[TcpTransport] = []
    for i in range(args.subscribers):
        conn = connect(f"stress-sub-{i}")
        conn.send_mqtt(mqtt_subscribe_v4(1, "stress/#", 0))
        conn.recv_mqtt(timeout=CONNECT_TIMEOUT)
        subs.append(conn)
        print(f"  sub-{i} ok")
        time.sleep(0.8)

    print("阶段1: 建立发布连接…")
    pubs: list[TcpTransport] = []
    for i in range(args.publishers):
        conn = connect(f"stress-pub-{i}")
        pubs.append(conn)
        print(f"  pub-{i} ok")
        time.sleep(0.5)

    print(f"已就绪: {len(pubs)} pubs + {len(subs)} subs，开始压测 {args.duration}s…")
    stop = threading.Event()
    go = threading.Event()
    counters = Counters()

    def sub_loop(conn: TcpTransport):
        go.wait(timeout=120)
        while not stop.is_set():
            n = drain(conn, 0.2)
            if n:
                with counters.lock:
                    counters.received += n

    def pub_loop(idx: int, conn: TcpTransport):
        go.wait(timeout=120)
        interval = 1.0 / args.rate if args.rate > 0 else 0.0
        pad = "x" * max(0, args.payload - 40)
        end = time.time() + args.duration
        n = 0
        next_t = time.perf_counter()
        while time.time() < end and not stop.is_set():
            n += 1
            t0 = time.perf_counter()
            body = f"{idx}:{n}:{t0:.6f}:{pad}"[: args.payload]
            try:
                conn.send_mqtt(mqtt_publish_v4(args.topic, body))
                with counters.lock:
                    counters.published += 1
                counters.add_latency((time.perf_counter() - t0) * 1000)
            except Exception:
                with counters.lock:
                    counters.publish_errors += 1
            if interval > 0:
                next_t += interval
                sleep_for = next_t - time.perf_counter()
                if sleep_for > 0:
                    time.sleep(sleep_for)
                else:
                    next_t = time.perf_counter()

    threads = [threading.Thread(target=sub_loop, args=(c,), daemon=True) for c in subs]
    threads += [
        threading.Thread(target=pub_loop, args=(i, c), daemon=True) for i, c in enumerate(pubs)
    ]
    for t in threads:
        t.start()

    time.sleep(0.3)
    t0 = time.perf_counter()
    go.set()
    time.sleep(args.duration)
    stop.set()
    elapsed = time.perf_counter() - t0
    time.sleep(1.0)

    for c in pubs + subs:
        try:
            c.close()
        except Exception:
            pass

    with counters.lock:
        pub = counters.published
        recv = counters.received
        perr = counters.publish_errors
        lats = list(counters.latencies_ms)

    print("-" * 64)
    print(f"压测窗口       : {elapsed:.2f}s")
    print(f"已发布         : {pub}  (理论 {expected}, 完成率 {100*pub/max(expected,1):.1f}%)")
    print(f"已接收(合计)   : {recv}  (~{recv/max(args.subscribers,1):.0f}/sub)")
    print(f"发布错误       : {perr}")
    print(f"吞吐(发布)     : {pub / max(elapsed, 0.001):.1f} msg/s")
    if lats:
        print(
            f"发送耗时 ms    : avg={statistics.mean(lats):.3f}  "
            f"p50={pct(lats,50):.3f}  p95={pct(lats,95):.3f}  p99={pct(lats,99):.3f}"
        )

    if token:
        try:
            time.sleep(2)
            after = admin_get("/api/monitor/system", token)
            stats = admin_get("/api/monitor/stats", token)
            print("-" * 64)
            print(
                f"[after]  CPU {after.get('cpu_usage', 0):.1f}%  "
                f"MEM {after.get('memory_usage', 0):.1f}%  "
                f"RSS={after.get('process_memory_bytes', 0)/1024/1024:.1f}MB"
            )
            print(
                f"[monitor] connections={stats.get('total_connections')}  "
                f"msgs_last_min={stats.get('messages_last_minute')}"
            )
        except Exception as e:
            print(f"[warn] {e}")

    ok = perr == 0 and pub >= expected * 0.85
    print("=" * 64)
    print("结果:", "PASS" if ok else "CHECK")
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
