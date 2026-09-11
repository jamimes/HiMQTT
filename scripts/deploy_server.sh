#!/usr/bin/env bash
set -euo pipefail

DEPLOY_DIR="${1:-/tmp/himqtt-deploy}"
SERVICE_NAME="himqtt"
BIN_PATH="/usr/local/bin/himqtt"
CONFIG_DIR="/etc/himqtt"
CONFIG_PATH="${CONFIG_DIR}/himqtt.toml"
SERVICE_USER="himqtt"
SERVICE_GROUP="himqtt"
DATA_DIR="/var/lib/himqtt"
ADMIN_STATIC_DIR="${DATA_DIR}/admin-web/dist"

# 对外服务端口（需在防火墙放行）
PUBLIC_PORTS=(1883 1884 8083)
# 本机监听端口（仅检测是否在听）
LOCAL_PORTS=(8091 3030 9042)

DB_USER="${DB_USER:-himqtt}"
DB_PASS="${DB_PASS:-himqtt}"
DB_NAME="${DB_NAME:-himqtt}"

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
  install -d -o "${SERVICE_USER}" -g "${SERVICE_GROUP}" "${DATA_DIR}"
  install -m 755 "${DEPLOY_DIR}/himqtt" "${BIN_PATH}"
  install -m 644 "${DEPLOY_DIR}/himqtt.toml" "${CONFIG_PATH}"
  install -m 644 "${DEPLOY_DIR}/himqtt.service" "/etc/systemd/system/${SERVICE_NAME}.service"

  # 管理后台静态资源（绝对路径，避免 WorkingDirectory 相对路径失效）
  if [[ -d "${DEPLOY_DIR}/admin-web-dist" ]]; then
    log "安装管理后台静态资源 -> ${ADMIN_STATIC_DIR}"
    rm -rf "${ADMIN_STATIC_DIR}"
    install -d -o "${SERVICE_USER}" -g "${SERVICE_GROUP}" "${ADMIN_STATIC_DIR}"
    cp -a "${DEPLOY_DIR}/admin-web-dist/." "${ADMIN_STATIC_DIR}/"
    chown -R "${SERVICE_USER}:${SERVICE_GROUP}" "${DATA_DIR}/admin-web"
  else
    log "警告: 部署包中无 admin-web-dist，管理后台页面可能空白"
    install -d -o "${SERVICE_USER}" -g "${SERVICE_GROUP}" "${ADMIN_STATIC_DIR}"
  fi

  # 将配置中的 static_dir 改为部署绝对路径
  if grep -q '^static_dir' "${CONFIG_PATH}"; then
    sed -i "s|^static_dir *=.*|static_dir = \"${ADMIN_STATIC_DIR}\"|" "${CONFIG_PATH}"
  else
    printf '\nstatic_dir = "%s"\n' "${ADMIN_STATIC_DIR}" >> "${CONFIG_PATH}"
  fi
}

setup_user() {
  if ! id "${SERVICE_USER}" >/dev/null 2>&1; then
    log "创建系统用户 ${SERVICE_USER}"
    useradd --system --no-create-home --shell /usr/sbin/nologin "${SERVICE_USER}"
  fi
  install -d -o "${SERVICE_USER}" -g "${SERVICE_GROUP}" "${DATA_DIR}"
  chown -R "${SERVICE_USER}:${SERVICE_GROUP}" "${CONFIG_DIR}" "${DATA_DIR}"
}

ensure_postgres() {
  log "检查 PostgreSQL（ACL / 管理后台依赖）"
  if ! command -v psql >/dev/null 2>&1 && ! command -v pg_isready >/dev/null 2>&1; then
    log "错误: 未检测到 PostgreSQL 客户端/服务"
    log "请先在服务器安装 PostgreSQL，并执行: scripts/setup_postgres_native.sh"
    exit 1
  fi

  if command -v pg_isready >/dev/null 2>&1; then
    if ! pg_isready -h 127.0.0.1 -p 5432 >/dev/null 2>&1; then
      if systemctl list-unit-files 2>/dev/null | grep -q '^postgresql'; then
        log "尝试启动 postgresql"
        systemctl start postgresql || true
        sleep 2
      fi
    fi
    if ! pg_isready -h 127.0.0.1 -p 5432 >/dev/null 2>&1; then
      log "错误: PostgreSQL 5432 未就绪，himqtt 启用 ACL 时无法启动"
      log "请先运行: sudo bash ${DEPLOY_DIR}/setup_postgres_native.sh"
      exit 1
    fi
  fi

  # 若有 postgres 超级用户，尝试确保业务库存在
  if id postgres >/dev/null 2>&1; then
    if sudo -u postgres psql -p 5432 -tAc "SELECT 1 FROM pg_roles WHERE rolname='${DB_USER}'" 2>/dev/null | grep -q 1; then
      sudo -u postgres psql -p 5432 -c "ALTER ROLE ${DB_USER} WITH LOGIN PASSWORD '${DB_PASS}';" >/dev/null
    else
      log "创建数据库角色 ${DB_USER}"
      sudo -u postgres psql -p 5432 -c "CREATE ROLE ${DB_USER} WITH LOGIN PASSWORD '${DB_PASS}';" >/dev/null
    fi
    if ! sudo -u postgres psql -p 5432 -tAc "SELECT 1 FROM pg_database WHERE datname='${DB_NAME}'" 2>/dev/null | grep -q 1; then
      log "创建数据库 ${DB_NAME}"
      sudo -u postgres psql -p 5432 -c "CREATE DATABASE ${DB_NAME} OWNER ${DB_USER};" >/dev/null
    fi
    sudo -u postgres psql -p 5432 -c "GRANT ALL PRIVILEGES ON DATABASE ${DB_NAME} TO ${DB_USER};" >/dev/null || true
  fi

  log "PostgreSQL 检查通过 (${DB_USER}@127.0.0.1:5432/${DB_NAME})"
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

dump_service_logs() {
  log "---- journalctl -u ${SERVICE_NAME} (最近 40 行) ----"
  journalctl -u "${SERVICE_NAME}" -n 40 --no-pager || true
  log "---- systemctl status ----"
  systemctl status "${SERVICE_NAME}" --no-pager -l || true
}

restart_service() {
  log "重启 systemd 服务 ${SERVICE_NAME}"
  systemctl daemon-reload
  systemctl enable "${SERVICE_NAME}" >/dev/null 2>&1 || true

  if ! systemctl restart "${SERVICE_NAME}"; then
    log "systemctl restart 失败"
    dump_service_logs
    exit 1
  fi

  sleep 3
  if ! systemctl is-active --quiet "${SERVICE_NAME}"; then
    log "服务未处于 active（常见原因: PostgreSQL 连不上 / 配置错误）"
    dump_service_logs
    exit 1
  fi
  log "服务状态: $(systemctl is-active "${SERVICE_NAME}")"
}

smoke_test() {
  log "本机 MQTT 冒烟测试"
  if command -v python3 >/dev/null 2>&1 && [[ -f "${DEPLOY_DIR}/mqtt_smoke_test.py" ]]; then
    python3 "${DEPLOY_DIR}/mqtt_smoke_test.py" || {
      log "警告: Python 冒烟测试失败（启用 ACL 后未认证连接会被拒绝，属正常）"
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

  setup_user
  install_files
  ensure_postgres

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
    dump_service_logs
    exit 1
  fi

  log "部署成功"
  log "MQTT v4:  ${DEPLOY_HOST:-<server-ip>}:1883"
  log "MQTT v5:  ${DEPLOY_HOST:-<server-ip>}:1884"
  log "WebSocket: ${DEPLOY_HOST:-<server-ip>}:8083"
  log "管理后台(本机): http://127.0.0.1:8091/  (admin / admin123)"
}

main "$@"
