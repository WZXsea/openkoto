CREATE TABLE legacy_import_batches (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    client_import_id TEXT NOT NULL,
    request_sha256 TEXT NOT NULL,
    schema_version TEXT NOT NULL,
    source_label TEXT,
    status TEXT NOT NULL,
    total_items INTEGER NOT NULL DEFAULT 0,
    imported_items INTEGER NOT NULL DEFAULT 0,
    skipped_items INTEGER NOT NULL DEFAULT 0,
    failed_items INTEGER NOT NULL DEFAULT 0,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    finished_at TIMESTAMPTZ,
    CONSTRAINT legacy_import_batches_status_valid CHECK (
        status IN ('running', 'completed', 'completed_with_errors')
    ),
    CONSTRAINT legacy_import_batches_counts_non_negative CHECK (
        total_items >= 0 AND imported_items >= 0 AND skipped_items >= 0 AND failed_items >= 0
    ),
    CONSTRAINT legacy_import_batches_client_import_id_not_empty CHECK (
        char_length(btrim(client_import_id)) > 0
    ),
    CONSTRAINT legacy_import_batches_request_sha256_not_empty CHECK (
        char_length(btrim(request_sha256)) > 0
    ),
    CONSTRAINT legacy_import_batches_schema_version_not_empty CHECK (
        char_length(btrim(schema_version)) > 0
    ),
    CONSTRAINT legacy_import_batches_user_id_id_unique UNIQUE (user_id, id)
);

CREATE UNIQUE INDEX legacy_import_batches_user_client_idx
    ON legacy_import_batches (user_id, client_import_id);
CREATE INDEX legacy_import_batches_user_created_idx
    ON legacy_import_batches (user_id, created_at DESC);

CREATE TABLE legacy_import_items (
    id UUID PRIMARY KEY,
    batch_id UUID NOT NULL REFERENCES legacy_import_batches(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    source_kind TEXT NOT NULL,
    source_id TEXT NOT NULL,
    target_kind TEXT,
    target_id TEXT,
    status TEXT NOT NULL,
    error TEXT,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT legacy_import_items_source_kind_not_empty CHECK (char_length(btrim(source_kind)) > 0),
    CONSTRAINT legacy_import_items_source_id_not_empty CHECK (char_length(btrim(source_id)) > 0),
    CONSTRAINT legacy_import_items_status_valid CHECK (status IN ('imported', 'skipped', 'failed'))
);

ALTER TABLE legacy_import_items
    ADD CONSTRAINT legacy_import_items_user_batch_fk
    FOREIGN KEY (user_id, batch_id)
    REFERENCES legacy_import_batches(user_id, id)
    ON DELETE CASCADE;

CREATE UNIQUE INDEX legacy_import_items_batch_source_idx
    ON legacy_import_items (batch_id, source_kind, source_id);
CREATE INDEX legacy_import_items_user_batch_idx
    ON legacy_import_items (user_id, batch_id);
