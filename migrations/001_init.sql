CREATE TABLE IF NOT EXISTS admin_users (
    id SERIAL PRIMARY KEY,
    username TEXT UNIQUE NOT NULL,
    password_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS mqtt_users (
    id SERIAL PRIMARY KEY,
    username TEXT UNIQUE NOT NULL,
    password_hash TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS mqtt_topics (
    id SERIAL PRIMARY KEY,
    topic TEXT UNIQUE NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS topic_acl (
    id SERIAL PRIMARY KEY,
    username TEXT NOT NULL REFERENCES mqtt_users(username) ON DELETE CASCADE,
    topic_pattern TEXT NOT NULL,
    can_subscribe BOOLEAN NOT NULL DEFAULT FALSE,
    can_publish BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(username, topic_pattern)
);

CREATE INDEX IF NOT EXISTS idx_topic_acl_username ON topic_acl(username);
