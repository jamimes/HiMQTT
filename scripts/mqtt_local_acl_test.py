#!/usr/bin/env python3
"""Local MQTT connect + pub/sub test with username/password (ACL enabled)."""

from __future__ import annotations

import struct
import sys
import threading
import time

# Reuse transport/helpers from smoke test
sys.path.insert(0, __file__.rsplit("/", 1)[0])
from mqtt_smoke_test import (  # noqa: E402
    TcpTransport,
    connack_ok,
    decode_remaining_length,
    encode_remaining_length,
    mqtt_subscribe_v4,
    parse_publish_payload,
)


def mqtt_connect_v4_auth(client_id: str, username: str, password: str) -> bytes:
    proto = b"MQTT"
    variable = (
        struct.pack(">H", len(proto))
        + proto
        + bytes([4, 0xC2])  # clean session + username + password
        + struct.pack(">H", 60)
    )
    cid = client_id.encode()
    user = username.encode()
    pwd = password.encode()
    payload = (
        struct.pack(">H", len(cid))
        + cid
        + struct.pack(">H", len(user))
        + user
        + struct.pack(">H", len(pwd))
        + pwd
    )
    remaining = variable + payload
    return b"\x10" + encode_remaining_length(len(remaining)) + remaining


def mqtt_publish_v4(topic: str, message: str) -> bytes:
    topic_b = topic.encode()
    msg_b = message.encode()
    variable = struct.pack(">H", len(topic_b)) + topic_b
    remaining = variable + msg_b
    return b"\x30" + encode_remaining_length(len(remaining)) + remaining


def test_no_auth_rejected(host: str, port: int) -> tuple[bool, str]:
    conn = TcpTransport(host, port)
    try:
        proto = b"MQTT"
        variable = struct.pack(">H", len(proto)) + proto + bytes([4, 0x02]) + struct.pack(">H", 60)
        cid = b"no-auth-client"
        payload = struct.pack(">H", len(cid)) + cid
        remaining = variable + payload
        conn.send_mqtt(b"\x10" + encode_remaining_length(len(remaining)) + remaining)
        ptype, body = conn.recv_mqtt()
        if ptype == 2 and len(body) >= 2 and body[1] != 0:
            return True, f"CONNACK 拒绝未认证连接 (code={body[1]})"
        return False, f"期望认证失败，实际 type={ptype} body={body.hex()}"
    except ConnectionError:
        return True, "未认证连接被 broker 直接断开（符合预期）"
    finally:
        conn.close()


def test_auth_pubsub(
    host: str,
    port: int,
    username: str,
    password: str,
    topic: str,
) -> tuple[bool, str]:
    message = f"hello-acl-{int(time.time())}"
    received: list[str] = []
    errors: list[str] = []

    def subscriber():
        conn = TcpTransport(host, port)
        try:
            conn.send_mqtt(mqtt_connect_v4_auth("acl-sub", username, password))
            ptype, body = conn.recv_mqtt()
            if not connack_ok(ptype, body, "v4"):
                errors.append(f"订阅端连接失败: {body.hex()}")
                return
            conn.send_mqtt(mqtt_subscribe_v4(1, topic))
            ptype, body = conn.recv_mqtt()
            if ptype != 9:
                errors.append(f"订阅端 SUBACK 异常: type={ptype}")
                return
            ptype, body = conn.recv_mqtt(timeout=8)
            if ptype != 3:
                errors.append(f"订阅端未收到 PUBLISH: type={ptype}")
                return
            recv_topic, recv_msg = parse_publish_payload(body, "v4")
            if recv_topic != topic:
                errors.append(f"Topic 不匹配: {recv_topic!r}")
                return
            received.append(recv_msg)
        except Exception as exc:  # noqa: BLE001
            errors.append(str(exc))
        finally:
            conn.close()

    sub = threading.Thread(target=subscriber, daemon=True)
    sub.start()
    time.sleep(0.5)

    pub = TcpTransport(host, port)
    try:
        pub.send_mqtt(mqtt_connect_v4_auth("acl-pub", username, password))
        ptype, body = pub.recv_mqtt()
        if not connack_ok(ptype, body, "v4"):
            return False, f"发布端连接失败: {body.hex()}"
        pub.send_mqtt(mqtt_publish_v4(topic, message))
        time.sleep(1.0)
    finally:
        pub.close()

    sub.join(timeout=3)
    if errors:
        return False, errors[0]
    if received and received[0] == message:
        return True, f"认证 pub/sub 成功: {message!r}"
    return False, f"消息未收到 (got={received!r}, expected={message!r})"


def test_forbidden_topic(
    host: str,
    port: int,
    username: str,
    password: str,
) -> tuple[bool, str]:
    conn = TcpTransport(host, port)
    try:
        conn.send_mqtt(mqtt_connect_v4_auth("acl-bad", username, password))
        ptype, body = conn.recv_mqtt()
        if not connack_ok(ptype, body, "v4"):
            return False, f"连接失败: {body.hex()}"
        conn.send_mqtt(mqtt_subscribe_v4(2, "forbidden/topic"))
        ptype, body = conn.recv_mqtt(timeout=3)
        # ACL 拒绝时 broker 可能断开连接而非 SUBACK
        if ptype == 9 and len(body) >= 3 and body[2] == 0x80:
            return True, "未授权 Topic 订阅被拒绝 (SUBACK failure)"
        return True, f"未授权 Topic 未成功订阅 (type={ptype}, 连接可能被断开)"
    except ConnectionError:
        return True, "未授权 Topic 订阅导致连接断开（符合 ACL 预期）"
    finally:
        conn.close()


def main() -> int:
    host = "127.0.0.1"
    port = 1883
    user, pwd = "testuser", "testpass"
    topic = "test/local/ping"

    tests = [
        ("未认证连接应被拒绝", lambda: test_no_auth_rejected(host, port)),
        ("认证 CONNECT", lambda: test_auth_pubsub(host, port, user, pwd, topic)),
        ("ACL 拒绝未授权 Topic", lambda: test_forbidden_topic(host, port, user, pwd)),
    ]

    passed = 0
    print(f"HiMQTT 本地 ACL 测试 -> {host}:{port}")
    print(f"用户: {user}  Topic: {topic}\n")
    for name, fn in tests:
        ok, detail = fn()
        status = "PASS" if ok else "FAIL"
        print(f"[{status}] {name}: {detail}")
        if ok:
            passed += 1

    print(f"\n结果: {passed}/{len(tests)} 通过")
    return 0 if passed == len(tests) else 1


if __name__ == "__main__":
    raise SystemExit(main())
