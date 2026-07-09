CREATE TABLE learning_items (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    material_id UUID,
    segment_id UUID REFERENCES material_segments(id) ON DELETE SET NULL,
    item_type TEXT NOT NULL,
    text TEXT NOT NULL,
    normalized_text TEXT NOT NULL,
    source_sentence TEXT NOT NULL DEFAULT '',
    context_before TEXT,
    context_after TEXT,
    meaning_in_context TEXT,
    definition_en TEXT,
    definition_zh TEXT,
    collocations JSONB NOT NULL DEFAULT '[]'::jsonb,
    examples JSONB NOT NULL DEFAULT '[]'::jsonb,
    tags JSONB NOT NULL DEFAULT '[]'::jsonb,
    status TEXT NOT NULL DEFAULT 'candidate',
    priority INTEGER NOT NULL DEFAULT 0,
    difficulty INTEGER,
    ai_explanation JSONB,
    review_state JSONB NOT NULL DEFAULT '{}'::jsonb,
    dedupe_key TEXT NOT NULL,
    accepted_at TIMESTAMPTZ,
    rejected_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT learning_items_user_material_fk
        FOREIGN KEY (user_id, material_id)
        REFERENCES materials(user_id, id)
        ON DELETE CASCADE,
    CONSTRAINT learning_items_text_not_empty CHECK (char_length(btrim(text)) > 0),
    CONSTRAINT learning_items_item_type_not_empty CHECK (char_length(btrim(item_type)) > 0),
    CONSTRAINT learning_items_normalized_text_not_empty CHECK (char_length(btrim(normalized_text)) > 0),
    CONSTRAINT learning_items_dedupe_key_format CHECK (char_length(dedupe_key) = 64),
    CONSTRAINT learning_items_type_valid CHECK (item_type IN ('word', 'phrase', 'sentence', 'grammar')),
    CONSTRAINT learning_items_status_valid CHECK (status IN ('candidate', 'accepted', 'rejected', 'archived')),
    CONSTRAINT learning_items_priority_range CHECK (priority >= 0 AND priority <= 100),
    CONSTRAINT learning_items_difficulty_range CHECK (
        difficulty IS NULL OR (difficulty >= 1 AND difficulty <= 5)
    ),
    CONSTRAINT learning_items_collocations_array CHECK (jsonb_typeof(collocations) = 'array'),
    CONSTRAINT learning_items_examples_array CHECK (jsonb_typeof(examples) = 'array'),
    CONSTRAINT learning_items_tags_array CHECK (jsonb_typeof(tags) = 'array'),
    CONSTRAINT learning_items_ai_explanation_object CHECK (
        ai_explanation IS NULL OR jsonb_typeof(ai_explanation) = 'object'
    ),
    CONSTRAINT learning_items_review_state_object CHECK (jsonb_typeof(review_state) = 'object')
);

CREATE INDEX learning_items_user_updated_at_idx ON learning_items (user_id, updated_at DESC);
CREATE INDEX learning_items_user_status_updated_at_idx ON learning_items (user_id, status, updated_at DESC);
CREATE INDEX learning_items_user_material_idx ON learning_items (user_id, material_id);
CREATE INDEX learning_items_user_segment_idx ON learning_items (user_id, segment_id);
CREATE UNIQUE INDEX learning_items_user_dedupe_key_idx ON learning_items (user_id, dedupe_key);
