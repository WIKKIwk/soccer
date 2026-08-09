-- PRJ-005 stores the source aggregate and published projection generation
-- for every match read model. A projection row is not current until a later
-- writer publishes a complete generation with all freshness metadata.
ALTER TABLE match_summary_projections
    ADD COLUMN source_aggregate_version BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN projection_version BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN computed_at_utc TIMESTAMPTZ,
    ADD COLUMN is_rebuilding BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN is_current BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN last_error_code TEXT,
    ADD CONSTRAINT match_summary_projection_source_version_nonnegative CHECK (
        source_aggregate_version >= 0
    ),
    ADD CONSTRAINT match_summary_projection_version_nonnegative CHECK (
        projection_version >= 0
    ),
    ADD CONSTRAINT match_summary_projection_current_generation_valid CHECK (
        NOT is_current OR projection_version > 0
    ),
    ADD CONSTRAINT match_summary_projection_current_computed_at_valid CHECK (
        NOT is_current OR computed_at_utc IS NOT NULL
    ),
    ADD CONSTRAINT match_summary_projection_current_state_valid CHECK (
        NOT (is_rebuilding AND is_current)
    ),
    ADD CONSTRAINT match_summary_projection_current_error_free CHECK (
        NOT is_current OR last_error_code IS NULL
    ),
    ADD CONSTRAINT match_summary_projection_error_code_not_blank CHECK (
        last_error_code IS NULL OR char_length(btrim(last_error_code)) > 0
    );

ALTER TABLE match_lineup_projections
    ADD COLUMN source_aggregate_version BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN projection_version BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN computed_at_utc TIMESTAMPTZ,
    ADD COLUMN is_rebuilding BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN is_current BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN last_error_code TEXT,
    ADD CONSTRAINT match_lineup_projection_source_version_nonnegative CHECK (
        source_aggregate_version >= 0
    ),
    ADD CONSTRAINT match_lineup_projection_version_nonnegative CHECK (
        projection_version >= 0
    ),
    ADD CONSTRAINT match_lineup_projection_current_generation_valid CHECK (
        NOT is_current OR projection_version > 0
    ),
    ADD CONSTRAINT match_lineup_projection_current_computed_at_valid CHECK (
        NOT is_current OR computed_at_utc IS NOT NULL
    ),
    ADD CONSTRAINT match_lineup_projection_current_state_valid CHECK (
        NOT (is_rebuilding AND is_current)
    ),
    ADD CONSTRAINT match_lineup_projection_current_error_free CHECK (
        NOT is_current OR last_error_code IS NULL
    ),
    ADD CONSTRAINT match_lineup_projection_error_code_not_blank CHECK (
        last_error_code IS NULL OR char_length(btrim(last_error_code)) > 0
    );

-- Timeline rows are event-level, so their match-level freshness metadata is
-- kept in one head row. The head can exist for an empty timeline and does not
-- copy or replace the canonical event stream.
CREATE TABLE match_timeline_projection_heads (
    match_id UUID PRIMARY KEY,
    source_aggregate_version BIGINT NOT NULL DEFAULT 0,
    projection_version BIGINT NOT NULL DEFAULT 0,
    computed_at_utc TIMESTAMPTZ,
    is_rebuilding BOOLEAN NOT NULL DEFAULT FALSE,
    is_current BOOLEAN NOT NULL DEFAULT FALSE,
    last_error_code TEXT,
    CONSTRAINT match_timeline_projection_head_source_version_nonnegative CHECK (
        source_aggregate_version >= 0
    ),
    CONSTRAINT match_timeline_projection_head_version_nonnegative CHECK (
        projection_version >= 0
    ),
    CONSTRAINT match_timeline_projection_head_current_generation_valid CHECK (
        NOT is_current OR projection_version > 0
    ),
    CONSTRAINT match_timeline_projection_head_current_computed_at_valid CHECK (
        NOT is_current OR computed_at_utc IS NOT NULL
    ),
    CONSTRAINT match_timeline_projection_head_current_state_valid CHECK (
        NOT (is_rebuilding AND is_current)
    ),
    CONSTRAINT match_timeline_projection_head_current_error_free CHECK (
        NOT is_current OR last_error_code IS NULL
    ),
    CONSTRAINT match_timeline_projection_head_error_code_not_blank CHECK (
        last_error_code IS NULL OR char_length(btrim(last_error_code)) > 0
    )
);
