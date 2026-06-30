#!/usr/bin/env bash
set -euo pipefail

PG_VERSION="${PG_VERSION:-16}"
PG_CONF="/etc/postgresql/${PG_VERSION}/main/postgresql.conf"
DB_USER="${DB_USER:-himqtt}"
DB_PASS="${DB_PASS:-himqtt}"
DB_NAME="${DB_NAME:-himqtt}"

if [[ "${EUID}" -ne 0 ]]; then
  echo "请使用 root 运行: sudo $0"
  exit 1
fi

if [[ ! -f "${PG_CONF}" ]]; then
  echo "未找到 ${PG_CONF}"
  exit 1
fi

echo "==> 修改端口为 5432"
sed -i 's/^#\?port = .*/port = 5432/' "${PG_CONF}"
grep '^port' "${PG_CONF}"

echo "==> 重启 PostgreSQL"
systemctl restart postgresql
sleep 2

if ! pg_isready -h 127.0.0.1 -p 5432 >/dev/null; then
  echo "5432 未就绪，请检查: systemctl status postgresql"
  exit 1
fi
echo "5432 已就绪"

echo "==> 创建用户: ${DB_USER}"
if sudo -u postgres psql -p 5432 -tAc "SELECT 1 FROM pg_roles WHERE rolname='${DB_USER}'" | grep -q 1; then
  sudo -u postgres psql -p 5432 -c "ALTER ROLE ${DB_USER} WITH LOGIN PASSWORD '${DB_PASS}';"
else
  sudo -u postgres psql -p 5432 -c "CREATE ROLE ${DB_USER} WITH LOGIN PASSWORD '${DB_PASS}';"
fi

echo "==> 创建数据库: ${DB_NAME}"
if sudo -u postgres psql -p 5432 -tAc "SELECT 1 FROM pg_database WHERE datname='${DB_NAME}'" | grep -q 1; then
  echo "数据库 ${DB_NAME} 已存在"
else
  sudo -u postgres psql -p 5432 -c "CREATE DATABASE ${DB_NAME} OWNER ${DB_USER};"
fi

sudo -u postgres psql -p 5432 -c "GRANT ALL PRIVILEGES ON DATABASE ${DB_NAME} TO ${DB_USER};"

echo "==> 连接测试"
PGPASSWORD="${DB_PASS}" psql -h 127.0.0.1 -p 5432 -U "${DB_USER}" -d "${DB_NAME}" -c \
  "SELECT current_user, current_database(), version();"

echo
echo "完成。连接字符串:"
echo "postgresql://${DB_USER}:${DB_PASS}@127.0.0.1:5432/${DB_NAME}"
