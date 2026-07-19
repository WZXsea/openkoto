ALTER TABLE learning_items
    ADD COLUMN IF NOT EXISTS quality_flags JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN IF NOT EXISTS status_before_archive TEXT,
    ADD COLUMN IF NOT EXISTS merged_into_id UUID,
    ADD COLUMN IF NOT EXISTS source_material_title_snapshot TEXT,
    ADD COLUMN IF NOT EXISTS source_type_snapshot TEXT;

UPDATE learning_items li
SET source_material_title_snapshot = COALESCE(li.source_material_title_snapshot, m.title),
    source_type_snapshot = COALESCE(li.source_type_snapshot, m.source_type)
FROM materials m
WHERE li.user_id = m.user_id AND li.material_id = m.id
  AND (li.source_material_title_snapshot IS NULL OR li.source_type_snapshot IS NULL);

CREATE OR REPLACE FUNCTION preserve_learning_item_source_snapshot()
RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'INSERT' AND NEW.material_id IS NOT NULL THEN
        SELECT COALESCE(NEW.source_material_title_snapshot, m.title),
               COALESCE(NEW.source_type_snapshot, m.source_type)
        INTO NEW.source_material_title_snapshot, NEW.source_type_snapshot
        FROM materials m
        WHERE m.user_id = NEW.user_id AND m.id = NEW.material_id;
    ELSIF TG_OP = 'UPDATE'
          AND NEW.material_id IS DISTINCT FROM OLD.material_id
          AND NEW.material_id IS NOT NULL THEN
        SELECT m.title, m.source_type
        INTO NEW.source_material_title_snapshot, NEW.source_type_snapshot
        FROM materials m
        WHERE m.user_id = NEW.user_id AND m.id = NEW.material_id;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS learning_items_source_snapshot_trigger ON learning_items;
CREATE TRIGGER learning_items_source_snapshot_trigger
    BEFORE INSERT OR UPDATE OF material_id ON learning_items
    FOR EACH ROW EXECUTE FUNCTION preserve_learning_item_source_snapshot();

ALTER TABLE learning_items
    DROP CONSTRAINT IF EXISTS learning_items_user_material_fk;
ALTER TABLE learning_items
    ADD CONSTRAINT learning_items_user_material_fk
    FOREIGN KEY (user_id, material_id)
    REFERENCES materials(user_id, id)
    ON DELETE SET NULL (material_id);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'learning_items_quality_flags_array'
          AND conrelid = 'learning_items'::regclass
    ) THEN
        ALTER TABLE learning_items
            ADD CONSTRAINT learning_items_quality_flags_array
            CHECK (jsonb_typeof(quality_flags) = 'array');
    END IF;
END
$$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'learning_items_status_before_archive_valid'
          AND conrelid = 'learning_items'::regclass
    ) THEN
        ALTER TABLE learning_items
            ADD CONSTRAINT learning_items_status_before_archive_valid
            CHECK (
                status_before_archive IS NULL OR
                status_before_archive IN ('candidate', 'accepted', 'rejected')
            );
    END IF;
END
$$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'learning_items_merged_into_fk'
          AND conrelid = 'learning_items'::regclass
    ) THEN
        ALTER TABLE learning_items
            ADD CONSTRAINT learning_items_merged_into_fk
            FOREIGN KEY (user_id, merged_into_id)
            REFERENCES learning_items(user_id, id)
            ON DELETE SET NULL (merged_into_id);
    END IF;
END
$$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'learning_items_not_merged_into_self'
          AND conrelid = 'learning_items'::regclass
    ) THEN
        ALTER TABLE learning_items
            ADD CONSTRAINT learning_items_not_merged_into_self
            CHECK (merged_into_id IS NULL OR merged_into_id <> id);
    END IF;
END
$$;

CREATE INDEX IF NOT EXISTS learning_items_user_type_updated_at_idx
    ON learning_items (user_id, item_type, updated_at DESC);
CREATE INDEX IF NOT EXISTS learning_items_user_quality_flags_idx
    ON learning_items USING GIN (quality_flags);
CREATE INDEX IF NOT EXISTS learning_items_user_tags_idx
    ON learning_items USING GIN (tags);

CREATE TABLE IF NOT EXISTS word_pack_learning_items (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    pack_id TEXT NOT NULL,
    learning_item_id UUID NOT NULL,
    added_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, pack_id, learning_item_id),
    FOREIGN KEY (user_id, pack_id)
        REFERENCES word_packs(user_id, id)
        ON DELETE CASCADE,
    FOREIGN KEY (user_id, learning_item_id)
        REFERENCES learning_items(user_id, id)
        ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS word_pack_learning_items_item_idx
    ON word_pack_learning_items (user_id, learning_item_id);

CREATE TABLE IF NOT EXISTS learning_activity_events (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    learning_item_id UUID,
    material_id UUID,
    event_type TEXT NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    payload_sha256 TEXT NOT NULL,
    idempotency_key TEXT,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT learning_activity_events_item_fk
        FOREIGN KEY (user_id, learning_item_id)
        REFERENCES learning_items(user_id, id)
        ON DELETE SET NULL (learning_item_id),
    CONSTRAINT learning_activity_events_material_fk
        FOREIGN KEY (user_id, material_id)
        REFERENCES materials(user_id, id)
        ON DELETE SET NULL (material_id),
    CONSTRAINT learning_activity_events_type_valid CHECK (
        event_type IN (
            'read', 'create', 'accept', 'reject', 'archive', 'restore',
            'organize', 'local_preview', 'merge', 'migrate'
        )
    ),
    CONSTRAINT learning_activity_events_metadata_object
        CHECK (jsonb_typeof(metadata) = 'object'),
    CONSTRAINT learning_activity_events_payload_sha256_format
        CHECK (payload_sha256 ~ '^[0-9a-f]{64}$'),
    CONSTRAINT learning_activity_events_idempotency_key_not_empty
        CHECK (idempotency_key IS NULL OR char_length(btrim(idempotency_key)) > 0)
);

CREATE UNIQUE INDEX IF NOT EXISTS learning_activity_events_user_idempotency_idx
    ON learning_activity_events (user_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;
CREATE INDEX IF NOT EXISTS learning_activity_events_user_occurred_idx
    ON learning_activity_events (user_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS learning_activity_events_user_material_occurred_idx
    ON learning_activity_events (user_id, material_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS learning_activity_events_user_item_occurred_idx
    ON learning_activity_events (user_id, learning_item_id, occurred_at DESC);
