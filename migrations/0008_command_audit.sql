-- The first command audit slice keeps one business audit row per logical
-- command. The command ID is already UUIDv7-backed and is the safe
-- correlation key; request payloads and fingerprints are intentionally not
-- copied into audit rows.
CREATE TABLE command_audit (
    command_id UUID PRIMARY KEY REFERENCES commands(id),
    actor_id UUID NOT NULL REFERENCES admin_users(id),
    aggregate_id UUID NOT NULL,
    action TEXT NOT NULL,
    outcome TEXT NOT NULL,
    aggregate_version BIGINT NOT NULL,
    event_count BIGINT NOT NULL,
    event_id UUID REFERENCES match_events(event_id),
    rejection_code TEXT,
    details JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT command_audit_action_not_blank CHECK (char_length(btrim(action)) > 0),
    CONSTRAINT command_audit_outcome_valid CHECK (outcome IN ('accepted', 'rejected')),
    CONSTRAINT command_audit_aggregate_version_nonnegative CHECK (aggregate_version >= 0),
    CONSTRAINT command_audit_event_count_nonnegative CHECK (event_count >= 0),
    CONSTRAINT command_audit_details_object CHECK (jsonb_typeof(details) = 'object'),
    CONSTRAINT command_audit_outcome_shape CHECK (
        (
            outcome = 'accepted'
            AND rejection_code IS NULL
        )
        OR (
            outcome = 'rejected'
            AND event_id IS NULL
            AND event_count = 0
            AND rejection_code IS NOT NULL
            AND char_length(btrim(rejection_code)) > 0
        )
    )
);

CREATE INDEX command_audit_aggregate_created_at_idx
    ON command_audit (aggregate_id, created_at, command_id);
