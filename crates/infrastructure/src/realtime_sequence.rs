use football_contracts::{RealtimeSequence, RealtimeTopic};
use sqlx::{Error as SqlxError, PgPool, Postgres, Transaction};
use std::fmt;

#[derive(Debug)]
pub enum RealtimeSequenceRepositoryError {
    Database(SqlxError),
    InvalidStoredSequence { topic: String, value: i64 },
}

impl fmt::Display for RealtimeSequenceRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => {
                write!(
                    formatter,
                    "realtime sequence database operation failed: {error}"
                )
            }
            Self::InvalidStoredSequence { topic, value } => write!(
                formatter,
                "realtime topic {topic} has invalid stored sequence {value}"
            ),
        }
    }
}

impl std::error::Error for RealtimeSequenceRepositoryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::InvalidStoredSequence { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllocatedRealtimeTopicSequence {
    pub topic: RealtimeTopic,
    pub sequence: RealtimeSequence,
}

fn decode_sequence(
    topic: RealtimeTopic,
    value: i64,
) -> Result<AllocatedRealtimeTopicSequence, RealtimeSequenceRepositoryError> {
    let topic_name = topic.as_topic_name();
    let sequence_value = u64::try_from(value).map_err(|_| {
        RealtimeSequenceRepositoryError::InvalidStoredSequence {
            topic: topic_name.clone(),
            value,
        }
    })?;
    let sequence = RealtimeSequence::try_new(sequence_value).ok_or(
        RealtimeSequenceRepositoryError::InvalidStoredSequence {
            topic: topic_name,
            value,
        },
    )?;

    Ok(AllocatedRealtimeTopicSequence { topic, sequence })
}

/// Atomically allocates the next positive sequence for one topic.
///
/// The allocation is part of the caller's transaction. Concurrent allocators
/// serialize on the topic primary key, while a rollback makes the allocation
/// available again instead of creating an unobserved gap.
pub async fn allocate_realtime_topic_sequence(
    transaction: &mut Transaction<'_, Postgres>,
    topic: RealtimeTopic,
) -> Result<AllocatedRealtimeTopicSequence, RealtimeSequenceRepositoryError> {
    let topic_name = topic.as_topic_name();
    let value: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO realtime_topic_sequences (topic, last_sequence)
        VALUES ($1, 1)
        ON CONFLICT (topic) DO UPDATE
            SET last_sequence = realtime_topic_sequences.last_sequence + 1
        RETURNING last_sequence
        "#,
    )
    .bind(topic_name)
    .fetch_one(&mut **transaction)
    .await
    .map_err(RealtimeSequenceRepositoryError::Database)?;

    decode_sequence(topic, value)
}

pub async fn load_realtime_topic_sequence(
    pool: &PgPool,
    topic: RealtimeTopic,
) -> Result<Option<AllocatedRealtimeTopicSequence>, RealtimeSequenceRepositoryError> {
    let topic_name = topic.as_topic_name();
    let value: Option<i64> =
        sqlx::query_scalar("SELECT last_sequence FROM realtime_topic_sequences WHERE topic = $1")
            .bind(topic_name)
            .fetch_optional(pool)
            .await
            .map_err(RealtimeSequenceRepositoryError::Database)?;

    value.map(|value| decode_sequence(topic, value)).transpose()
}
