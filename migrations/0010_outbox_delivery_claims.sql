-- Outbox delivery claims are lease-backed storage state. A later worker
-- increment may add publish timestamps, retry scheduling, and poison details;
-- this migration only makes one message claimable by one worker at a time.
ALTER TABLE outbox_messages
    ADD COLUMN delivery_status TEXT NOT NULL DEFAULT 'pending',
    ADD COLUMN attempt_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN claim_owner UUID,
    ADD COLUMN claim_expires_at TIMESTAMPTZ;

ALTER TABLE outbox_messages
    ADD CONSTRAINT outbox_delivery_status_valid CHECK (
        delivery_status IN ('pending', 'processing', 'published')
    ),
    ADD CONSTRAINT outbox_attempt_count_nonnegative CHECK (attempt_count >= 0),
    ADD CONSTRAINT outbox_claim_shape CHECK (
        (
            delivery_status = 'processing'
            AND claim_owner IS NOT NULL
            AND claim_expires_at IS NOT NULL
        )
        OR (
            delivery_status IN ('pending', 'published')
            AND claim_owner IS NULL
            AND claim_expires_at IS NULL
        )
    );

CREATE INDEX outbox_delivery_claim_order_idx
    ON outbox_messages (delivery_status, claim_expires_at, created_at, message_id);
