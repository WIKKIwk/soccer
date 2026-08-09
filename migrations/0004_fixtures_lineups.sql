-- Competition and season master tables are introduced by later CMP persistence
-- increments; their canonical IDs are retained here without fabricated rows.
CREATE TABLE fixtures (
    id UUID PRIMARY KEY,
    competition_id UUID NOT NULL,
    season_id UUID NOT NULL,
    home_team_id UUID NOT NULL REFERENCES teams(id),
    away_team_id UUID NOT NULL REFERENCES teams(id),
    kickoff_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT fixtures_distinct_teams CHECK (home_team_id <> away_team_id)
);

CREATE TABLE lineups (
    id UUID PRIMARY KEY,
    fixture_id UUID NOT NULL REFERENCES fixtures(id),
    team_id UUID NOT NULL REFERENCES teams(id),
    status TEXT NOT NULL DEFAULT 'draft',
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT lineups_status_is_draft CHECK (status = 'draft'),
    CONSTRAINT lineups_fixture_team_unique UNIQUE (fixture_id, team_id)
);

CREATE TABLE lineup_entries (
    lineup_id UUID NOT NULL REFERENCES lineups(id),
    player_id UUID NOT NULL REFERENCES players(id),
    role TEXT NOT NULL,
    CONSTRAINT lineup_entries_role_valid CHECK (role IN ('starter', 'substitute')),
    PRIMARY KEY (lineup_id, player_id)
);
