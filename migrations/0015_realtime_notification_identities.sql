-- One committed identity belongs to one outbox message for its entire delivery
-- lifecycle. Retries reuse the same topic and sequence instead of allocating a
-- second realtime position for the same notification.
CREATE TABLE realtime_notification_identities (
    message_id UUID PRIMARY KEY REFERENCES outbox_messages(message_id),
    topic TEXT NOT NULL,
    sequence BIGINT NOT NULL,
    assigned_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT realtime_notification_identity_topic_not_blank
        CHECK (char_length(btrim(topic)) > 0),
    CONSTRAINT realtime_notification_identity_sequence_positive
        CHECK (sequence > 0),
    CONSTRAINT realtime_notification_topic_sequence_unique
        UNIQUE (topic, sequence)
);

CREATE INDEX realtime_notification_identity_topic_order_idx
    ON realtime_notification_identities (topic, sequence, message_id);
