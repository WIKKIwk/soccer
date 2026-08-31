use football_domain::{Competition, CompetitionId, CompetitionName, Season, SeasonId, SeasonYear};
use football_infrastructure::{
    DatabasePoolConfig, MIGRATIONS_DIRECTORY, advance_season_results_revision, connect_database,
    load_season, persist_competition, persist_season, run_migrations,
};
use sqlx::query_scalar;
use std::path::PathBuf;
use uuid::Uuid;

fn repository_migrations_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(MIGRATIONS_DIRECTORY)
}

fn uuid(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("season results revision fixture UUID should be valid")
}

fn competition() -> Competition {
    Competition::new(
        CompetitionId::from_uuid(uuid("019ffb00-3bc5-7cf0-b3d2-0728c077a930")),
        CompetitionName::try_new("O‘zbekiston Superligasi")
            .expect("fixture competition name should be valid"),
    )
}

fn season() -> Season {
    Season::new(
        SeasonId::from_uuid(uuid("019ffb01-3bc5-7cf0-b3d2-0728c077a930")),
        competition().id(),
        SeasonYear::try_new(2026).expect("fixture season year should be valid"),
    )
}

async fn apply_migrations(pool: &sqlx::PgPool) {
    let history_table_exists: bool =
        query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
            .fetch_one(pool)
            .await
            .expect("migration history preflight query should succeed");
    assert!(
        !history_table_exists,
        "season revision target is not clean: _sqlx_migrations already exists"
    );

    run_migrations(pool, repository_migrations_path())
        .await
        .expect("all repository migrations should apply to the clean target");
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn season_results_revision_is_zero_monotonic_and_transactional() {
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
        .expect("clean season revision target should accept a PostgreSQL connection");
    apply_migrations(&pool).await;

    persist_competition(&pool, &competition())
        .await
        .expect("revision fixture competition should persist");
    let initial = persist_season(&pool, season())
        .await
        .expect("revision fixture season should persist");
    assert_eq!(initial.results_revision().value(), 0);

    let mut rolled_back = pool
        .begin()
        .await
        .expect("rollback revision transaction should begin");
    let rolled_back_revision = advance_season_results_revision(&mut rolled_back, season().id())
        .await
        .expect("revision should advance inside a transaction");
    assert_eq!(rolled_back_revision.value(), 1);
    rolled_back
        .rollback()
        .await
        .expect("revision transaction should roll back");

    let after_rollback = load_season(&pool, season().id())
        .await
        .expect("season should load after rollback")
        .expect("season should remain persisted after rollback");
    assert_eq!(after_rollback.results_revision().value(), 0);

    let mut committed = pool
        .begin()
        .await
        .expect("committed revision transaction should begin");
    let committed_revision = advance_season_results_revision(&mut committed, season().id())
        .await
        .expect("revision should advance before commit");
    assert_eq!(committed_revision.value(), 1);
    committed
        .commit()
        .await
        .expect("revision transaction should commit");

    let persisted_again = persist_season(&pool, season())
        .await
        .expect("identity-only season seed should remain idempotent");
    assert_eq!(persisted_again.results_revision().value(), 1);

    let mut second = pool
        .begin()
        .await
        .expect("second revision transaction should begin");
    let second_revision = advance_season_results_revision(&mut second, season().id())
        .await
        .expect("second revision should advance before commit");
    assert_eq!(second_revision.value(), 2);
    second
        .commit()
        .await
        .expect("second revision transaction should commit");

    let final_season = load_season(&pool, season().id())
        .await
        .expect("season should load after committed revisions")
        .expect("season should remain persisted after committed revisions");
    assert_eq!(final_season.results_revision().value(), 2);

    pool.close().await;
}
