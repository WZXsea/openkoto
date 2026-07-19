ALTER TABLE materials
    ADD COLUMN normalized_source_url TEXT,
    ADD COLUMN content_sha256 TEXT,
    ADD COLUMN file_sha256 TEXT,
    ADD COLUMN archived_at TIMESTAMPTZ,
    ADD CONSTRAINT materials_content_sha256_format CHECK (
        content_sha256 IS NULL OR content_sha256 ~ '^[0-9a-f]{64}$'
    ),
    ADD CONSTRAINT materials_file_sha256_format CHECK (
        file_sha256 IS NULL OR file_sha256 ~ '^[0-9a-f]{64}$'
    );

CREATE INDEX materials_user_normalized_source_url_idx
    ON materials (user_id, normalized_source_url)
    WHERE normalized_source_url IS NOT NULL;
CREATE INDEX materials_user_content_sha256_idx
    ON materials (user_id, content_sha256)
    WHERE content_sha256 IS NOT NULL;
CREATE INDEX materials_user_file_sha256_idx
    ON materials (user_id, file_sha256)
    WHERE file_sha256 IS NOT NULL;
CREATE INDEX materials_user_updated_at_idx
    ON materials (user_id, updated_at DESC);
CREATE INDEX materials_user_archived_at_idx
    ON materials (user_id, archived_at, created_at DESC);

CREATE TABLE material_tags (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    color TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT material_tags_name_not_empty CHECK (char_length(btrim(name)) > 0),
    CONSTRAINT material_tags_color_not_empty CHECK (
        color IS NULL OR char_length(btrim(color)) > 0
    ),
    CONSTRAINT material_tags_user_id_id_unique UNIQUE (user_id, id)
);

CREATE UNIQUE INDEX material_tags_user_lower_name_idx
    ON material_tags (user_id, lower(name));
CREATE INDEX material_tags_user_updated_at_idx
    ON material_tags (user_id, updated_at DESC);

CREATE TABLE material_tag_links (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    material_id UUID NOT NULL,
    tag_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, material_id, tag_id),
    CONSTRAINT material_tag_links_user_material_fk
        FOREIGN KEY (user_id, material_id)
        REFERENCES materials(user_id, id)
        ON DELETE CASCADE,
    CONSTRAINT material_tag_links_user_tag_fk
        FOREIGN KEY (user_id, tag_id)
        REFERENCES material_tags(user_id, id)
        ON DELETE CASCADE
);

CREATE INDEX material_tag_links_user_tag_idx
    ON material_tag_links (user_id, tag_id, material_id);
CREATE INDEX material_tag_links_user_material_idx
    ON material_tag_links (user_id, material_id, tag_id);

CREATE TABLE reading_progress (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    material_id UUID NOT NULL,
    reader_kind TEXT NOT NULL,
    locator JSONB NOT NULL DEFAULT '{}'::jsonb,
    progress_ratio DOUBLE PRECISION NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'unread',
    last_opened_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, material_id),
    CONSTRAINT reading_progress_user_material_fk
        FOREIGN KEY (user_id, material_id)
        REFERENCES materials(user_id, id)
        ON DELETE CASCADE,
    CONSTRAINT reading_progress_reader_kind_valid CHECK (
        reader_kind IN ('article', 'pdf', 'epub', 'txt', 'media')
    ),
    CONSTRAINT reading_progress_status_valid CHECK (
        status IN ('unread', 'reading', 'completed')
    ),
    CONSTRAINT reading_progress_value_range CHECK (
        progress_ratio >= 0 AND progress_ratio <= 1
    ),
    CONSTRAINT reading_progress_locator_object CHECK (jsonb_typeof(locator) = 'object')
);

CREATE INDEX reading_progress_user_status_updated_idx
    ON reading_progress (user_id, status, updated_at DESC);
CREATE INDEX reading_progress_user_last_read_idx
    ON reading_progress (user_id, last_opened_at DESC);

CREATE TABLE material_import_jobs (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    source_kind TEXT NOT NULL,
    source_uri TEXT,
    normalized_source_url TEXT,
    file_id UUID,
    input_hash TEXT NOT NULL,
    file_sha256 TEXT,
    content_sha256 TEXT,
    status TEXT NOT NULL DEFAULT 'queued',
    progress DOUBLE PRECISION NOT NULL DEFAULT 0,
    error_code TEXT,
    error_message TEXT,
    result_material_id UUID,
    preview JSONB NOT NULL DEFAULT '{}'::jsonb,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    CONSTRAINT material_import_jobs_source_kind_valid CHECK (
        source_kind IN (
            'article', 'url', 'text_file', 'book', 'audio', 'video', 'subtitle', 'youtube'
        )
    ),
    CONSTRAINT material_import_jobs_status_valid CHECK (
        status IN (
            'queued', 'validating', 'parsing', 'preview_ready', 'committing', 'succeeded',
            'failed_retryable', 'failed_terminal', 'cancelled'
        )
    ),
    CONSTRAINT material_import_jobs_progress_range CHECK (progress >= 0 AND progress <= 1),
    CONSTRAINT material_import_jobs_input_hash_format CHECK (
        input_hash ~ '^[0-9a-f]{64}$'
    ),
    CONSTRAINT material_import_jobs_file_sha256_format CHECK (
        file_sha256 IS NULL OR file_sha256 ~ '^[0-9a-f]{64}$'
    ),
    CONSTRAINT material_import_jobs_content_sha256_format CHECK (
        content_sha256 IS NULL OR content_sha256 ~ '^[0-9a-f]{64}$'
    ),
    CONSTRAINT material_import_jobs_preview_object CHECK (jsonb_typeof(preview) = 'object'),
    CONSTRAINT material_import_jobs_metadata_object CHECK (jsonb_typeof(metadata) = 'object'),
    CONSTRAINT material_import_jobs_user_id_id_unique UNIQUE (user_id, id),
    CONSTRAINT material_import_jobs_user_file_fk
        FOREIGN KEY (user_id, file_id)
        REFERENCES files(user_id, id)
        ON DELETE SET NULL (file_id),
    CONSTRAINT material_import_jobs_user_material_fk
        FOREIGN KEY (user_id, result_material_id)
        REFERENCES materials(user_id, id)
        ON DELETE SET NULL (result_material_id)
);

CREATE INDEX material_import_jobs_user_created_idx
    ON material_import_jobs (user_id, created_at DESC);
CREATE INDEX material_import_jobs_user_status_updated_idx
    ON material_import_jobs (user_id, status, updated_at DESC);
CREATE INDEX material_import_jobs_user_normalized_url_idx
    ON material_import_jobs (user_id, normalized_source_url)
    WHERE normalized_source_url IS NOT NULL;
CREATE INDEX material_import_jobs_user_content_sha256_idx
    ON material_import_jobs (user_id, content_sha256)
    WHERE content_sha256 IS NOT NULL;
CREATE INDEX material_import_jobs_user_file_sha256_idx
    ON material_import_jobs (user_id, file_sha256)
    WHERE file_sha256 IS NOT NULL;
CREATE INDEX material_import_jobs_user_input_hash_idx
    ON material_import_jobs (user_id, input_hash)
    WHERE input_hash IS NOT NULL;
