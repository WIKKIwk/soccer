-- Retry scheduling keeps a transient publish failure claimable only after its
-- deterministic backoff. Poison-message classification remains a later
-- forward migration and is not inferred from this timestamp.
ALTER TABLE outbox_messages
    ADD COLUMN next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP;

CREATE INDEX outbox_retry_due_order_idx
    ON outbox_messages (delivery_status, next_attempt_at, created_at, message_id);
