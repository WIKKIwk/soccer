ALTER TABLE commands
    ADD COLUMN aggregate_id UUID,
    ADD COLUMN completed_at TIMESTAMPTZ,
    ADD COLUMN final_aggregate_version BIGINT,
    ADD COLUMN event_count BIGINT,
    ADD COLUMN result_payload JSONB,
    ADD COLUMN rejection_code TEXT,
    ADD COLUMN rejection_details JSONB;

ALTER TABLE commands
    ADD CONSTRAINT commands_terminal_result_shape CHECK (
        (
            status = 'accepted'
            AND aggregate_id IS NOT NULL
            AND completed_at IS NOT NULL
            AND final_aggregate_version IS NOT NULL
            AND final_aggregate_version >= 0
            AND event_count IS NOT NULL
            AND event_count >= 0
            AND result_payload IS NOT NULL
            AND rejection_code IS NULL
            AND rejection_details IS NULL
        )
        OR (
            status = 'rejected'
            AND aggregate_id IS NOT NULL
            AND completed_at IS NOT NULL
            AND final_aggregate_version IS NOT NULL
            AND final_aggregate_version >= 0
            AND event_count IS NULL
            AND result_payload IS NULL
            AND rejection_code IS NOT NULL
            AND char_length(btrim(rejection_code)) > 0
            AND rejection_details IS NOT NULL
            AND jsonb_typeof(rejection_details) = 'object'
        )
        OR (
            status IN ('received', 'processing', 'failed_retryable')
            AND aggregate_id IS NULL
            AND completed_at IS NULL
            AND final_aggregate_version IS NULL
            AND event_count IS NULL
            AND result_payload IS NULL
            AND rejection_code IS NULL
            AND rejection_details IS NULL
        )
    );
