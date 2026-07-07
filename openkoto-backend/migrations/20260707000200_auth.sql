CREATE TABLE users (
    id UUID PRIMARY KEY,
    email TEXT NOT NULL,
    email_normalized TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    display_name TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT users_email_normalized_not_empty CHECK (char_length(email_normalized) > 0),
    CONSTRAINT users_password_hash_not_empty CHECK (char_length(password_hash) > 0)
);

CREATE INDEX users_created_at_idx ON users (created_at DESC);

CREATE TABLE sessions (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    last_used_at TIMESTAMPTZ,
    CONSTRAINT sessions_token_hash_not_empty CHECK (char_length(token_hash) > 0),
    CONSTRAINT sessions_expires_after_created CHECK (expires_at > created_at)
);

CREATE INDEX sessions_user_id_idx ON sessions (user_id);
CREATE INDEX sessions_active_token_hash_idx ON sessions (token_hash) WHERE revoked_at IS NULL;
