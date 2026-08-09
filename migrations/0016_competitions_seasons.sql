CREATE TABLE competitions (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT competitions_name_not_blank CHECK (btrim(name) <> '')
);

CREATE TABLE seasons (
    id UUID PRIMARY KEY,
    competition_id UUID NOT NULL REFERENCES competitions(id),
    year INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT seasons_year_positive CHECK (year > 0 AND year <= 65535),
    CONSTRAINT seasons_competition_year_unique UNIQUE (competition_id, year)
);
