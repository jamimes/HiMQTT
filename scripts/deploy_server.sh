#!/usr/bin/env bash
set -euo pipefail

DEPLOY_DIR="${1:-/tmp/himqtt-deploy}"
SERVICE_NAME="himqtt"
BIN_PATH="/usr/local/bin/himqtt"
CONFIG_DIR="/etc/himqtt"
CONFIG_PATH="${CONFIG_DIR}/himqtt.toml"
SERVICE_USER="himqtt"
SERVICE_GROUP="himqtt"

# 对外服务端口（需在防火墙放行）
PUBLIC_PORTS=(1883 1884 8083)
# 本机监听端口（仅检测是否在听）
LOCAL_PORTS=(8090 3030 9042)

log() { echo "[deploy] $*"; }

require_root() {
  if [[ "${EUID}" -ne 0 ]]; then
    log "请使用 root 或通过 sudo 运行"
    exit 1
  fi
}

install_files() {
  log "安装二进制与配置"
  install -d "${CONFIG_DIR}"
  install -m 755 "${DEPLOY_DIR}/himqtt" "${BIN_PATH}"
  install -m 644 "${DEPLOY_DIR}/himqtt.toml" "${CONFIG_PATH}"
  install -m 644 "${DEPLOY_DIR}/himqtt.service" "/etc/systemd/system/${SERVICE_NAME}.service"
}

setup_user() {
  if ! id "${SERVICE_USER}" >/dev/null 2>&1; then
    log "创建系统用户 ${SERVICE_USER}"
    useradd --system --no-create-home --shell /usr/sbin/nologin "${SERVICE_USER}"
  fi
  install -d -o "${SERVICE_USER}" -g "${SERVICE_GROUP}" /var/lib/himqtt
  chown -R "${SERVICE_USER}:${SERVICE_GROUP}" "${CONFIG_DIR}" /var/lib/himqtt
}

ensure_ufw_port() {
  local port="$1"
  if ! command -v ufw >/dev/null 2>&1; then
    log "未安装 ufw，跳过防火墙配置（端口 ${port}）"
    return 0
  fi

  local status
  status="$(ufw status 2>/dev/null || true)"
  if ! grep -qi "Status: active" <<<"${status}"; then
    log "ufw 未启用，跳过端口 ${port}（如需启用: ufw enable）"
    return 0
  fi

  if grep -qE "(^|[[:space:]])${port}/tcp([[:space:]]|$)" <<<"${status}"; then
    log "ufw 已放行 ${port}/tcp"
    return 0
  fi

  log "ufw 放行 ${port}/tcp"
  ufw allow "${port}/tcp" comment "HiMQTT ${port}" >/dev/null
}

check_listen() {
  local port="$1"
  local expect_local="${2:-any}"

  if ss -tln | awk '{print $4}' | grep -qE ":${port}$"; then
    log "端口 ${port} 正在监听"
    if [[ "${expect_local}" == "local" ]]; then
      if ss -tln | grep -q "127.0.0.1:${port}"; then
        log "端口 ${port} 绑定 127.0.0.1（符合预期）"
      else
        log "警告: 端口 ${port} 未绑定 127.0.0.1"
      fi
    fi
    return 0
  fi

  log "错误: 端口 ${port} 未监听"
  return 1
}

restart_service() {
  log "重启 systemd 服务 ${SERVICE_NAME}"
  systemctl daemon-reload
  systemctl enable "${SERVICE_NAME}" >/dev/null 2>&1 || true
  systemctl restart "${SERVICE_NAME}"
  sleep 2
  systemctl is-active --quiet "${SERVICE_NAME}"
  log "服务状态: $(systemctl is-active "${SERVICE_NAME}")"
}

smoke_test() {
  log "本机 MQTT 冒烟测试"
  if command -v python3 >/dev/null 2>&1 && [[ -f "${DEPLOY_DIR}/mqtt_smoke_test.py" ]]; then
    python3 "${DEPLOY_DIR}/mqtt_smoke_test.py" || {
      log "警告: Python 冒烟测试失败"
      return 0
    }
    log "Python 冒烟测试通过"
    return 0
  fi

  if ss -tln | grep -q ":1883"; then
    log "1883 已监听（未安装 python3 测试脚本，跳过详细 MQTT 测试）"
  fi
}

main() {
  require_root
  [[ -x "${DEPLOY_DIR}/himqtt" ]] || { log "缺少 ${DEPLOY_DIR}/himqtt"; exit 1; }

  install_files
  setup_user

  log "检查并配置防火墙"
  for port in "${PUBLIC_PORTS[@]}"; do
    ensure_ufw_port "${port}"
  done

  restart_service

  log "检查服务端口"
  failed=0
  for port in "${PUBLIC_PORTS[@]}"; do
    check_listen "${port}" "public" || failed=1
  done
  for port in "${LOCAL_PORTS[@]}"; do
    check_listen "${port}" "local" || failed=1
  done

  smoke_test

  if [[ "${failed}" -ne 0 ]]; then
    log "部署完成，但部分端口检测失败"
    journalctl -u "${SERVICE_NAME}" -n 30 --no-pager || true
    exit 1
  fi

  log "部署成功"
  log "MQTT v4:  ${DEPLOY_HOST:-<server-ip>}:1883"
  log "MQTT v5:  ${DEPLOY_HOST:-<server-ip>}:1884"
  log "WebSocket: ${DEPLOY_HOST:-<server-ip>}:8083"
  log "监控页(本机): http://127.0.0.1:8090/"
}

main "$@"
