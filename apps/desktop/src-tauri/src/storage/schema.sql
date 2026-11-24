PRAGMA user_version = 1;

CREATE TABLE IF NOT EXISTS subscriptions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    url TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    status TEXT NOT NULL DEFAULT 'inactive' CHECK (status IN ('active', 'inactive', 'error')),
    auto_refetch_minutes INTEGER NOT NULL DEFAULT 0,
    last_fetched_at TEXT,
    last_error TEXT,
    user_agent TEXT,
    request_headers TEXT,
    sort INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_subscriptions_sort ON subscriptions(sort);

CREATE TABLE IF NOT EXISTS profiles (
    id TEXT PRIMARY KEY,
    subscription_id TEXT NOT NULL REFERENCES subscriptions(id) ON DELETE CASCADE,
    server_id TEXT NOT NULL,
    link TEXT NOT NULL,
    remarks TEXT,
    protocol TEXT,
    address TEXT,
    port INTEGER,
    raw_config TEXT NOT NULL DEFAULT '{}',
    present INTEGER NOT NULL DEFAULT 1,
    latency_ms INTEGER,
    reachable INTEGER NOT NULL DEFAULT 0,
    checked_at TEXT,
    sort_index INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (subscription_id, server_id)
);

CREATE INDEX IF NOT EXISTS idx_profiles_present_sort ON profiles(present, sort_index);
CREATE INDEX IF NOT EXISTS idx_profiles_subscription ON profiles(subscription_id);

CREATE TABLE IF NOT EXISTS service_profiles (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    test_urls TEXT NOT NULL DEFAULT '[]',
    is_enabled INTEGER NOT NULL DEFAULT 1,
    is_default INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_service_profiles_enabled ON service_profiles(is_enabled);

CREATE TABLE IF NOT EXISTS profile_service_results (
    profile_id TEXT NOT NULL,
    service_profile_id TEXT NOT NULL,
    reachable INTEGER NOT NULL DEFAULT 0,
    checked_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (profile_id, service_profile_id)
);

CREATE TABLE IF NOT EXISTS settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    auto_refetch_enabled INTEGER NOT NULL DEFAULT 0,
    refetch_interval_minutes INTEGER NOT NULL DEFAULT 15,
    auto_test_enabled INTEGER NOT NULL DEFAULT 0,
    test_interval_minutes INTEGER NOT NULL DEFAULT 15,
    test_timeout_ms INTEGER NOT NULL DEFAULT 5000,
    auto_reconnect_enabled INTEGER NOT NULL DEFAULT 0,
    reconnect_cooldown_seconds INTEGER NOT NULL DEFAULT 60,
    ping_url TEXT NOT NULL DEFAULT 'https://www.google.com/generate_204',
    local_proxy_port INTEGER NOT NULL DEFAULT 10808,
    proxy_exclusions TEXT NOT NULL DEFAULT '[]',
    system_proxy_enabled INTEGER NOT NULL DEFAULT 1
);

INSERT OR IGNORE INTO settings (id) VALUES (1);
