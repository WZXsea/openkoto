CREATE TABLE IF NOT EXISTS material_document_migration_reports (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    material_id UUID NOT NULL,
    source_authority TEXT NOT NULL,
    legacy_content_sha256 TEXT,
    legacy_segments_sha256 TEXT,
    canonical_content_sha256 TEXT,
    content_mismatch BOOLEAN NOT NULL DEFAULT FALSE,
    legacy_content_backup TEXT,
    legacy_segment_count INTEGER NOT NULL DEFAULT 0,
    generated_block_count INTEGER NOT NULL DEFAULT 0,
    details JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, material_id),
    CONSTRAINT material_document_migration_reports_material_fk
        FOREIGN KEY (user_id, material_id)
        REFERENCES materials(user_id, id)
        ON DELETE CASCADE,
    CONSTRAINT material_document_migration_reports_source_valid CHECK (
        source_authority IN ('content', 'segments')
    ),
    CONSTRAINT material_document_migration_reports_counts_non_negative CHECK (
        legacy_segment_count >= 0 AND generated_block_count >= 0
    ),
    CONSTRAINT material_document_migration_reports_content_hash_format CHECK (
        legacy_content_sha256 IS NULL OR legacy_content_sha256 ~ '^[0-9a-f]{64}$'
    ),
    CONSTRAINT material_document_migration_reports_segments_hash_format CHECK (
        legacy_segments_sha256 IS NULL OR legacy_segments_sha256 ~ '^[0-9a-f]{64}$'
    ),
    CONSTRAINT material_document_migration_reports_canonical_hash_format CHECK (
        canonical_content_sha256 IS NULL OR canonical_content_sha256 ~ '^[0-9a-f]{64}$'
    ),
    CONSTRAINT material_document_migration_reports_details_object CHECK (
        jsonb_typeof(details) = 'object'
    )
);

CREATE INDEX IF NOT EXISTS material_document_migration_reports_mismatch_idx
    ON material_document_migration_reports (user_id, content_mismatch, created_at DESC)
    WHERE content_mismatch = TRUE;
