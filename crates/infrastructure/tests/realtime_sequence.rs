use football_contracts::{RealtimeSequence, RealtimeTopic};
use football_infrastructure::{
    DatabasePoolConfig, MIGRATIONS_DIRECTORY, RealtimeSequenceRepositoryError,
    allocate_realtime_topic_sequence, connect_database, load_realtime_topic_sequence,
    run_migrations,
};
use sqlx::{PgPool, query, query_scalar};
use std::path::PathBuf;
use uuid::Uuid;

fn repository_migrations_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(MIGRATIONS_DIRECTORY)
}

fn uuid(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("realtime sequence fixture UUID should be valid")
}

fn topic() -> RealtimeTopic {
    RealtimeTopic::match_topic(uuid("019ff200-3bc5-7cf0-b3d2-0728c077a930"))
}

async fn apply_migrations(pool: &PgPool) {
    let history_table_exists: bool =
        query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
            .fetch_one(pool)
            .await
            .expect("migration history preflight query should succeed");
    assert!(
        !history_table_exists,
        "realtime sequence target is not clean: _sqlx_migrations already exists"
    );

    run_migrations(pool, repository_migrations_path())
        .await
        .expect("all repository migrations should apply to the clean target");
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn realtime_topic_sequence_is_monotonic_transactional_and_concurrent() {
    let confirmation = std::env::var("ALLOW_CLEAN_DATABASE_MIGRATION")
        .expect("set ALLOW_CLEAN_DATABASE_MIGRATION=1 to authorize this database operation");
    assert_eq!(
        confirmation, "1",
        "set ALLOW_CLEAN_DATABASE_MIGRATION=1 to authorize this database operation"
    );

    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must point to an explicitly approved clean database");
    let pool = connect_database(&database_url, DatabasePoolConfig::default())
        .await
        .expect("clean realtime sequence target should accept a PostgreSQL connection");
    apply_migrations(&pool).await;

    let realtime_topic = topic();
    let mut first_transaction = pool
        .begin()
        .await
        .expect("first sequence transaction should begin");
    let first = allocate_realtime_topic_sequence(&mut first_transaction, realtime_topic)
        .await
        .expect("first topic sequence should allocate");
    assert_eq!(first.topic, realtime_topic);
    assert_eq!(first.sequence, RealtimeSequence::first());

    let pool_for_second = pool.clone();
    let second_task = tokio::spawn(async move {
        let mut second_transaction = pool_for_second
            .begin()
            .await
            .expect("second sequence transaction should begin");
        let second = allocate_realtime_topic_sequence(&mut second_transaction, realtime_topic)
            .await
            .expect("second concurrent topic sequence should allocate");
        second_transaction
            .commit()
            .await
            .expect("second sequence transaction should commit");
        second
    });
    tokio::task::yield_now().await;

    first_transaction
        .commit()
        .await
        .expect("first sequence transaction should commit");
    let second = second_task
        .await
        .expect("second sequence task should complete");
    assert_eq!(second.topic, realtime_topic);
    assert_eq!(second.sequence.value(), 2);

    let stored_after_concurrent = load_realtime_topic_sequence(&pool, realtime_topic)
        .await
        .expect("stored concurrent sequence should load")
        .expect("topic sequence row should exist after commit");
    assert_eq!(stored_after_concurrent.sequence.value(), 2);

    let mut rolled_back_transaction = pool
        .begin()
        .await
        .expect("rollback sequence transaction should begin");
    let rolled_back =
        allocate_realtime_topic_sequence(&mut rolled_back_transaction, realtime_topic)
            .await
            .expect("rolled-back sequence should allocate before rollback");
    assert_eq!(rolled_back.sequence.value(), 3);
    rolled_back_transaction
        .rollback()
        .await
        .expect("sequence allocation transaction should roll back");

    let after_rollback = load_realtime_topic_sequence(&pool, realtime_topic)
        .await
        .expect("sequence after rollback should load")
        .expect("topic sequence row should remain after rollback");
    assert_eq!(after_rollback.sequence.value(), 2);

    let mut third_transaction = pool
        .begin()
        .await
        .expect("third sequence transaction should begin");
    let third = allocate_realtime_topic_sequence(&mut third_transaction, realtime_topic)
        .await
        .expect("third topic sequence should allocate");
    assert_eq!(third.sequence.value(), 3);
    third_transaction
        .commit()
        .await
        .expect("third sequence transaction should commit");

    let final_sequence = load_realtime_topic_sequence(&pool, realtime_topic)
        .await
        .expect("final sequence should load")
        .expect("final topic sequence row should exist");
    assert_eq!(final_sequence.sequence.value(), 3);

    let unseen_topic = RealtimeTopic::match_topic(uuid("019ff201-3bc5-7cf0-b3d2-0728c077a930"));
    assert!(
        load_realtime_topic_sequence(&pool, unseen_topic)
            .await
            .expect("unseen topic sequence lookup should succeed")
            .is_none()
    );

    let invalid_topic = RealtimeTopic::match_topic(uuid("019ff202-3bc5-7cf0-b3d2-0728c077a930"));
    query(
        "INSERT INTO realtime_topic_sequences (topic, last_sequence)
         VALUES ($1, 0)",
    )
    .bind(invalid_topic.to_string())
    .execute(&pool)
    .await
    .expect("invalid sequence fixture row should insert");
    assert!(matches!(
        load_realtime_topic_sequence(&pool, invalid_topic).await,
        Err(RealtimeSequenceRepositoryError::InvalidStoredSequence { value: 0, .. })
    ));

    pool.close().await;
}
