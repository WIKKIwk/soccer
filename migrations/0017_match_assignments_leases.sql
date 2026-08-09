CREATE TABLE match_assignments (
    match_id UUID PRIMARY KEY,
    fixture_id UUID NOT NULL UNIQUE REFERENCES fixtures(id),
    operator_id UUID NOT NULL REFERENCES admin_users(id),
    assigned_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE match_operator_leases (
    lease_id UUID PRIMARY KEY,
    match_id UUID NOT NULL UNIQUE REFERENCES match_assignments(match_id),
    operator_id UUID NOT NULL REFERENCES admin_users(id),
    claimed_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT match_operator_leases_expiry_after_claim CHECK (expires_at > claimed_at)
);
