-- Match lineup projections are disposable public read state. Draft lineups
-- remain scheduling/lineup source data; this schema stores the match-scoped
-- provisional or confirmed view without copying player names or team identity.
CREATE TABLE match_lineup_projections (
    match_id UUID PRIMARY KEY,
    fixture_id UUID NOT NULL UNIQUE REFERENCES fixtures(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE match_lineup_team_projections (
    match_id UUID NOT NULL REFERENCES match_lineup_projections(match_id) ON DELETE CASCADE,
    side TEXT NOT NULL,
    lineup_status TEXT NOT NULL DEFAULT 'provisional',
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (match_id, side),
    CONSTRAINT match_lineup_team_side_valid CHECK (side IN ('home', 'away')),
    CONSTRAINT match_lineup_team_status_valid CHECK (
        lineup_status IN ('provisional', 'confirmed')
    )
);

CREATE TABLE match_lineup_entry_projections (
    match_id UUID NOT NULL,
    side TEXT NOT NULL,
    player_id UUID NOT NULL REFERENCES players(id),
    role TEXT NOT NULL,
    PRIMARY KEY (match_id, side, player_id),
    CONSTRAINT match_lineup_entry_team_fk
        FOREIGN KEY (match_id, side)
        REFERENCES match_lineup_team_projections (match_id, side)
        ON DELETE CASCADE,
    CONSTRAINT match_lineup_entry_role_valid CHECK (
        role IN ('starter', 'substitute')
    ),
    CONSTRAINT match_lineup_entry_player_once_per_match UNIQUE (match_id, player_id)
);

CREATE INDEX match_lineup_entry_order_idx
    ON match_lineup_entry_projections (match_id, side, role, player_id);
