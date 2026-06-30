#!/usr/bin/env python3
"""Minimal MQTT smoke tests without external dependencies."""

from __future__ import annotations

import base64
import os
import secrets
import socket
import struct
import sys
import threading
import time
from abc import ABC, abstractmethod


def encode_remaining_length(length: int) -> bytes:
    out = bytearray()
    while True:
        digit = length % 128
        length //= 128
        if length > 0:
            digit |= 0x80
        out.append(digit)
        if length == 0:
            break
    return bytes(out)


def decode_remaining_length(data: bytes, start: int = 0) -> tuple[int, int]:
    multiplier = 1
    value = 0
    pos = start
    while True:
        digit = data[pos]
        pos += 1
        value += (digit & 0x7F) * multiplier
        multiplier *= 128
        if (digit & 0x80) == 0:
            break
    return value, pos


class Transport(ABC):
    @abstractmethod
    def send_mqtt(self, packet: bytes) -> None: ...

    @abstractmethod
    def recv_mqtt(self, timeout: float = 5.0) -> tuple[int, bytes]: ...

    @abstractmethod
    def close(self) -> None: ...


class TcpTransport(Transport):
    def __init__(self, host: str, port: int):
        self.sock = socket.create_connection((host, port), timeout=5)

    def send_mqtt(self, packet: bytes) -> None:
        self.sock.sendall(packet)

    def recv_mqtt(self, timeout: float = 5.0) -> tuple[int, bytes]:
        self.sock.settimeout(timeout)
        header = self.sock.recv(1)
        if not header:
            raise ConnectionError("connection closed")
        packet_type = header[0] >> 4
        raw = bytearray(header)
        while True:
            b = self.sock.recv(1)
            raw.extend(b)
            if not (b[0] & 0x80):
                break
        remaining, _ = decode_remaining_length(raw, 1)
        body = bytearray()
        while len(body) < remaining:
            chunk = self.sock.recv(remaining - len(body))
            if not chunk:
                raise ConnectionError("connection closed while reading body")
            body.extend(chunk)
        return packet_type, bytes(body)

    def close(self) -> None:
        self.sock.close()


class WebSocketTransport(Transport):
    def __init__(self, host: str, port: int, path: str = "/"):
        self.sock = socket.create_connection((host, port), timeout=5)
        key = base64.b64encode(secrets.token_bytes(16)).decode()
        request = (
            f"GET {path} HTTP/1.1\r\n"
            f"Host: {host}:{port}\r\n"
            "Upgrade: websocket\r\n"
            "Connection: Upgrade\r\n"
            f"Sec-WebSocket-Key: {key}\r\n"
            "Sec-WebSocket-Version: 13\r\n"
            "Sec-WebSocket-Protocol: mqtt\r\n"
            "\r\n"
        )
        self.sock.sendall(request.encode())
        response = b""
        while b"\r\n\r\n" not in response:
            chunk = self.sock.recv(4096)
            if not chunk:
                raise ConnectionError("websocket handshake closed")
            response += chunk
        status_line = response.split(b"\r\n", 1)[0].decode()
        if "101" not in status_line:
            raise ConnectionError(f"websocket handshake failed: {status_line}")
        if b"sec-websocket-protocol: mqtt" not in response.lower():
            raise ConnectionError("websocket subprotocol mqtt not accepted")

    def _send_frame(self, payload: bytes) -> None:
        mask = os.urandom(4)
        masked = bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
        length = len(payload)
        if length < 126:
            header = bytes([0x82, 0x80 | length]) + mask
        elif length < 65536:
            header = bytes([0x82, 0xFE]) + struct.pack(">H", length) + mask
        else:
            header = bytes([0x82, 0xFF]) + struct.pack(">Q", length) + mask
        self.sock.sendall(header + masked)

    def _recv_frame(self, timeout: float) -> bytes:
        self.sock.settimeout(timeout)
        hdr = self.sock.recv(2)
        if len(hdr) < 2:
            raise ConnectionError("websocket closed")
        b1, b2 = hdr
        opcode = b1 & 0x0F
        if opcode == 0x8:
            raise ConnectionError("websocket closed by server")
        masked = bool(b2 & 0x80)
        length = b2 & 0x7F
        if length == 126:
            length = struct.unpack(">H", self.sock.recv(2))[0]
        elif length == 127:
            length = struct.unpack(">Q", self.sock.recv(8))[0]
        mask = self.sock.recv(4) if masked else b""
        payload = bytearray()
        while len(payload) < length:
            payload.extend(self.sock.recv(length - len(payload)))
        if masked:
            payload = bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
        return bytes(payload)

    def send_mqtt(self, packet: bytes) -> None:
        self._send_frame(packet)

    def recv_mqtt(self, timeout: float = 5.0) -> tuple[int, bytes]:
        payload = self._recv_frame(timeout)
        if not payload:
            raise ConnectionError("empty websocket frame")
        packet_type = payload[0] >> 4
        remaining, pos = decode_remaining_length(payload, 1)
        body = payload[pos : pos + remaining]
        if len(body) != remaining:
            raise ConnectionError("truncated mqtt packet in websocket frame")
        return packet_type, body

    def close(self) -> None:
        self.sock.close()


def mqtt_connect_v4(client_id: str) -> bytes:
    proto = b"MQTT"
    variable = (
        struct.pack(">H", len(proto))
        + proto
        + bytes([4, 0x02])
        + struct.pack(">H", 60)
    )
    payload = struct.pack(">H", len(client_id)) + client_id.encode()
    remaining = variable + payload
    return b"\x10" + encode_remaining_length(len(remaining)) + remaining


def mqtt_connect_v5(client_id: str) -> bytes:
    proto = b"MQTT"
    variable = (
        struct.pack(">H", len(proto))
        + proto
        + bytes([5, 0x02])
        + struct.pack(">H", 60)
        + b"\x00"
    )
    payload = struct.pack(">H", len(client_id)) + client_id.encode()
    remaining = variable + payload
    return b"\x10" + encode_remaining_length(len(remaining)) + remaining


def mqtt_subscribe_v4(packet_id: int, topic: str, qos: int = 0) -> bytes:
    topic_b = topic.encode()
    payload = (
        struct.pack(">H", packet_id)
        + struct.pack(">H", len(topic_b))
        + topic_b
        + bytes([qos])
    )
    return b"\x82" + encode_remaining_length(len(payload)) + payload


def mqtt_subscribe_v5(packet_id: int, topic: str) -> bytes:
    topic_b = topic.encode()
    payload = struct.pack(">H", packet_id) + b"\x00" + struct.pack(">H", len(topic_b)) + topic_b + b"\x00"
    return b"\x82" + encode_remaining_length(len(payload)) + payload


def mqtt_publish_v4(topic: str, message: str) -> bytes:
    topic_b = topic.encode()
    msg_b = message.encode()
    variable = struct.pack(">H", len(topic_b)) + topic_b
    remaining = variable + msg_b
    return b"\x30" + encode_remaining_length(len(remaining)) + remaining


def mqtt_publish_v5(topic: str, message: str) -> bytes:
    topic_b = topic.encode()
    msg_b = message.encode()
    variable = struct.pack(">H", len(topic_b)) + topic_b + b"\x00"
    remaining = variable + msg_b
    return b"\x30" + encode_remaining_length(len(remaining)) + remaining


def mqtt_pingreq() -> bytes:
    return b"\xC0\x00"


def parse_publish_payload(body: bytes, protocol: str) -> tuple[str, str]:
    tlen = struct.unpack(">H", body[:2])[0]
    pos = 2 + tlen
    if protocol == "v5":
        if body[pos] == 0:
            pos += 1
        else:
            prop_len, pos = decode_remaining_length(body, pos)
            pos += prop_len
    topic = body[2 : 2 + tlen].decode()
    message = body[pos:].decode()
    return topic, message


def connack_ok(ptype: int, body: bytes, protocol: str) -> bool:
    if ptype != 2:
        return False
    if protocol == "v5":
        return len(body) >= 2 and body[1] == 0
    return len(body) >= 2 and body[1] == 0


def open_transport(host: str, port: int, transport: str) -> Transport:
    if transport == "ws":
        return WebSocketTransport(host, port)
    return TcpTransport(host, port)


def test_connect(host: str, port: int, protocol: str, transport: str) -> tuple[bool, str]:
    conn = open_transport(host, port, transport)
    try:
        connect = mqtt_connect_v5 if protocol == "v5" else mqtt_connect_v4
        conn.send_mqtt(connect("smoke-test-client"))
        ptype, body = conn.recv_mqtt()
        if not connack_ok(ptype, body, protocol):
            return False, f"CONNACK rejected: type={ptype} body={body.hex()}"
        return True, "CONNACK ok (session accepted)"
    finally:
        conn.close()


def test_pubsub(
    host: str,
    port: int,
    protocol: str,
    transport: str,
    topic: str | None = None,
) -> tuple[bool, str]:
    topic = topic or f"himqtt/smoke/{protocol}-{transport}"
    message = f"hello-{protocol}-{transport}-{int(time.time())}"
    received: list[str] = []
    error: list[str] = []

    subscribe = mqtt_subscribe_v5 if protocol == "v5" else mqtt_subscribe_v4
    publish = mqtt_publish_v5 if protocol == "v5" else mqtt_publish_v4
    connect = mqtt_connect_v5 if protocol == "v5" else mqtt_connect_v4

    def subscriber():
        conn = open_transport(host, port, transport)
        try:
            conn.send_mqtt(connect("smoke-sub"))
            ptype, body = conn.recv_mqtt()
            if not connack_ok(ptype, body, protocol):
                error.append(f"sub connect failed: type={ptype} body={body.hex()}")
                return
            conn.send_mqtt(subscribe(1, topic))
            ptype, body = conn.recv_mqtt()
            if ptype != 9:
                error.append(f"expected SUBACK(9), got {ptype}")
                return
            ptype, body = conn.recv_mqtt(timeout=8)
            if ptype != 3:
                error.append(f"expected PUBLISH(3), got {ptype}")
                return
            recv_topic, recv_msg = parse_publish_payload(body, protocol)
            if recv_topic != topic:
                error.append(f"topic mismatch: {recv_topic!r}")
                return
            received.append(recv_msg)
        except Exception as exc:  # noqa: BLE001
            error.append(str(exc))
        finally:
            conn.close()

    sub = threading.Thread(target=subscriber, daemon=True)
    sub.start()
    time.sleep(0.5)

    pub = open_transport(host, port, transport)
    try:
        pub.send_mqtt(connect("smoke-pub"))
        ptype, body = pub.recv_mqtt()
        if not connack_ok(ptype, body, protocol):
            return False, f"publisher connect failed: {body.hex()}"
        pub.send_mqtt(publish(topic, message))
        time.sleep(1.0)
    finally:
        pub.close()

    sub.join(timeout=3)
    if error:
        return False, error[0]
    if received and received[0] == message:
        return True, f"pub/sub ok: {message!r}"
    return False, f"no matching message received (got {received!r}, expected {message!r})"


def test_ping(host: str, port: int, protocol: str, transport: str) -> tuple[bool, str]:
    conn = open_transport(host, port, transport)
    connect = mqtt_connect_v5 if protocol == "v5" else mqtt_connect_v4
    try:
        conn.send_mqtt(connect("smoke-ping"))
        ptype, body = conn.recv_mqtt()
        if not connack_ok(ptype, body, protocol):
            return False, "connect before ping failed"
        conn.send_mqtt(mqtt_pingreq())
        ptype, body = conn.recv_mqtt()
        if ptype != 13:
            return False, f"expected PINGRESP(13), got {ptype}"
        return True, "PINGREQ/PINGRESP ok"
    finally:
        conn.close()


def test_cross_transport(host: str) -> tuple[bool, str]:
    topic = f"himqtt/smoke/cross-{int(time.time())}"
    message = f"cross-{int(time.time())}"
    received: list[str] = []
    error: list[str] = []

    def ws_subscriber():
        conn = WebSocketTransport(host, 8083)
        try:
            conn.send_mqtt(mqtt_connect_v4("cross-sub-ws"))
            ptype, body = conn.recv_mqtt()
            if not connack_ok(ptype, body, "v4"):
                error.append(f"ws sub connect failed: {body.hex()}")
                return
            conn.send_mqtt(mqtt_subscribe_v4(1, topic))
            ptype, body = conn.recv_mqtt()
            if ptype != 9:
                error.append(f"ws suback expected 9, got {ptype}")
                return
            ptype, body = conn.recv_mqtt(timeout=8)
            if ptype != 3:
                error.append(f"ws publish expected 3, got {ptype}")
                return
            recv_topic, recv_msg = parse_publish_payload(body, "v4")
            if recv_topic != topic:
                error.append(f"topic mismatch: {recv_topic!r}")
                return
            received.append(recv_msg)
        except Exception as exc:  # noqa: BLE001
            error.append(str(exc))
        finally:
            conn.close()

    sub = threading.Thread(target=ws_subscriber, daemon=True)
    sub.start()
    time.sleep(0.5)

    pub = TcpTransport(host, 1883)
    try:
        pub.send_mqtt(mqtt_connect_v4("cross-pub-tcp"))
        ptype, body = pub.recv_mqtt()
        if not connack_ok(ptype, body, "v4"):
            return False, f"tcp publisher connect failed: {body.hex()}"
        pub.send_mqtt(mqtt_publish_v4(topic, message))
        time.sleep(1.0)
    finally:
        pub.close()

    sub.join(timeout=3)
    if error:
        return False, error[0]
    if received and received[0] == message:
        return True, f"TCP(1883) publish -> WS(8083) subscribe ok: {message!r}"
    return False, f"cross transport failed (got {received!r}, expected {message!r})"


def run_suite(label: str, host: str, port: int, protocol: str, transport: str) -> int:
    print(f"\n--- {label} ({host}:{port}, protocol={protocol}, transport={transport}) ---")
    tests = [
        ("CONNECT + CONNACK", lambda: test_connect(host, port, protocol, transport)),
        ("PUBLISH / SUBSCRIBE", lambda: test_pubsub(host, port, protocol, transport)),
        ("PINGREQ / PINGRESP", lambda: test_ping(host, port, protocol, transport)),
    ]
    passed = 0
    for name, fn in tests:
        ok, detail = fn()
        status = "PASS" if ok else "FAIL"
        print(f"[{status}] {name}: {detail}")
        if ok:
            passed += 1
    print(f"Suite result: {passed}/{len(tests)} passed")
    return 0 if passed == len(tests) else 1


def main() -> int:
    host = "127.0.0.1"
    rc = 0
    rc |= run_suite("MQTT v4 / TCP", host, 1883, "v4", "tcp")
    rc |= run_suite("MQTT v5 / TCP", host, 1884, "v5", "tcp")
    rc |= run_suite("MQTT v4 / WebSocket", host, 8083, "v4", "ws")

    print("\n--- Cross transport: TCP publish -> WebSocket subscribe ---")
    ok, detail = test_cross_transport(host)
    status = "PASS" if ok else "FAIL"
    print(f"[{status}] Cross transport: {detail}")
    if not ok:
        rc |= 1

    total_suites = 4
    print(f"\nOverall: {'ALL PASSED' if rc == 0 else 'SOME TESTS FAILED'} ({total_suites} suites)")
    return rc


if __name__ == "__main__":
    raise SystemExit(main())
