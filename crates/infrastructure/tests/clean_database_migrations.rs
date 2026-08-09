use football_infrastructure::{
    DatabasePoolConfig, MIGRATIONS_DIRECTORY, connect_database, load_migrator, run_migrations,
};
use sqlx::query_scalar;
use std::path::PathBuf;

fn repository_migrations_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(MIGRATIONS_DIRECTORY)
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn applies_all_repository_migrations_to_a_clean_database() {
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
        .expect("clean migration target should accept a PostgreSQL connection");

    let history_table_exists: bool =
        query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
            .fetch_one(&pool)
            .await
            .expect("migration history preflight query should succeed");
    assert!(
        !history_table_exists,
        "migration target is not clean: _sqlx_migrations already exists"
    );

    let migrator = load_migrator(repository_migrations_path())
        .await
        .expect("repository migrations should resolve");
    let expected_versions: Vec<i64> = migrator.iter().map(|migration| migration.version).collect();

    run_migrations(&pool, repository_migrations_path())
        .await
        .expect("all repository migrations should apply to a clean target");

    let applied_versions: Vec<i64> =
        query_scalar("SELECT version FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pool)
            .await
            .expect("migration history should be queryable after application");

    assert_eq!(applied_versions, expected_versions);
    pool.close().await;
}
