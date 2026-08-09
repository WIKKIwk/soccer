use crate::outbox::OutboxPublishedMessage;
use football_domain::{EventId, UtcInstant};
use std::fmt;

/// The currently measurable end-to-end fanout boundary.
///
/// `published_at` is supplied by the worker after the transport acknowledges
/// the prepared notification. It is intentionally not described as client
/// receipt latency because the WebSocket server and client acknowledgement
/// protocol are not implemented yet.
pub const REALTIME_FANOUT_LATENCY_MEASUREMENT: &str = "outbox_created_to_publish_ack";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RealtimeFanoutLatencySample {
    message_id: EventId,
    outbox_created_at: UtcInstant,
    published_at: UtcInstant,
    elapsed_millis: u64,
}

impl RealtimeFanoutLatencySample {
    pub const fn message_id(self) -> EventId {
        self.message_id
    }

    pub const fn outbox_created_at(self) -> UtcInstant {
        self.outbox_created_at
    }

    pub const fn published_at(self) -> UtcInstant {
        self.published_at
    }

    pub const fn elapsed_millis(self) -> u64 {
        self.elapsed_millis
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealtimeFanoutLatencyError {
    PublishedMessageMismatch { expected: EventId, actual: EventId },
    PublishedBeforeOutboxCreated { message_id: EventId },
    ElapsedMillisOutOfRange { message_id: EventId, value: i128 },
}

impl fmt::Display for RealtimeFanoutLatencyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PublishedMessageMismatch { expected, actual } => write!(
                formatter,
                "published outbox message {actual} does not match expected message {expected}"
            ),
            Self::PublishedBeforeOutboxCreated { message_id } => write!(
                formatter,
                "published timestamp for outbox message {message_id} is before creation"
            ),
            Self::ElapsedMillisOutOfRange { message_id, value } => write!(
                formatter,
                "fanout latency for outbox message {message_id} is outside u64 milliseconds: {value}"
            ),
        }
    }
}

impl std::error::Error for RealtimeFanoutLatencyError {}

/// Measures one committed outbox-to-publish acknowledgement sample.
///
/// The caller must invoke this after the transaction that records
/// `published_at` commits. The function is pure and never changes delivery
/// state or emits a metric by itself.
pub fn measure_realtime_fanout_latency(
    message_id: EventId,
    outbox_created_at: UtcInstant,
    published: OutboxPublishedMessage,
) -> Result<RealtimeFanoutLatencySample, RealtimeFanoutLatencyError> {
    if published.message_id != message_id {
        return Err(RealtimeFanoutLatencyError::PublishedMessageMismatch {
            expected: message_id,
            actual: published.message_id,
        });
    }

    let elapsed_millis = (published.published_at.as_offset_datetime()
        - outbox_created_at.as_offset_datetime())
    .whole_milliseconds();
    if elapsed_millis < 0 {
        return Err(RealtimeFanoutLatencyError::PublishedBeforeOutboxCreated { message_id });
    }
    let elapsed_millis = u64::try_from(elapsed_millis).map_err(|_| {
        RealtimeFanoutLatencyError::ElapsedMillisOutOfRange {
            message_id,
            value: elapsed_millis,
        }
    })?;

    Ok(RealtimeFanoutLatencySample {
        message_id,
        outbox_created_at,
        published_at: published.published_at,
        elapsed_millis,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        REALTIME_FANOUT_LATENCY_MEASUREMENT, RealtimeFanoutLatencyError,
        measure_realtime_fanout_latency,
    };
    use crate::outbox::OutboxPublishedMessage;
    use football_domain::{EventId, UtcInstant};
    use time::{Duration, OffsetDateTime};
    use uuid::Uuid;

    fn event_id() -> EventId {
        EventId::from_uuid(
            Uuid::parse_str("01a00500-3bc5-7cf0-b3d2-0728c077a930")
                .expect("latency fixture event ID should be valid"),
        )
    }

    fn instant_millis(value: i64) -> UtcInstant {
        UtcInstant::from_offset_datetime(OffsetDateTime::UNIX_EPOCH + Duration::milliseconds(value))
    }

    fn published_at(message_id: EventId, millis: i64) -> OutboxPublishedMessage {
        OutboxPublishedMessage {
            message_id,
            attempt_count: 1,
            published_at: instant_millis(millis),
        }
    }

    #[test]
    fn measures_the_named_outbox_to_publish_ack_boundary_in_milliseconds() {
        let message_id = event_id();
        let sample = measure_realtime_fanout_latency(
            message_id,
            instant_millis(10_000),
            published_at(message_id, 11_750),
        )
        .expect("ordered timestamps should produce a latency sample");

        assert_eq!(
            REALTIME_FANOUT_LATENCY_MEASUREMENT,
            "outbox_created_to_publish_ack"
        );
        assert_eq!(sample.message_id(), message_id);
        assert_eq!(sample.outbox_created_at(), instant_millis(10_000));
        assert_eq!(sample.published_at(), instant_millis(11_750));
        assert_eq!(sample.elapsed_millis(), 1_750);
    }

    #[test]
    fn rejects_mismatched_and_backwards_publish_timestamps() {
        let message_id = event_id();
        let other_id = EventId::from_uuid(
            Uuid::parse_str("01a00501-3bc5-7cf0-b3d2-0728c077a930")
                .expect("second latency fixture event ID should be valid"),
        );
        assert!(matches!(
            measure_realtime_fanout_latency(
                message_id,
                instant_millis(10_000),
                published_at(other_id, 11_000),
            ),
            Err(RealtimeFanoutLatencyError::PublishedMessageMismatch { .. })
        ));
        assert!(matches!(
            measure_realtime_fanout_latency(
                message_id,
                instant_millis(10_000),
                published_at(message_id, 9_999),
            ),
            Err(RealtimeFanoutLatencyError::PublishedBeforeOutboxCreated { .. })
        ));
    }
}
