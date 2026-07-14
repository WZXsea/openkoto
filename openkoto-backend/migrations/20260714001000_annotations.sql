CREATE UNIQUE INDEX IF NOT EXISTS material_segments_user_material_id_idx
    ON material_segments (user_id, material_id, id);

CREATE TABLE IF NOT EXISTS annotations (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    material_id UUID NOT NULL,
    segment_id UUID,
    kind TEXT NOT NULL,
    locator JSONB NOT NULL,
    source_text TEXT NOT NULL,
    material_revision TEXT,
    content_sha256 TEXT,
    color TEXT,
    note TEXT,
    tags JSONB NOT NULL DEFAULT '[]'::jsonb,
    learning_item_id UUID,
    client_request_id TEXT NOT NULL,
    idempotency_payload_sha256 TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT annotations_user_id_id_unique UNIQUE (user_id, id),
    CONSTRAINT annotations_user_material_fk
        FOREIGN KEY (user_id, material_id)
        REFERENCES materials(user_id, id)
        ON DELETE CASCADE,
    CONSTRAINT annotations_user_material_segment_fk
        FOREIGN KEY (user_id, material_id, segment_id)
        REFERENCES material_segments(user_id, material_id, id)
        ON DELETE SET NULL (segment_id),
    CONSTRAINT annotations_user_learning_item_fk
        FOREIGN KEY (user_id, learning_item_id)
        REFERENCES learning_items(user_id, id)
        ON DELETE SET NULL (learning_item_id),
    CONSTRAINT annotations_kind_valid CHECK (
        kind IN ('highlight', 'excerpt', 'note', 'vocabulary', 'grammar')
    ),
    CONSTRAINT annotations_locator_object CHECK (jsonb_typeof(locator) = 'object'),
    CONSTRAINT annotations_source_text_valid CHECK (
        kind = 'note' OR char_length(btrim(source_text)) > 0
    ),
    CONSTRAINT annotations_material_revision_not_empty CHECK (
        material_revision IS NULL OR char_length(btrim(material_revision)) > 0
    ),
    CONSTRAINT annotations_content_sha256_format CHECK (
        content_sha256 IS NULL OR content_sha256 ~ '^[0-9a-f]{64}$'
    ),
    CONSTRAINT annotations_color_not_empty CHECK (
        color IS NULL OR char_length(btrim(color)) > 0
    ),
    CONSTRAINT annotations_tags_array CHECK (jsonb_typeof(tags) = 'array'),
    CONSTRAINT annotations_client_request_id_not_empty CHECK (
        char_length(btrim(client_request_id)) > 0
        AND char_length(client_request_id) <= 255
    ),
    CONSTRAINT annotations_idempotency_payload_sha256_format CHECK (
        idempotency_payload_sha256 ~ '^[0-9a-f]{64}$'
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS annotations_user_client_request_id_idx
    ON annotations (user_id, client_request_id)
    WHERE client_request_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS annotations_user_updated_at_idx
    ON annotations (user_id, updated_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS annotations_user_material_updated_at_idx
    ON annotations (user_id, material_id, updated_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS annotations_user_kind_updated_at_idx
    ON annotations (user_id, kind, updated_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS annotations_user_learning_item_idx
    ON annotations (user_id, learning_item_id)
    WHERE learning_item_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS annotations_user_material_segment_idx
    ON annotations (user_id, material_id, segment_id)
    WHERE segment_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS annotations_tags_gin_idx
    ON annotations USING GIN (tags);
