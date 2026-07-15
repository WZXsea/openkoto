ALTER TABLE agent_tasks
    ADD COLUMN IF NOT EXISTS input_snapshot JSONB,
    ADD COLUMN IF NOT EXISTS retry_of_task_id TEXT,
    ADD COLUMN IF NOT EXISTS root_task_id TEXT,
    ADD COLUMN IF NOT EXISTS attempt INTEGER NOT NULL DEFAULT 1,
    ADD COLUMN IF NOT EXISTS output_version INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS legacy_status TEXT;

UPDATE agent_tasks
SET input_snapshot = COALESCE(input_snapshot, input),
    root_task_id = COALESCE(root_task_id, id),
    legacy_status = CASE WHEN status = 'interrupted' THEN 'interrupted' ELSE legacy_status END,
    status = CASE WHEN status = 'interrupted' THEN 'failed' ELSE status END,
    error = CASE
        WHEN status = 'interrupted' AND (error IS NULL OR btrim(error) = '')
        THEN 'Legacy interrupted task migrated to failed; retry is available.'
        ELSE error
    END;

ALTER TABLE agent_tasks
    ALTER COLUMN input_snapshot SET DEFAULT '{}'::jsonb,
    ALTER COLUMN input_snapshot SET NOT NULL,
    ALTER COLUMN root_task_id SET NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'agent_tasks_status_valid'
          AND conrelid = 'agent_tasks'::regclass
    ) THEN
        ALTER TABLE agent_tasks
            ADD CONSTRAINT agent_tasks_status_valid
            CHECK (status IN ('queued', 'running', 'succeeded', 'failed', 'cancelled'));
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'agent_tasks_input_snapshot_object'
          AND conrelid = 'agent_tasks'::regclass
    ) THEN
        ALTER TABLE agent_tasks
            ADD CONSTRAINT agent_tasks_input_snapshot_object
            CHECK (jsonb_typeof(input_snapshot) = 'object');
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'agent_tasks_attempt_positive'
          AND conrelid = 'agent_tasks'::regclass
    ) THEN
        ALTER TABLE agent_tasks
            ADD CONSTRAINT agent_tasks_attempt_positive CHECK (attempt > 0);
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'agent_tasks_output_version_non_negative'
          AND conrelid = 'agent_tasks'::regclass
    ) THEN
        ALTER TABLE agent_tasks
            ADD CONSTRAINT agent_tasks_output_version_non_negative CHECK (output_version >= 0);
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'agent_tasks_retry_of_fk'
          AND conrelid = 'agent_tasks'::regclass
    ) THEN
        ALTER TABLE agent_tasks
            ADD CONSTRAINT agent_tasks_retry_of_fk
            FOREIGN KEY (user_id, retry_of_task_id)
            REFERENCES agent_tasks(user_id, id)
            ON DELETE SET NULL (retry_of_task_id);
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'agent_tasks_root_fk'
          AND conrelid = 'agent_tasks'::regclass
    ) THEN
        ALTER TABLE agent_tasks
            ADD CONSTRAINT agent_tasks_root_fk
            FOREIGN KEY (user_id, root_task_id)
            REFERENCES agent_tasks(user_id, id)
            ON DELETE RESTRICT;
    END IF;
END
$$;

CREATE INDEX IF NOT EXISTS agent_tasks_user_status_updated_idx
    ON agent_tasks (user_id, status, updated_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS agent_tasks_user_root_attempt_idx
    ON agent_tasks (user_id, root_task_id, attempt);

CREATE TABLE IF NOT EXISTS agent_task_events (
    id UUID PRIMARY KEY,
    sequence BIGSERIAL UNIQUE,
    external_event_id TEXT,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    task_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    from_status TEXT,
    to_status TEXT,
    stage TEXT,
    message TEXT,
    error TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    idempotency_key TEXT,
    payload_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    FOREIGN KEY (user_id, task_id)
        REFERENCES agent_tasks(user_id, id)
        ON DELETE CASCADE,
    CONSTRAINT agent_task_events_type_not_empty CHECK (char_length(btrim(event_type)) > 0),
    CONSTRAINT agent_task_events_statuses_valid CHECK (
        (from_status IS NULL OR from_status IN ('queued', 'running', 'succeeded', 'failed', 'cancelled'))
        AND (to_status IS NULL OR to_status IN ('queued', 'running', 'succeeded', 'failed', 'cancelled'))
    ),
    CONSTRAINT agent_task_events_metadata_object CHECK (jsonb_typeof(metadata) = 'object'),
    CONSTRAINT agent_task_events_idempotency_not_empty CHECK (
        idempotency_key IS NULL OR char_length(btrim(idempotency_key)) > 0
    ),
    CONSTRAINT agent_task_events_payload_hash_format CHECK (payload_hash ~ '^[0-9a-f]{64}$')
);

CREATE UNIQUE INDEX IF NOT EXISTS agent_task_events_user_idempotency_idx
    ON agent_task_events (user_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS agent_task_events_user_task_external_event_idx
    ON agent_task_events (user_id, task_id, external_event_id)
    WHERE external_event_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS agent_task_events_user_task_timeline_idx
    ON agent_task_events (user_id, task_id, created_at, sequence);

INSERT INTO agent_task_events (
    id, user_id, task_id, event_type, from_status, to_status, stage, message, error,
    metadata, idempotency_key, payload_hash, created_at
)
SELECT md5(at.user_id::text || ':agent-observability:' || at.id)::uuid,
       at.user_id,
       at.id,
       CASE WHEN at.legacy_status = 'interrupted'
            THEN 'legacy_interrupted_migrated' ELSE 'created' END,
       CASE WHEN at.legacy_status = 'interrupted' THEN NULL ELSE NULL END,
       at.status,
       at.stage,
       CASE WHEN at.legacy_status = 'interrupted'
            THEN 'Legacy interrupted status migrated to failed.' ELSE at.message END,
       at.error,
       jsonb_build_object(
           'attempt', at.attempt,
           'root_task_id', at.root_task_id,
           'legacy_status', at.legacy_status
       ),
       'migration:agent-observability:' || at.id,
       md5(at.user_id::text || ':' || at.id || ':' || at.status) ||
           md5(at.status || ':' || at.id || ':' || at.user_id::text),
       CASE
           WHEN at.updated_at ~ '^\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}'
           THEN at.updated_at::timestamptz
           ELSE NOW()
       END
FROM agent_tasks at
WHERE NOT EXISTS (
    SELECT 1
    FROM agent_task_events existing_event
    WHERE existing_event.user_id = at.user_id
      AND existing_event.task_id = at.id
)
ON CONFLICT DO NOTHING;

CREATE TABLE IF NOT EXISTS assistant_action_audits (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    task_id TEXT NOT NULL,
    event_id UUID NOT NULL UNIQUE REFERENCES agent_task_events(id) ON DELETE CASCADE,
    action_kind TEXT NOT NULL,
    status TEXT NOT NULL,
    code TEXT,
    message TEXT,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    registry_scope TEXT NOT NULL,
    external_write BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    FOREIGN KEY (user_id, task_id)
        REFERENCES agent_tasks(user_id, id)
        ON DELETE CASCADE,
    CONSTRAINT assistant_action_audits_kind_not_empty CHECK (char_length(btrim(action_kind)) > 0),
    CONSTRAINT assistant_action_audits_status_valid CHECK (status IN ('executed', 'rejected')),
    CONSTRAINT assistant_action_audits_payload_object CHECK (jsonb_typeof(payload) = 'object'),
    CONSTRAINT assistant_action_audits_scope_valid CHECK (
        registry_scope IN ('local_read', 'local_write', 'external_write', 'unregistered')
    ),
    CONSTRAINT assistant_action_audits_external_write_rejected CHECK (
        NOT external_write OR status = 'rejected'
    )
);

CREATE INDEX IF NOT EXISTS assistant_action_audits_user_task_created_idx
    ON assistant_action_audits (user_id, task_id, created_at, id);
