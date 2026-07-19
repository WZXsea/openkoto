ALTER TABLE material_blocks
    DROP CONSTRAINT IF EXISTS material_blocks_type_valid;
ALTER TABLE material_blocks
    ADD CONSTRAINT material_blocks_type_valid CHECK (
        block_type IN ('paragraph', 'heading', 'list_item', 'quote', 'divider')
    );

CREATE TABLE IF NOT EXISTS material_segment_lineage (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    material_id UUID NOT NULL,
    revision BIGINT NOT NULL,
    from_segment_id UUID,
    to_segment_id UUID,
    operation TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT material_segment_lineage_user_material_fk
        FOREIGN KEY (user_id, material_id)
        REFERENCES materials(user_id, id)
        ON DELETE CASCADE,
    CONSTRAINT material_segment_lineage_from_fk
        FOREIGN KEY (user_id, material_id, from_segment_id)
        REFERENCES material_segments(user_id, material_id, id),
    CONSTRAINT material_segment_lineage_to_fk
        FOREIGN KEY (user_id, material_id, to_segment_id)
        REFERENCES material_segments(user_id, material_id, id),
    CONSTRAINT material_segment_lineage_revision_positive CHECK (revision >= 1),
    CONSTRAINT material_segment_lineage_operation_valid CHECK (
        operation IN ('preserved', 'modified', 'inserted', 'deleted', 'split', 'merged')
    ),
    CONSTRAINT material_segment_lineage_endpoint_present CHECK (
        from_segment_id IS NOT NULL OR to_segment_id IS NOT NULL
    )
);

CREATE INDEX IF NOT EXISTS material_segment_lineage_user_revision_idx
    ON material_segment_lineage (user_id, material_id, revision);
CREATE INDEX IF NOT EXISTS material_segment_lineage_from_idx
    ON material_segment_lineage (user_id, material_id, from_segment_id)
    WHERE from_segment_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS material_segment_lineage_to_idx
    ON material_segment_lineage (user_id, material_id, to_segment_id)
    WHERE to_segment_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS material_relations (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    source_material_id UUID NOT NULL,
    target_material_id UUID NOT NULL,
    relation_type TEXT NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT material_relations_source_fk
        FOREIGN KEY (user_id, source_material_id)
        REFERENCES materials(user_id, id)
        ON DELETE CASCADE,
    CONSTRAINT material_relations_target_fk
        FOREIGN KEY (user_id, target_material_id)
        REFERENCES materials(user_id, id)
        ON DELETE CASCADE,
    CONSTRAINT material_relations_type_valid CHECK (
        relation_type IN ('editable_derivative')
    ),
    CONSTRAINT material_relations_metadata_object CHECK (jsonb_typeof(metadata) = 'object'),
    CONSTRAINT material_relations_not_self CHECK (source_material_id <> target_material_id),
    CONSTRAINT material_relations_unique UNIQUE (
        user_id, source_material_id, target_material_id, relation_type
    )
);

CREATE INDEX IF NOT EXISTS material_relations_user_source_idx
    ON material_relations (user_id, source_material_id, relation_type);
CREATE INDEX IF NOT EXISTS material_relations_user_target_idx
    ON material_relations (user_id, target_material_id, relation_type);
