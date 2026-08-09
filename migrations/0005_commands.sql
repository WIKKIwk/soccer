CREATE TABLE commands (
    id UUID PRIMARY KEY,
    client_command_id UUID NOT NULL UNIQUE,
    request_fingerprint BYTEA NOT NULL,
    status TEXT NOT NULL DEFAULT 'received',
    processing_owner UUID,
    processing_expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    started_at TIMESTAMPTZ,
    CONSTRAINT commands_request_fingerprint_sha256 CHECK (octet_length(request_fingerprint) = 32),
    CONSTRAINT commands_status_valid CHECK (
        status IN ('received', 'processing', 'accepted', 'rejected', 'failed_retryable')
    ),
    CONSTRAINT commands_processing_lease_shape CHECK (
        (
            status = 'processing'
            AND processing_owner IS NOT NULL
            AND processing_expires_at IS NOT NULL
        )
        OR (
            status <> 'processing'
            AND processing_owner IS NULL
            AND processing_expires_at IS NULL
        )
    )
);
