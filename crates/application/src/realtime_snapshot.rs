use crate::realtime_gap::RealtimeSequenceGap;
use football_contracts::{
    RealtimeSnapshotRequiredReason, RealtimeSubscriptionResponse, RealtimeTopic,
};
use uuid::Uuid;

/// Builds the control response for a reconnect whose retained sequence cannot
/// be replayed safely. Snapshot data is intentionally not included here; the
/// canonical REST snapshot boundary belongs to RT-012.
pub fn realtime_snapshot_required_response(
    request_id: Uuid,
    topic: RealtimeTopic,
    gap: RealtimeSequenceGap,
) -> RealtimeSubscriptionResponse {
    let reason = match gap {
        RealtimeSequenceGap::BeforeRetention { .. } => {
            RealtimeSnapshotRequiredReason::BeforeRetention
        }
        RealtimeSequenceGap::InsideRetention { .. } => {
            RealtimeSnapshotRequiredReason::InsideRetention
        }
        RealtimeSequenceGap::ClientAheadOfNewest { .. } => {
            RealtimeSnapshotRequiredReason::ClientAheadOfNewest
        }
    };

    RealtimeSubscriptionResponse::SnapshotRequired {
        request_id,
        topic,
        reason,
    }
}

#[cfg(test)]
mod tests {
    use super::realtime_snapshot_required_response;
    use crate::realtime_gap::RealtimeSequenceGap;
    use football_contracts::{
        RealtimeSnapshotRequiredReason, RealtimeSubscriptionResponse, RealtimeTopic,
    };
    use uuid::Uuid;

    fn request_id() -> Uuid {
        Uuid::parse_str("019ff160-3bc5-7cf0-b3d2-0728c077a930")
            .expect("snapshot response request ID should be valid")
    }

    fn topic() -> RealtimeTopic {
        RealtimeTopic::match_topic(
            Uuid::parse_str("019ff161-3bc5-7cf0-b3d2-0728c077a930")
                .expect("snapshot response topic ID should be valid"),
        )
    }

    fn sequence(value: u64) -> football_contracts::RealtimeSequence {
        football_contracts::RealtimeSequence::try_new(value)
            .expect("snapshot response fixture sequence should be positive")
    }

    #[test]
    fn maps_each_realtime_gap_to_a_snapshot_required_response() {
        let cases = [
            (
                RealtimeSequenceGap::BeforeRetention {
                    requested_sequence: sequence(17),
                    oldest_retained_sequence: sequence(18),
                },
                RealtimeSnapshotRequiredReason::BeforeRetention,
            ),
            (
                RealtimeSequenceGap::InsideRetention {
                    expected_sequence: sequence(19),
                    found_sequence: None,
                },
                RealtimeSnapshotRequiredReason::InsideRetention,
            ),
            (
                RealtimeSequenceGap::ClientAheadOfNewest {
                    last_known_sequence: sequence(21),
                    newest_retained_sequence: sequence(20),
                },
                RealtimeSnapshotRequiredReason::ClientAheadOfNewest,
            ),
        ];

        for (gap, reason) in cases {
            assert_eq!(
                realtime_snapshot_required_response(request_id(), topic(), gap),
                RealtimeSubscriptionResponse::SnapshotRequired {
                    request_id: request_id(),
                    topic: topic(),
                    reason,
                }
            );
        }
    }
}
