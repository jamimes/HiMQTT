#!/usr/bin/env python3
"""HiMQTT reliability tests: repeated connect, pub/sub, ping, concurrency."""

from __future__ import annotations

import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import mqtt_smoke_test as mqtt  # noqa: E402

HOST = "127.0.0.1"
ROUNDS = {
    "connect_v4": 50,
    "pubsub_v4": 30,
    "pubsub_v5": 20,
    "pubsub_ws": 20,
    "cross_transport": 10,
    "ping_v4": 30,
    "concurrent_burst": 20,
}


def run_rounds(name: str, count: int, fn) -> tuple[int, int, float]:
    ok = 0
    start = time.perf_counter()
    failures: list[str] = []
    for i in range(count):
        passed, detail = fn(i)
        if passed:
            ok += 1
        elif len(failures) < 3:
            failures.append(f"#{i + 1}: {detail}")
    elapsed = time.perf_counter() - start
    status = "PASS" if ok == count else "FAIL"
    print(f"[{status}] {name}: {ok}/{count} ok ({elapsed:.2f}s)")
    for msg in failures:
        print(f"         {msg}")
    return ok, count, elapsed


def test_connect_v4(_: int):
    return mqtt.test_connect(HOST, 1883, "v4", "tcp")


def test_pubsub_v4(i: int):
    topic = f"himqtt/reliability/v4/{i % 5}"
    return mqtt.test_pubsub(HOST, 1883, "v4", "tcp", topic=topic)


def test_pubsub_v5(i: int):
    topic = f"himqtt/reliability/v5/{i % 5}"
    return mqtt.test_pubsub(HOST, 1884, "v5", "tcp", topic=topic)


def test_pubsub_ws(i: int):
    topic = f"himqtt/reliability/ws/{i % 5}"
    return mqtt.test_pubsub(HOST, 8083, "v4", "ws", topic=topic)


def test_cross(_: int):
    return mqtt.test_cross_transport(HOST)


def test_ping_v4(_: int):
    return mqtt.test_ping(HOST, 1883, "v4", "tcp")


def test_concurrent_burst(_: int):
    topic = f"himqtt/reliability/burst/{int(time.time() * 1000) % 100000}"
    expected: list[str] = []
    received: list[str] = []
    errors: list[str] = []
    lock = threading.Lock()
    message_count = 5

    def subscriber(sub_id: int):
        conn = mqtt.open_transport(HOST, 1883, "tcp")
        try:
            conn.send_mqtt(mqtt.mqtt_connect_v4(f"burst-sub-{sub_id}"))
            ptype, body = conn.recv_mqtt()
            if not mqtt.connack_ok(ptype, body, "v4"):
                errors.append(f"sub{sub_id} connect failed")
                return
            conn.send_mqtt(mqtt.mqtt_subscribe_v4(sub_id + 1, topic))
            ptype, body = conn.recv_mqtt()
            if ptype != 9:
                errors.append(f"sub{sub_id} suback failed")
                return
            for _ in range(message_count):
                ptype, body = conn.recv_mqtt(timeout=10)
                if ptype != 3:
                    errors.append(f"sub{sub_id} publish missing")
                    return
                _, msg = mqtt.parse_publish_payload(body, "v4")
                with lock:
                    received.append(msg)
        except Exception as exc:  # noqa: BLE001
            errors.append(f"sub{sub_id}: {exc}")
        finally:
            conn.close()

    threads = [threading.Thread(target=subscriber, args=(i,), daemon=True) for i in range(3)]
    for t in threads:
        t.start()
    time.sleep(0.8)

    for n in range(message_count):
        msg = f"burst-{n}-{time.time_ns()}"
        expected.append(msg)
        pub = mqtt.open_transport(HOST, 1883, "tcp")
        try:
            pub.send_mqtt(mqtt.mqtt_connect_v4(f"burst-pub-{n}"))
            ptype, body = pub.recv_mqtt()
            if not mqtt.connack_ok(ptype, body, "v4"):
                return False, f"publisher connect failed at {n}"
            pub.send_mqtt(mqtt.mqtt_publish_v4(topic, msg))
        finally:
            pub.close()
        time.sleep(0.05)

    for t in threads:
        t.join(timeout=12)

    if errors:
        return False, errors[0]
    # 3 subscribers x 5 messages = 15 deliveries
    if len(received) != message_count * 3:
        return False, f"expected {message_count * 3} deliveries, got {len(received)}"
    for msg in expected:
        if received.count(msg) != 3:
            return False, f"message {msg!r} delivered {received.count(msg)}/3 times"
    return True, f"{len(received)} fan-out deliveries ok"


def main() -> int:
    print("HiMQTT reliability test")
    print(f"Target: {HOST}\n")

    total_ok = 0
    total_count = 0
    total_time = 0.0

    suites = [
        ("Repeated CONNECT (v4)", ROUNDS["connect_v4"], test_connect_v4),
        ("Repeated PUB/SUB (v4)", ROUNDS["pubsub_v4"], test_pubsub_v4),
        ("Repeated PUB/SUB (v5)", ROUNDS["pubsub_v5"], test_pubsub_v5),
        ("Repeated PUB/SUB (WebSocket)", ROUNDS["pubsub_ws"], test_pubsub_ws),
        ("Cross transport (TCP->WS)", ROUNDS["cross_transport"], test_cross),
        ("Repeated PING (v4)", ROUNDS["ping_v4"], test_ping_v4),
        ("Concurrent fan-out burst", ROUNDS["concurrent_burst"], test_concurrent_burst),
    ]

    for name, count, fn in suites:
        ok, n, elapsed = run_rounds(name, count, fn)
        total_ok += ok
        total_count += n
        total_time += elapsed

    rate = (total_ok / total_count * 100) if total_count else 0
    print(f"\nSummary: {total_ok}/{total_count} checks passed ({rate:.1f}%), total {total_time:.1f}s")
    if total_ok == total_count:
        print("Result: RELIABLE")
        return 0
    print("Result: UNRELIABLE")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
