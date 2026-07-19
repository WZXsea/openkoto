CREATE TABLE IF NOT EXISTS backend_metadata (
    key TEXT PRIMARY KEY,
    value JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO backend_metadata (key, value)
VALUES ('schema', '{"name":"openkoto-backend","phase":"pr-4.1"}'::jsonb)
ON CONFLICT (key)
DO UPDATE SET value = EXCLUDED.value, updated_at = NOW();
