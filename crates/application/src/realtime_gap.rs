use crate::realtime_retention::RealtimeRetainedMessageWindow;
use football_contracts::RealtimeSequence;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealtimeReplayAssessment {
    NoCursor,
    UpToDate,
    Replayable {
        from_sequence: RealtimeSequence,
        through_sequence: RealtimeSequence,
    },
    Gap(RealtimeSequenceGap),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealtimeSequenceGap {
    BeforeRetention {
        requested_sequence: RealtimeSequence,
        oldest_retained_sequence: RealtimeSequence,
    },
    InsideRetention {
        expected_sequence: RealtimeSequence,
        found_sequence: Option<RealtimeSequence>,
    },
    ClientAheadOfNewest {
        last_known_sequence: RealtimeSequence,
        newest_retained_sequence: RealtimeSequence,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealtimeRetainedSequenceError {
    OutsideWindow {
        sequence: RealtimeSequence,
        oldest_retained_sequence: RealtimeSequence,
        newest_retained_sequence: RealtimeSequence,
    },
    NotStrictlyIncreasing {
        previous_sequence: RealtimeSequence,
        current_sequence: RealtimeSequence,
    },
}

impl fmt::Display for RealtimeRetainedSequenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutsideWindow {
                sequence,
                oldest_retained_sequence,
                newest_retained_sequence,
            } => write!(
                formatter,
                "retained sequence {} is outside window {}..={}",
                sequence.value(),
                oldest_retained_sequence.value(),
                newest_retained_sequence.value()
            ),
            Self::NotStrictlyIncreasing {
                previous_sequence,
                current_sequence,
            } => write!(
                formatter,
                "retained sequences are not strictly increasing: {} then {}",
                previous_sequence.value(),
                current_sequence.value()
            ),
        }
    }
}

impl std::error::Error for RealtimeRetainedSequenceError {}

/// Assesses whether a client cursor can be replayed from a complete retained
/// sequence list for one topic.
///
/// `retained_sequences` must be ordered by sequence and represent the rows the
/// storage adapter made available for this reconnect attempt. The function
/// detects both a cursor that predates the retention window and an internal or
/// trailing hole in the retained sequence list. It does not decide the
/// `snapshot_required` wire response; RT-011 owns that contract.
pub fn assess_realtime_replay(
    last_known_sequence: Option<RealtimeSequence>,
    window: RealtimeRetainedMessageWindow,
    retained_sequences: &[RealtimeSequence],
) -> Result<RealtimeReplayAssessment, RealtimeRetainedSequenceError> {
    validate_retained_sequence_order(window, retained_sequences)?;

    let Some(last_known_sequence) = last_known_sequence else {
        return Ok(RealtimeReplayAssessment::NoCursor);
    };

    let newest_sequence = window.newest_sequence();
    if last_known_sequence.value() > newest_sequence.value() {
        return Ok(RealtimeReplayAssessment::Gap(
            RealtimeSequenceGap::ClientAheadOfNewest {
                last_known_sequence,
                newest_retained_sequence: newest_sequence,
            },
        ));
    }
    if last_known_sequence == newest_sequence {
        return Ok(RealtimeReplayAssessment::UpToDate);
    }

    let requested_sequence = RealtimeSequence::try_new(last_known_sequence.value() + 1)
        .expect("a non-newest positive sequence must have a next sequence");
    let first_replay_sequence = if requested_sequence.value() < window.oldest_sequence().value() {
        return Ok(RealtimeReplayAssessment::Gap(
            RealtimeSequenceGap::BeforeRetention {
                requested_sequence,
                oldest_retained_sequence: window.oldest_sequence(),
            },
        ));
    } else {
        requested_sequence
    };

    let mut expected_sequence = first_replay_sequence;
    for sequence in retained_sequences {
        if sequence.value() <= last_known_sequence.value() {
            continue;
        }
        if *sequence != expected_sequence {
            return Ok(RealtimeReplayAssessment::Gap(
                RealtimeSequenceGap::InsideRetention {
                    expected_sequence,
                    found_sequence: Some(*sequence),
                },
            ));
        }
        if *sequence == newest_sequence {
            return Ok(RealtimeReplayAssessment::Replayable {
                from_sequence: first_replay_sequence,
                through_sequence: newest_sequence,
            });
        }
        expected_sequence = RealtimeSequence::try_new(sequence.value() + 1)
            .expect("a non-newest retained sequence must have a next sequence");
    }

    Ok(RealtimeReplayAssessment::Gap(
        RealtimeSequenceGap::InsideRetention {
            expected_sequence,
            found_sequence: None,
        },
    ))
}

fn validate_retained_sequence_order(
    window: RealtimeRetainedMessageWindow,
    retained_sequences: &[RealtimeSequence],
) -> Result<(), RealtimeRetainedSequenceError> {
    let mut previous_sequence = None;
    for sequence in retained_sequences {
        if !window.contains_sequence(*sequence) {
            return Err(RealtimeRetainedSequenceError::OutsideWindow {
                sequence: *sequence,
                oldest_retained_sequence: window.oldest_sequence(),
                newest_retained_sequence: window.newest_sequence(),
            });
        }
        if let Some(previous_sequence) = previous_sequence
            && *sequence <= previous_sequence
        {
            return Err(RealtimeRetainedSequenceError::NotStrictlyIncreasing {
                previous_sequence,
                current_sequence: *sequence,
            });
        }
        previous_sequence = Some(*sequence);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        RealtimeReplayAssessment, RealtimeRetainedSequenceError, RealtimeSequenceGap,
        assess_realtime_replay,
    };
    use crate::realtime_retention::RealtimeRetentionPolicy;
    use football_contracts::RealtimeSequence;

    fn sequence(value: u64) -> RealtimeSequence {
        RealtimeSequence::try_new(value).expect("gap fixture sequence should be positive")
    }

    fn window() -> crate::realtime_retention::RealtimeRetainedMessageWindow {
        RealtimeRetentionPolicy {
            max_messages_per_topic: 3,
            max_age_millis: 1_000,
        }
        .window_for(sequence(20))
        .expect("gap fixture window should build")
    }

    #[test]
    fn distinguishes_no_cursor_and_up_to_date_cursor() {
        let retention_window = window();
        assert_eq!(
            assess_realtime_replay(None, retention_window, &[]).unwrap(),
            RealtimeReplayAssessment::NoCursor
        );
        assert_eq!(
            assess_realtime_replay(Some(sequence(20)), retention_window, &[]).unwrap(),
            RealtimeReplayAssessment::UpToDate
        );
    }

    #[test]
    fn detects_a_cursor_before_the_retention_window() {
        let assessment = assess_realtime_replay(Some(sequence(16)), window(), &[]).unwrap();
        assert_eq!(
            assessment,
            RealtimeReplayAssessment::Gap(RealtimeSequenceGap::BeforeRetention {
                requested_sequence: sequence(17),
                oldest_retained_sequence: sequence(18),
            })
        );
    }

    #[test]
    fn detects_a_client_cursor_ahead_of_the_server() {
        let assessment = assess_realtime_replay(Some(sequence(21)), window(), &[]).unwrap();
        assert_eq!(
            assessment,
            RealtimeReplayAssessment::Gap(RealtimeSequenceGap::ClientAheadOfNewest {
                last_known_sequence: sequence(21),
                newest_retained_sequence: sequence(20),
            })
        );
    }

    #[test]
    fn accepts_a_complete_boundary_replay() {
        let assessment = assess_realtime_replay(
            Some(sequence(17)),
            window(),
            &[sequence(18), sequence(19), sequence(20)],
        )
        .unwrap();
        assert_eq!(
            assessment,
            RealtimeReplayAssessment::Replayable {
                from_sequence: sequence(18),
                through_sequence: sequence(20),
            }
        );
    }

    #[test]
    fn detects_internal_and_trailing_retained_sequence_gaps() {
        let internal =
            assess_realtime_replay(Some(sequence(17)), window(), &[sequence(18), sequence(20)])
                .unwrap();
        assert_eq!(
            internal,
            RealtimeReplayAssessment::Gap(RealtimeSequenceGap::InsideRetention {
                expected_sequence: sequence(19),
                found_sequence: Some(sequence(20)),
            })
        );

        let trailing =
            assess_realtime_replay(Some(sequence(17)), window(), &[sequence(18), sequence(19)])
                .unwrap();
        assert_eq!(
            trailing,
            RealtimeReplayAssessment::Gap(RealtimeSequenceGap::InsideRetention {
                expected_sequence: sequence(20),
                found_sequence: None,
            })
        );
    }

    #[test]
    fn rejects_unsorted_or_out_of_window_retained_sequences() {
        assert_eq!(
            assess_realtime_replay(Some(sequence(17)), window(), &[sequence(19), sequence(18)]),
            Err(RealtimeRetainedSequenceError::NotStrictlyIncreasing {
                previous_sequence: sequence(19),
                current_sequence: sequence(18),
            })
        );
        assert!(matches!(
            assess_realtime_replay(Some(sequence(17)), window(), &[sequence(17)]),
            Err(RealtimeRetainedSequenceError::OutsideWindow {
                sequence: observed_sequence,
                oldest_retained_sequence,
                newest_retained_sequence,
            }) if observed_sequence == sequence(17)
                && oldest_retained_sequence == sequence(18)
                && newest_retained_sequence == sequence(20)
        ));
    }
}
