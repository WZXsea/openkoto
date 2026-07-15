ALTER TABLE materials
    ADD COLUMN IF NOT EXISTS current_revision BIGINT NOT NULL DEFAULT 1,
    ADD COLUMN IF NOT EXISTS content_updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ADD COLUMN IF NOT EXISTS editable_source_material_id UUID;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'materials_editable_source_fk'
          AND conrelid = 'materials'::regclass
    ) THEN
        ALTER TABLE materials
            ADD CONSTRAINT materials_editable_source_fk
            FOREIGN KEY (user_id, editable_source_material_id)
            REFERENCES materials(user_id, id)
            ON DELETE SET NULL (editable_source_material_id);
    END IF;
END
$$;

CREATE UNIQUE INDEX IF NOT EXISTS materials_user_editable_source_idx
    ON materials (user_id, editable_source_material_id)
    WHERE editable_source_material_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS material_blocks (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    material_id UUID NOT NULL,
    block_order INTEGER NOT NULL,
    block_type TEXT NOT NULL DEFAULT 'paragraph',
    attrs JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ,
    CONSTRAINT material_blocks_user_material_fk
        FOREIGN KEY (user_id, material_id)
        REFERENCES materials(user_id, id)
        ON DELETE CASCADE,
    CONSTRAINT material_blocks_type_valid CHECK (
        block_type IN ('paragraph', 'heading', 'list_item', 'quote')
    ),
    CONSTRAINT material_blocks_order_non_negative CHECK (block_order >= 0),
    CONSTRAINT material_blocks_attrs_object CHECK (jsonb_typeof(attrs) = 'object'),
    CONSTRAINT material_blocks_user_material_id_unique UNIQUE (user_id, material_id, id)
);

CREATE UNIQUE INDEX IF NOT EXISTS material_blocks_live_order_idx
    ON material_blocks (material_id, block_order)
    WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS material_blocks_user_material_idx
    ON material_blocks (user_id, material_id, deleted_at, block_order);

ALTER TABLE material_segments
    ADD COLUMN IF NOT EXISTS block_id UUID,
    ADD COLUMN IF NOT EXISTS block_segment_order INTEGER,
    ADD COLUMN IF NOT EXISTS text_sha256 TEXT,
    ADD COLUMN IF NOT EXISTS reading_source_sha256 TEXT,
    ADD COLUMN IF NOT EXISTS translation_source_sha256 TEXT,
    ADD COLUMN IF NOT EXISTS explanation_source_sha256 TEXT,
    ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'material_segments_user_block_fk'
          AND conrelid = 'material_segments'::regclass
    ) THEN
        ALTER TABLE material_segments
            ADD CONSTRAINT material_segments_user_block_fk
            FOREIGN KEY (user_id, material_id, block_id)
            REFERENCES material_blocks(user_id, material_id, id);
    END IF;
END
$$;

ALTER TABLE material_segments
    DROP CONSTRAINT IF EXISTS material_segments_order_non_negative;

ALTER TABLE material_segments
    ADD CONSTRAINT material_segments_order_non_negative CHECK (segment_order >= 0),
    ADD CONSTRAINT material_segments_block_order_non_negative CHECK (
        block_segment_order IS NULL OR block_segment_order >= 0
    ),
    ADD CONSTRAINT material_segments_text_sha256_format CHECK (
        text_sha256 IS NULL OR text_sha256 ~ '^[0-9a-f]{64}$'
    ),
    ADD CONSTRAINT material_segments_reading_sha256_format CHECK (
        reading_source_sha256 IS NULL OR reading_source_sha256 ~ '^[0-9a-f]{64}$'
    ),
    ADD CONSTRAINT material_segments_translation_sha256_format CHECK (
        translation_source_sha256 IS NULL OR translation_source_sha256 ~ '^[0-9a-f]{64}$'
    ),
    ADD CONSTRAINT material_segments_explanation_sha256_format CHECK (
        explanation_source_sha256 IS NULL OR explanation_source_sha256 ~ '^[0-9a-f]{64}$'
    );

DROP INDEX IF EXISTS material_segments_material_order_idx;
CREATE UNIQUE INDEX IF NOT EXISTS material_segments_live_order_idx
    ON material_segments (material_id, segment_order)
    WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS material_segments_user_block_idx
    ON material_segments (user_id, material_id, block_id, deleted_at, block_segment_order);

CREATE TABLE IF NOT EXISTS material_revisions (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    material_id UUID NOT NULL,
    revision BIGINT NOT NULL,
    parent_revision BIGINT,
    action TEXT NOT NULL,
    content_sha256 TEXT,
    snapshot JSONB NOT NULL,
    change_summary JSONB NOT NULL DEFAULT '{}'::jsonb,
    client_request_id TEXT,
    request_sha256 TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT material_revisions_user_material_fk
        FOREIGN KEY (user_id, material_id)
        REFERENCES materials(user_id, id)
        ON DELETE CASCADE,
    CONSTRAINT material_revisions_revision_positive CHECK (revision >= 1),
    CONSTRAINT material_revisions_action_valid CHECK (
        action IN ('migration', 'edit', 'restore')
    ),
    CONSTRAINT material_revisions_snapshot_object CHECK (jsonb_typeof(snapshot) = 'object'),
    CONSTRAINT material_revisions_summary_object CHECK (jsonb_typeof(change_summary) = 'object'),
    CONSTRAINT material_revisions_content_sha256_format CHECK (
        content_sha256 IS NULL OR content_sha256 ~ '^[0-9a-f]{64}$'
    ),
    CONSTRAINT material_revisions_request_sha256_format CHECK (
        request_sha256 IS NULL OR request_sha256 ~ '^[0-9a-f]{64}$'
    ),
    CONSTRAINT material_revisions_user_material_revision_unique
        UNIQUE (user_id, material_id, revision)
);

CREATE UNIQUE INDEX IF NOT EXISTS material_revisions_user_client_request_idx
    ON material_revisions (user_id, client_request_id)
    WHERE client_request_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS material_revisions_user_material_created_idx
    ON material_revisions (user_id, material_id, revision DESC);

CREATE TABLE IF NOT EXISTS material_edit_drafts (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    material_id UUID NOT NULL,
    base_revision BIGINT NOT NULL,
    blocks JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, material_id),
    CONSTRAINT material_edit_drafts_user_material_fk
        FOREIGN KEY (user_id, material_id)
        REFERENCES materials(user_id, id)
        ON DELETE CASCADE,
    CONSTRAINT material_edit_drafts_base_revision_positive CHECK (base_revision >= 1),
    CONSTRAINT material_edit_drafts_blocks_array CHECK (jsonb_typeof(blocks) = 'array')
);

ALTER TABLE learning_items
    ADD COLUMN IF NOT EXISTS source_segment_sha256 TEXT;

ALTER TABLE learning_items
    ADD CONSTRAINT learning_items_source_segment_sha256_format CHECK (
        source_segment_sha256 IS NULL OR source_segment_sha256 ~ '^[0-9a-f]{64}$'
    );

UPDATE learning_items li
SET source_segment_sha256 = ms.text_sha256
FROM material_segments ms
WHERE li.user_id = ms.user_id
  AND li.segment_id = ms.id
  AND li.source_segment_sha256 IS NULL
  AND ms.text_sha256 IS NOT NULL;

CREATE OR REPLACE FUNCTION capture_learning_item_segment_hash()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.segment_id IS NULL THEN
        NEW.source_segment_sha256 := NULL;
    ELSIF TG_OP = 'INSERT' OR NEW.segment_id IS DISTINCT FROM OLD.segment_id THEN
        SELECT ms.text_sha256 INTO NEW.source_segment_sha256
        FROM material_segments ms
        WHERE ms.user_id = NEW.user_id AND ms.id = NEW.segment_id;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS learning_items_segment_hash_trigger ON learning_items;
CREATE TRIGGER learning_items_segment_hash_trigger
    BEFORE INSERT OR UPDATE OF segment_id ON learning_items
    FOR EACH ROW EXECUTE FUNCTION capture_learning_item_segment_hash();
