ALTER TABLE materials
    DROP CONSTRAINT materials_source_type_valid;

ALTER TABLE materials
    ADD CONSTRAINT materials_source_type_valid CHECK (
        source_type IS NULL OR source_type IN (
            'article', 'web', 'text_file', 'youtube', 'local_video', 'audio', 'book'
        )
    );
