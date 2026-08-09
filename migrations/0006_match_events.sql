-- Match aggregate persistence is introduced by later MCH increments. Keep the
-- canonical MatchId as a typed UUID until that aggregate table exists.
CREATE TABLE match_events (
    event_id UUID PRIMARY KEY,
    match_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL,
    causation_command_id UUID NOT NULL REFERENCES commands(id),
    event_index BIGINT NOT NULL,
    event_type TEXT NOT NULL,
    event_schema_version INTEGER NOT NULL DEFAULT 1,
    period TEXT NOT NULL,
    clock_ms BIGINT NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL,
    actor_id UUID NOT NULL REFERENCES admin_users(id),
    verification_status TEXT NOT NULL,
    payload JSONB NOT NULL,
    supersedes_event_id UUID REFERENCES match_events(event_id),
    voids_event_id UUID REFERENCES match_events(event_id),
    CONSTRAINT match_events_aggregate_version_positive CHECK (aggregate_version > 0),
    CONSTRAINT match_events_event_index_nonnegative CHECK (event_index >= 0),
    CONSTRAINT match_events_schema_version_positive CHECK (event_schema_version > 0),
    CONSTRAINT match_events_clock_nonnegative CHECK (clock_ms >= 0),
    CONSTRAINT match_events_event_type_not_blank CHECK (char_length(btrim(event_type)) > 0),
    CONSTRAINT match_events_period_valid CHECK (
        period IN ('pre_match', 'first_half', 'half_time', 'second_half')
    ),
    CONSTRAINT match_events_verification_status_valid CHECK (
        verification_status IN ('provisional', 'verified')
    ),
    CONSTRAINT match_events_aggregate_order_unique UNIQUE (match_id, aggregate_version),
    CONSTRAINT match_events_command_index_unique UNIQUE (causation_command_id, event_index)
);
