-- The match-summary projection is disposable read state derived from the
-- canonical match stream and aggregate state. Timeline rows, lineup rows, and
-- projection/source versions are introduced by later PRJ increments.
CREATE TABLE match_summary_projections (
    match_id UUID PRIMARY KEY,
    fixture_id UUID NOT NULL UNIQUE REFERENCES fixtures(id),
    home_goals BIGINT NOT NULL DEFAULT 0,
    away_goals BIGINT NOT NULL DEFAULT 0,
    match_state TEXT NOT NULL DEFAULT 'draft',
    verification_status TEXT NOT NULL DEFAULT 'provisional',
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT match_summary_home_goals_valid CHECK (
        home_goals BETWEEN 0 AND 4294967295
    ),
    CONSTRAINT match_summary_away_goals_valid CHECK (
        away_goals BETWEEN 0 AND 4294967295
    ),
    CONSTRAINT match_summary_state_valid CHECK (
        match_state IN (
            'draft',
            'scheduled',
            'pre_match',
            'lineups_provisional',
            'lineups_confirmed',
            'first_half',
            'half_time',
            'second_half',
            'interrupted',
            'finished',
            'postponed',
            'cancelled'
        )
    ),
    CONSTRAINT match_summary_verification_status_valid CHECK (
        verification_status IN ('provisional', 'verified')
    )
);
