#!/usr/bin/env python3
"""启动多个 MQTT 测试客户端：3 台 GPS + 1 个平台订阅端 + 1 个本地测试端。"""

from __future__ import annotations

import json
import random
import sys
import threading
import time

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
STOP = threading.Event()
CONNECT_TIMEOUT = 30.0


def connect(client_id: str, username: str, password: str) -> TcpTransport:
    last_err: Exception | None = None
    for attempt in range(1, 4):
        try:
            conn = TcpTransport(HOST, PORT)
            conn.sock.settimeout(CONNECT_TIMEOUT)
            conn.send_mqtt(mqtt_connect_v4_auth(client_id, username, password))
            ptype, body = conn.recv_mqtt(timeout=CONNECT_TIMEOUT)
            if not connack_ok(ptype, body, "v4"):
                conn.close()
                raise RuntimeError(f"CONNACK 失败 type={ptype} body={body.hex()}")
            return conn
        except Exception as e:
            last_err = e
            print(f"[..] {client_id} 第 {attempt} 次连接失败: {e}", flush=True)
            time.sleep(1.5 * attempt)
    raise RuntimeError(f"{client_id} 连接失败: {last_err}")


def drain(conn: TcpTransport, timeout: float = 0.2) -> list[tuple[str, str]]:
    out: list[tuple[str, str]] = []
    deadline = time.time() + timeout
    while time.time() < deadline and not STOP.is_set():
        remain = max(0.05, deadline - time.time())
        try:
            ptype, body = conn.recv_mqtt(timeout=remain)
        except Exception:
            break
        if ptype == 3:
            topic, msg = parse_publish_payload(body, "v4")
            out.append((topic, msg))
    return out


def run_platform() -> None:
    name = "platform"
    try:
        conn = connect("platform-hub", name, "pass123")
        for i, topic in enumerate(["fleet/+/gps", "fleet/+/status"], start=1):
            conn.send_mqtt(mqtt_subscribe_v4(i, topic, 0))
            conn.recv_mqtt(timeout=CONNECT_TIMEOUT)
        print(f"[OK] {name} 已连接并订阅 fleet/+/gps, fleet/+/status", flush=True)
        tick = 0
        while not STOP.is_set():
            msgs = drain(conn, 1.0)
            for topic, msg in msgs:
                print(f"[platform] ← {topic}: {msg[:120]}", flush=True)
            tick += 1
            if tick % 15 == 0:
                target = random.choice(["gps001", "gps002", "gps003"])
                payload = json.dumps({"cmd": "locate", "ts": int(time.time())})
                conn.send_mqtt(mqtt_publish_v4(f"fleet/{target}/cmd", payload))
                print(f"[platform] → fleet/{target}/cmd: {payload}", flush=True)
            if tick % 20 == 0:
                conn.send_mqtt(mqtt_pingreq())
                try:
                    conn.recv_mqtt(timeout=2)
                except Exception:
                    pass
    except Exception as e:
        print(f"[ERR] platform: {e}", flush=True)


def run_gps(device_id: str, lat0: float, lon0: float) -> None:
    name = device_id
    try:
        conn = connect(device_id, name, "pass123")
        conn.send_mqtt(mqtt_subscribe_v4(1, f"fleet/{device_id}/cmd", 0))
        conn.recv_mqtt(timeout=CONNECT_TIMEOUT)
        print(f"[OK] {name} 已连接，上报 GPS + 订阅 cmd", flush=True)
        n = 0
        while not STOP.is_set():
            for topic, msg in drain(conn, 0.3):
                print(f"[{name}] ← {topic}: {msg}", flush=True)
            n += 1
            lat = lat0 + random.uniform(-0.01, 0.01)
            lon = lon0 + random.uniform(-0.01, 0.01)
            gps = json.dumps(
                {
                    "device": device_id,
                    "lat": round(lat, 6),
                    "lon": round(lon, 6),
                    "speed": round(random.uniform(0, 80), 1),
                    "ts": int(time.time()),
                }
            )
            conn.send_mqtt(mqtt_publish_v4(f"fleet/{device_id}/gps", gps))
            if n % 3 == 0:
                status = json.dumps(
                    {
                        "device": device_id,
                        "battery": random.randint(20, 100),
                        "signal": random.randint(1, 5),
                        "ts": int(time.time()),
                    }
                )
                conn.send_mqtt(mqtt_publish_v4(f"fleet/{device_id}/status", status))
            if n % 10 == 0:
                conn.send_mqtt(mqtt_pingreq())
                try:
                    conn.recv_mqtt(timeout=1)
                except Exception:
                    pass
            time.sleep(2.0)
    except Exception as e:
        print(f"[ERR] {name}: {e}", flush=True)


def run_testuser() -> None:
    name = "testuser"
    try:
        conn = connect("testuser-local", name, "testpass")
        conn.send_mqtt(mqtt_subscribe_v4(1, "test/local/#", 0))
        conn.recv_mqtt(timeout=CONNECT_TIMEOUT)
        print(f"[OK] {name} 已连接并订阅 test/local/#", flush=True)
        n = 0
        while not STOP.is_set():
            for topic, msg in drain(conn, 0.5):
                print(f"[testuser] ← {topic}: {msg}", flush=True)
            n += 1
            payload = f"ping-{n}-{int(time.time())}"
            conn.send_mqtt(mqtt_publish_v4("test/local/ping", payload))
            time.sleep(3.0)
    except Exception as e:
        print(f"[ERR] {name}: {e}", flush=True)


def main() -> int:
    duration = int(sys.argv[1]) if len(sys.argv) > 1 else 120
    print(f"启动演示客户端，持续 {duration}s → {HOST}:{PORT}", flush=True)

    # 串行拉起，避免 bcrypt 认证互相抢占导致超时
    starters = [
        ("platform", run_platform),
        ("gps001", lambda: run_gps("gps001", 31.2304, 121.4737)),
        ("gps002", lambda: run_gps("gps002", 39.9042, 116.4074)),
        ("gps003", lambda: run_gps("gps003", 23.1291, 113.2644)),
        ("testuser", run_testuser),
    ]
    threads: list[threading.Thread] = []
    for name, target in starters:
        t = threading.Thread(target=target, name=name, daemon=True)
        t.start()
        threads.append(t)
        time.sleep(3.0)

    try:
        time.sleep(duration)
    except KeyboardInterrupt:
        pass
    STOP.set()
    print("演示结束，客户端退出。", flush=True)
    time.sleep(1)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
