#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$ROOT/target/debug/himqtt"
CFG="$ROOT/config/himqtt.toml"
LOG="$ROOT/target/himqtt-dev.log"
PIDFILE="$ROOT/target/himqtt-dev.pid"

cd "$ROOT"

echo "==> 编译 debug 版本"
cargo build -q

echo "==> 启动 HiMQTT (debug)"
if [[ -f "$PIDFILE" ]] && kill -0 "$(cat "$PIDFILE")" 2>/dev/null; then
  kill "$(cat "$PIDFILE")" 2>/dev/null || true
  sleep 1
fi

"$BIN" -q -c "$CFG" -vv >"$LOG" 2>&1 &
echo $! >"$PIDFILE"
SERVER_PID="$(cat "$PIDFILE")"

cleanup() {
  if kill -0 "$SERVER_PID" 2>/dev/null; then
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
  rm -f "$PIDFILE"
}
trap cleanup EXIT

echo "    PID=$SERVER_PID, 日志=$LOG"
sleep 2

echo
echo "==> 端口监听检查"
PORT_FAIL=0
for spec in "1883:MQTT v4" "1884:MQTT v5" "8083:WebSocket" "3030:Console" "9042:Prometheus"; do
  port="${spec%%:*}"
  name="${spec#*:}"
  if ss -tln | awk '{print $4}' | grep -q ":${port}$"; then
    echo "[PASS] ${name} 监听 0.0.0.0:${port} 或 127.0.0.1:${port}"
  else
    echo "[FAIL] ${name} 未监听端口 ${port}"
    PORT_FAIL=1
  fi
done

echo
echo "==> HTTP 端点探测"
if curl -sf --max-time 3 "http://127.0.0.1:3030/" >/dev/null 2>&1; then
  echo "[PASS] Console HTTP 127.0.0.1:3030 可访问"
else
  code="$(curl -s -o /dev/null -w '%{http_code}' --max-time 3 "http://127.0.0.1:3030/" 2>/dev/null || echo "000")"
  echo "[INFO] Console HTTP 返回 ${code}"
fi

if curl -sf --max-time 3 "http://127.0.0.1:9042/metrics" | head -1 | grep -q .; then
  echo "[PASS] Prometheus /metrics 有响应"
else
  echo "[INFO] Prometheus /metrics 暂无响应或路径不同"
fi

echo
echo "==> MQTT 全量协议测试 (v4 / v5 / WebSocket / 跨协议路由)"
python3 "$ROOT/scripts/mqtt_smoke_test.py"
MQTT_RC=$?

echo
echo "==> 最近服务器日志 (末尾 25 行)"
tail -25 "$LOG" || true

if [[ "$PORT_FAIL" -ne 0 ]]; then
  exit 1
fi
exit "$MQTT_RC"
