CREATE TABLE word_packs (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    cover_url TEXT,
    author TEXT,
    language_from TEXT,
    language_to TEXT,
    tags JSONB NOT NULL DEFAULT '[]'::jsonb,
    version TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    is_system BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (user_id, id),
    CONSTRAINT word_packs_id_not_empty CHECK (char_length(btrim(id)) > 0),
    CONSTRAINT word_packs_name_not_empty CHECK (char_length(btrim(name)) > 0),
    CONSTRAINT word_packs_tags_array CHECK (jsonb_typeof(tags) = 'array')
);

CREATE INDEX word_packs_user_updated_at_idx ON word_packs (user_id, updated_at DESC);

CREATE TABLE favorite_vocabularies (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    id TEXT NOT NULL,
    word TEXT NOT NULL,
    meaning TEXT NOT NULL,
    usage TEXT NOT NULL DEFAULT '',
    explanation TEXT,
    example TEXT,
    reading TEXT,
    source_article_id TEXT,
    source_article_title TEXT,
    srs_state TEXT NOT NULL DEFAULT 'new',
    ease_factor DOUBLE PRECISION NOT NULL DEFAULT 2.5,
    repetitions INTEGER NOT NULL DEFAULT 0,
    interval_days INTEGER NOT NULL DEFAULT 0,
    due_date TEXT NOT NULL,
    last_reviewed_at TEXT,
    review_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    PRIMARY KEY (user_id, id),
    CONSTRAINT favorite_vocabularies_id_not_empty CHECK (char_length(btrim(id)) > 0),
    CONSTRAINT favorite_vocabularies_word_not_empty CHECK (char_length(btrim(word)) > 0),
    CONSTRAINT favorite_vocabularies_meaning_not_empty CHECK (char_length(btrim(meaning)) > 0),
    CONSTRAINT favorite_vocabularies_srs_state_valid CHECK (srs_state IN ('new', 'learning', 'review')),
    CONSTRAINT favorite_vocabularies_ease_factor_valid CHECK (ease_factor >= 1.3),
    CONSTRAINT favorite_vocabularies_repetitions_non_negative CHECK (repetitions >= 0),
    CONSTRAINT favorite_vocabularies_interval_non_negative CHECK (interval_days >= 0),
    CONSTRAINT favorite_vocabularies_review_count_non_negative CHECK (review_count >= 0)
);

CREATE INDEX favorite_vocabularies_user_created_at_idx ON favorite_vocabularies (user_id, created_at DESC);
CREATE INDEX favorite_vocabularies_user_due_date_idx ON favorite_vocabularies (user_id, due_date);

CREATE TABLE favorite_vocabulary_packs (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    vocabulary_id TEXT NOT NULL,
    pack_id TEXT NOT NULL,
    PRIMARY KEY (user_id, vocabulary_id, pack_id),
    FOREIGN KEY (user_id, vocabulary_id)
        REFERENCES favorite_vocabularies(user_id, id)
        ON DELETE CASCADE,
    FOREIGN KEY (user_id, pack_id)
        REFERENCES word_packs(user_id, id)
        ON DELETE CASCADE
);

CREATE INDEX favorite_vocabulary_packs_pack_idx ON favorite_vocabulary_packs (user_id, pack_id);

CREATE TABLE favorite_grammars (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    id TEXT NOT NULL,
    point TEXT NOT NULL,
    explanation TEXT NOT NULL,
    example TEXT,
    source_article_id TEXT,
    source_article_title TEXT,
    created_at TEXT NOT NULL,
    PRIMARY KEY (user_id, id),
    CONSTRAINT favorite_grammars_id_not_empty CHECK (char_length(btrim(id)) > 0),
    CONSTRAINT favorite_grammars_point_not_empty CHECK (char_length(btrim(point)) > 0),
    CONSTRAINT favorite_grammars_explanation_not_empty CHECK (char_length(btrim(explanation)) > 0)
);

CREATE INDEX favorite_grammars_user_created_at_idx ON favorite_grammars (user_id, created_at DESC);

CREATE TABLE bookmarks (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    id TEXT NOT NULL,
    book_path TEXT NOT NULL,
    book_type TEXT NOT NULL,
    title TEXT NOT NULL,
    note TEXT,
    selected_text TEXT,
    page_number INTEGER,
    epub_cfi TEXT,
    created_at TEXT NOT NULL,
    color TEXT,
    PRIMARY KEY (user_id, id),
    CONSTRAINT bookmarks_id_not_empty CHECK (char_length(btrim(id)) > 0),
    CONSTRAINT bookmarks_book_path_not_empty CHECK (char_length(btrim(book_path)) > 0),
    CONSTRAINT bookmarks_book_type_valid CHECK (book_type IN ('epub', 'txt', 'pdf')),
    CONSTRAINT bookmarks_title_not_empty CHECK (char_length(btrim(title)) > 0),
    CONSTRAINT bookmarks_page_number_positive CHECK (page_number IS NULL OR page_number > 0)
);

CREATE INDEX bookmarks_user_created_at_idx ON bookmarks (user_id, created_at DESC);
CREATE INDEX bookmarks_user_book_path_idx ON bookmarks (user_id, book_path);

CREATE TABLE agent_tasks (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    id TEXT NOT NULL,
    task_type TEXT NOT NULL,
    status TEXT NOT NULL,
    article_id TEXT NOT NULL,
    input JSONB NOT NULL,
    progress DOUBLE PRECISION NOT NULL DEFAULT 0,
    stage TEXT,
    message TEXT,
    error TEXT,
    worker_session_id TEXT,
    artifact_ids JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    started_at TEXT,
    finished_at TEXT,
    PRIMARY KEY (user_id, id),
    CONSTRAINT agent_tasks_id_not_empty CHECK (char_length(btrim(id)) > 0),
    CONSTRAINT agent_tasks_article_id_not_empty CHECK (char_length(btrim(article_id)) > 0),
    CONSTRAINT agent_tasks_input_object CHECK (jsonb_typeof(input) = 'object'),
    CONSTRAINT agent_tasks_artifact_ids_array CHECK (jsonb_typeof(artifact_ids) = 'array'),
    CONSTRAINT agent_tasks_progress_range CHECK (progress >= 0 AND progress <= 1)
);

CREATE INDEX agent_tasks_user_updated_at_idx ON agent_tasks (user_id, updated_at DESC);
CREATE INDEX agent_tasks_user_article_idx ON agent_tasks (user_id, article_id);

CREATE TABLE artifacts (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    id TEXT NOT NULL,
    task_id TEXT NOT NULL,
    article_id TEXT NOT NULL,
    artifact_type TEXT NOT NULL,
    version TEXT NOT NULL,
    content JSONB NOT NULL,
    metadata JSONB,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (user_id, id),
    FOREIGN KEY (user_id, task_id)
        REFERENCES agent_tasks(user_id, id)
        ON DELETE CASCADE,
    CONSTRAINT artifacts_id_not_empty CHECK (char_length(btrim(id)) > 0),
    CONSTRAINT artifacts_task_id_not_empty CHECK (char_length(btrim(task_id)) > 0),
    CONSTRAINT artifacts_article_id_not_empty CHECK (char_length(btrim(article_id)) > 0),
    CONSTRAINT artifacts_type_not_empty CHECK (char_length(btrim(artifact_type)) > 0),
    CONSTRAINT artifacts_version_not_empty CHECK (char_length(btrim(version)) > 0)
);

CREATE INDEX artifacts_user_article_idx ON artifacts (user_id, article_id);
CREATE INDEX artifacts_user_task_idx ON artifacts (user_id, task_id);
