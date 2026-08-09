-- The first outbox slice stores one committed match-event message per source
-- event. Delivery state, claiming, retries, and poison handling are later
-- forward migrations owned by the outbox worker increment.
CREATE TABLE outbox_messages (
    message_id UUID PRIMARY KEY,
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL,
    causation_command_id UUID NOT NULL REFERENCES commands(id),
    source_event_id UUID NOT NULL REFERENCES match_events(event_id),
    message_type TEXT NOT NULL,
    schema_version INTEGER NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT outbox_aggregate_type_not_blank CHECK (char_length(btrim(aggregate_type)) > 0),
    CONSTRAINT outbox_source_version_positive CHECK (source_aggregate_version > 0),
    CONSTRAINT outbox_message_type_not_blank CHECK (char_length(btrim(message_type)) > 0),
    CONSTRAINT outbox_schema_version_positive CHECK (schema_version > 0),
    CONSTRAINT outbox_event_message_identity UNIQUE (source_event_id, message_type)
);

CREATE INDEX outbox_aggregate_order_idx
    ON outbox_messages (aggregate_id, source_aggregate_version, message_id);
