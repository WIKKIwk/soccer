-- Timeline rows are a disposable ordered view of canonical C1 event
-- metadata. The payload and correction history remain owned by match_events.
-- The composite source key prevents a row from pairing an event with another
-- match while still retaining match_id for efficient projection queries.
ALTER TABLE match_events
    ADD CONSTRAINT match_events_event_match_unique UNIQUE (event_id, match_id);

CREATE TABLE match_timeline_projections (
    event_id UUID PRIMARY KEY,
    match_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL,
    event_index BIGINT NOT NULL,
    event_type TEXT NOT NULL,
    period TEXT NOT NULL,
    clock_ms BIGINT NOT NULL,
    CONSTRAINT match_timeline_source_fk
        FOREIGN KEY (event_id, match_id)
        REFERENCES match_events (event_id, match_id),
    CONSTRAINT match_timeline_aggregate_version_positive CHECK (aggregate_version > 0),
    CONSTRAINT match_timeline_event_index_nonnegative CHECK (event_index >= 0),
    CONSTRAINT match_timeline_event_type_valid CHECK (
        event_type IN (
            'GoalRecorded',
            'YellowCardRecorded',
            'SecondYellowCardRecorded',
            'RedCardRecorded',
            'SubstitutionRecorded'
        )
    ),
    CONSTRAINT match_timeline_period_live CHECK (
        period IN ('first_half', 'second_half')
    ),
    CONSTRAINT match_timeline_clock_nonnegative CHECK (clock_ms >= 0),
    CONSTRAINT match_timeline_match_version_unique UNIQUE (match_id, aggregate_version)
);

CREATE INDEX match_timeline_match_order_idx
    ON match_timeline_projections (match_id, aggregate_version, event_id);
