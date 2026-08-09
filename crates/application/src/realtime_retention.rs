use football_contracts::RealtimeSequence;
use std::fmt;

pub const DEFAULT_REALTIME_RETAINED_MESSAGES_PER_TOPIC: u64 = 256;
pub const DEFAULT_REALTIME_RETAINED_MESSAGE_MAX_AGE_MILLIS: u64 = 15 * 60 * 1_000;

/// Bounded replay policy for one realtime topic.
///
/// A message must satisfy both the sequence-count bound and the age bound to
/// remain eligible for reconnect replay. The policy does not own storage or a
/// clock; the future transport/storage adapter supplies the newest sequence
/// and the non-negative message age.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RealtimeRetentionPolicy {
    pub max_messages_per_topic: u64,
    pub max_age_millis: u64,
}

impl Default for RealtimeRetentionPolicy {
    fn default() -> Self {
        Self {
            max_messages_per_topic: DEFAULT_REALTIME_RETAINED_MESSAGES_PER_TOPIC,
            max_age_millis: DEFAULT_REALTIME_RETAINED_MESSAGE_MAX_AGE_MILLIS,
        }
    }
}

impl RealtimeRetentionPolicy {
    pub fn validate(self) -> Result<(), RealtimeRetentionPolicyError> {
        if self.max_messages_per_topic == 0 {
            return Err(RealtimeRetentionPolicyError::MaxMessagesPerTopicMustBePositive);
        }
        if self.max_age_millis == 0 {
            return Err(RealtimeRetentionPolicyError::MaxAgeMustBePositive);
        }
        Ok(())
    }

    pub fn window_for(
        self,
        newest_sequence: RealtimeSequence,
    ) -> Result<RealtimeRetainedMessageWindow, RealtimeRetentionPolicyError> {
        self.validate()?;

        let oldest_value = newest_sequence
            .value()
            .saturating_sub(self.max_messages_per_topic.saturating_sub(1))
            .max(RealtimeSequence::first().value());
        let oldest_sequence = RealtimeSequence::try_new(oldest_value)
            .expect("retention sequence floor must remain positive");

        Ok(RealtimeRetainedMessageWindow {
            oldest_sequence,
            newest_sequence,
            max_age_millis: self.max_age_millis,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RealtimeRetainedMessageWindow {
    oldest_sequence: RealtimeSequence,
    newest_sequence: RealtimeSequence,
    max_age_millis: u64,
}

impl RealtimeRetainedMessageWindow {
    pub const fn oldest_sequence(self) -> RealtimeSequence {
        self.oldest_sequence
    }

    pub const fn newest_sequence(self) -> RealtimeSequence {
        self.newest_sequence
    }

    pub const fn max_age_millis(self) -> u64 {
        self.max_age_millis
    }

    pub const fn contains_sequence(self, sequence: RealtimeSequence) -> bool {
        sequence.value() >= self.oldest_sequence.value()
            && sequence.value() <= self.newest_sequence.value()
    }

    /// Returns true only when both sequence and age bounds retain the message.
    /// A message at exactly the age boundary is expired.
    pub const fn retains(self, sequence: RealtimeSequence, age_millis: u64) -> bool {
        self.contains_sequence(sequence) && age_millis < self.max_age_millis
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealtimeRetentionPolicyError {
    MaxMessagesPerTopicMustBePositive,
    MaxAgeMustBePositive,
}

impl fmt::Display for RealtimeRetentionPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::MaxMessagesPerTopicMustBePositive => "max_messages_per_topic must be positive",
            Self::MaxAgeMustBePositive => "max_age_millis must be positive",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for RealtimeRetentionPolicyError {}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_REALTIME_RETAINED_MESSAGE_MAX_AGE_MILLIS,
        DEFAULT_REALTIME_RETAINED_MESSAGES_PER_TOPIC, RealtimeRetentionPolicy,
        RealtimeRetentionPolicyError,
    };
    use football_contracts::RealtimeSequence;

    #[test]
    fn default_retention_policy_is_bounded_and_valid() {
        let policy = RealtimeRetentionPolicy::default();
        assert_eq!(
            policy.max_messages_per_topic,
            DEFAULT_REALTIME_RETAINED_MESSAGES_PER_TOPIC
        );
        assert_eq!(
            policy.max_age_millis,
            DEFAULT_REALTIME_RETAINED_MESSAGE_MAX_AGE_MILLIS
        );
        assert!(policy.validate().is_ok());
    }

    #[test]
    fn rejects_zero_retention_bounds() {
        assert_eq!(
            RealtimeRetentionPolicy {
                max_messages_per_topic: 0,
                ..RealtimeRetentionPolicy::default()
            }
            .validate(),
            Err(RealtimeRetentionPolicyError::MaxMessagesPerTopicMustBePositive)
        );
        assert_eq!(
            RealtimeRetentionPolicy {
                max_age_millis: 0,
                ..RealtimeRetentionPolicy::default()
            }
            .validate(),
            Err(RealtimeRetentionPolicyError::MaxAgeMustBePositive)
        );
    }

    #[test]
    fn computes_a_sequence_window_without_underflow() {
        let policy = RealtimeRetentionPolicy {
            max_messages_per_topic: 3,
            max_age_millis: 1_000,
        };
        let early_window = policy
            .window_for(RealtimeSequence::try_new(2).unwrap())
            .expect("early retention window should build");
        assert_eq!(early_window.oldest_sequence().value(), 1);
        assert_eq!(early_window.newest_sequence().value(), 2);

        let later_window = policy
            .window_for(RealtimeSequence::try_new(20).unwrap())
            .expect("later retention window should build");
        assert_eq!(later_window.oldest_sequence().value(), 18);
        assert!(!later_window.contains_sequence(RealtimeSequence::try_new(17).unwrap()));
        assert!(later_window.contains_sequence(RealtimeSequence::try_new(18).unwrap()));
        assert!(later_window.contains_sequence(RealtimeSequence::try_new(20).unwrap()));
        assert!(!later_window.contains_sequence(RealtimeSequence::try_new(21).unwrap()));
    }

    #[test]
    fn requires_both_sequence_and_age_bounds() {
        let policy = RealtimeRetentionPolicy {
            max_messages_per_topic: 3,
            max_age_millis: 1_000,
        };
        let window = policy
            .window_for(RealtimeSequence::try_new(20).unwrap())
            .expect("retention window should build");

        assert!(window.retains(RealtimeSequence::try_new(18).unwrap(), 999));
        assert!(!window.retains(RealtimeSequence::try_new(18).unwrap(), 1_000));
        assert!(!window.retains(RealtimeSequence::try_new(17).unwrap(), 1));
    }
}
