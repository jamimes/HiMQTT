CREATE TABLE IF NOT EXISTS mqtt_user_categories (
    id SERIAL PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE mqtt_users
    ADD COLUMN IF NOT EXISTS category_id INTEGER REFERENCES mqtt_user_categories(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS last_login_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS last_login_ip TEXT;

CREATE INDEX IF NOT EXISTS idx_mqtt_users_category_id ON mqtt_users(category_id);

ALTER TABLE mqtt_topics
    ADD COLUMN IF NOT EXISTS owner_username TEXT REFERENCES mqtt_users(username) ON DELETE CASCADE;

CREATE INDEX IF NOT EXISTS idx_mqtt_topics_owner ON mqtt_topics(owner_username);

CREATE TABLE IF NOT EXISTS system_settings (
    key TEXT PRIMARY KEY,
    value JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO system_settings (key, value)
VALUES (
    'user_defaults',
    '{
      "default_topics": [
        {"topic": "devices/{username}/data", "description": "设备上行数据"},
        {"topic": "devices/{username}/cmd", "description": "设备下行指令"}
      ],
      "default_acls": [
        {"topic_pattern": "devices/{username}/#", "can_subscribe": true, "can_publish": true}
      ]
    }'::jsonb
)
ON CONFLICT (key) DO NOTHING;
