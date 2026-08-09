-- A message that exhausts the delivery-attempt threshold becomes terminally
-- poisoned. It is never selected by the worker claim query; publish recovery
-- and operator replay are later contracts.
ALTER TABLE outbox_messages
    DROP CONSTRAINT outbox_delivery_status_valid,
    DROP CONSTRAINT outbox_claim_shape;

ALTER TABLE outbox_messages
    ADD COLUMN poisoned_at TIMESTAMPTZ,
    ADD COLUMN poison_reason TEXT;

ALTER TABLE outbox_messages
    ADD CONSTRAINT outbox_delivery_status_valid CHECK (
        delivery_status IN ('pending', 'processing', 'published', 'poisoned')
    ),
    ADD CONSTRAINT outbox_claim_shape CHECK (
        (
            delivery_status = 'processing'
            AND claim_owner IS NOT NULL
            AND claim_expires_at IS NOT NULL
        )
        OR (
            delivery_status IN ('pending', 'published', 'poisoned')
            AND claim_owner IS NULL
            AND claim_expires_at IS NULL
        )
    ),
    ADD CONSTRAINT outbox_poison_shape CHECK (
        (
            delivery_status = 'poisoned'
            AND poisoned_at IS NOT NULL
            AND poison_reason IS NOT NULL
            AND char_length(btrim(poison_reason)) > 0
        )
        OR (
            delivery_status <> 'poisoned'
            AND poisoned_at IS NULL
            AND poison_reason IS NULL
        )
    );

CREATE INDEX outbox_poisoned_order_idx
    ON outbox_messages (delivery_status, poisoned_at, message_id);
