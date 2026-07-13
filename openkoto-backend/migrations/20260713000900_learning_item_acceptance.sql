CREATE UNIQUE INDEX IF NOT EXISTS learning_items_user_id_id_idx
    ON learning_items (user_id, id);

ALTER TABLE favorite_vocabularies
    ADD COLUMN IF NOT EXISTS learning_item_id UUID;

ALTER TABLE favorite_grammars
    ADD COLUMN IF NOT EXISTS learning_item_id UUID;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'favorite_vocabularies_learning_item_fk'
          AND conrelid = 'favorite_vocabularies'::regclass
    ) THEN
        ALTER TABLE favorite_vocabularies
            ADD CONSTRAINT favorite_vocabularies_learning_item_fk
            FOREIGN KEY (user_id, learning_item_id)
            REFERENCES learning_items(user_id, id)
            ON DELETE CASCADE;
    END IF;
END
$$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'favorite_grammars_learning_item_fk'
          AND conrelid = 'favorite_grammars'::regclass
    ) THEN
        ALTER TABLE favorite_grammars
            ADD CONSTRAINT favorite_grammars_learning_item_fk
            FOREIGN KEY (user_id, learning_item_id)
            REFERENCES learning_items(user_id, id)
            ON DELETE CASCADE;
    END IF;
END
$$;

CREATE UNIQUE INDEX IF NOT EXISTS favorite_vocabularies_user_learning_item_idx
    ON favorite_vocabularies (user_id, learning_item_id);

CREATE UNIQUE INDEX IF NOT EXISTS favorite_grammars_user_learning_item_idx
    ON favorite_grammars (user_id, learning_item_id);
