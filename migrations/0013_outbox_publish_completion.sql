-- A published message records the acknowledgement time. The worker claim
-- remains the only path to completion; retry and poison transitions clear this
-- field before returning to another delivery state.
ALTER TABLE outbox_messages
    ADD COLUMN published_at TIMESTAMPTZ;

ALTER TABLE outbox_messages
    ADD CONSTRAINT outbox_published_shape CHECK (
        (
            delivery_status = 'published'
            AND published_at IS NOT NULL
        )
        OR (
            delivery_status <> 'published'
            AND published_at IS NULL
        )
    );

CREATE INDEX outbox_published_order_idx
    ON outbox_messages (delivery_status, published_at, message_id);
