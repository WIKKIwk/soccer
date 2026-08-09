-- Realtime delivery sequence is durable state scoped by the stable topic name.
-- It is allocated inside the caller's transaction so rollback does not create
-- an unobserved sequence gap.
CREATE TABLE realtime_topic_sequences (
    topic TEXT PRIMARY KEY,
    last_sequence BIGINT NOT NULL DEFAULT 0,
    CONSTRAINT realtime_topic_not_blank CHECK (char_length(btrim(topic)) > 0),
    CONSTRAINT realtime_topic_sequence_nonnegative CHECK (last_sequence >= 0)
);
