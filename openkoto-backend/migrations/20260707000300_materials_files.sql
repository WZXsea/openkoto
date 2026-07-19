CREATE TABLE files (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    original_name TEXT NOT NULL,
    storage_path TEXT NOT NULL,
    content_type TEXT,
    byte_size BIGINT NOT NULL,
    sha256 TEXT NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT files_original_name_not_empty CHECK (char_length(btrim(original_name)) > 0),
    CONSTRAINT files_storage_path_not_empty CHECK (char_length(storage_path) > 0),
    CONSTRAINT files_byte_size_non_negative CHECK (byte_size >= 0),
    CONSTRAINT files_sha256_format CHECK (char_length(sha256) = 64)
);

CREATE UNIQUE INDEX files_storage_path_idx ON files (storage_path);
CREATE UNIQUE INDEX files_user_id_id_idx ON files (user_id, id);
CREATE INDEX files_user_created_at_idx ON files (user_id, created_at DESC);

CREATE TABLE materials (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    content TEXT NOT NULL DEFAULT '',
    source_type TEXT,
    source_url TEXT,
    media_path TEXT,
    book_path TEXT,
    book_type TEXT,
    translated BOOLEAN NOT NULL DEFAULT FALSE,
    active_mind_map_artifact_id TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT materials_title_not_empty CHECK (char_length(btrim(title)) > 0),
    CONSTRAINT materials_source_type_valid CHECK (
        source_type IS NULL OR source_type IN ('article', 'web', 'youtube', 'local_video', 'audio', 'book')
    ),
    CONSTRAINT materials_book_type_valid CHECK (
        book_type IS NULL OR book_type IN ('epub', 'txt', 'pdf')
    )
);

CREATE UNIQUE INDEX materials_user_id_id_idx ON materials (user_id, id);
CREATE INDEX materials_user_created_at_idx ON materials (user_id, created_at DESC);
CREATE INDEX materials_user_source_type_idx ON materials (user_id, source_type);

CREATE TABLE material_segments (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    material_id UUID NOT NULL REFERENCES materials(id) ON DELETE CASCADE,
    segment_order INTEGER NOT NULL,
    text TEXT NOT NULL,
    reading_text TEXT,
    translation TEXT,
    explanation JSONB,
    start_time DOUBLE PRECISION,
    end_time DOUBLE PRECISION,
    is_new_paragraph BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT material_segments_text_not_empty CHECK (char_length(btrim(text)) > 0),
    CONSTRAINT material_segments_order_non_negative CHECK (segment_order >= 0),
    CONSTRAINT material_segments_time_range_valid CHECK (
        start_time IS NULL OR end_time IS NULL OR end_time >= start_time
    )
);

ALTER TABLE material_segments
    ADD CONSTRAINT material_segments_user_material_fk
    FOREIGN KEY (user_id, material_id)
    REFERENCES materials(user_id, id)
    ON DELETE CASCADE;

CREATE UNIQUE INDEX material_segments_material_order_idx ON material_segments (material_id, segment_order);
CREATE INDEX material_segments_user_material_idx ON material_segments (user_id, material_id);
