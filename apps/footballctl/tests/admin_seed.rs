use football_domain::AdminUserId;
use football_infrastructure::{
    DatabasePoolConfig, MIGRATIONS_DIRECTORY, connect_database, load_admin_user, run_migrations,
};
use std::{path::PathBuf, process::Command};

fn repository_migrations_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(MIGRATIONS_DIRECTORY)
}

fn run_cli(database_url: &str, arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_footballctl"))
        .env("DATABASE_URL", database_url)
        .env("ALLOW_DEVELOPMENT_ADMIN_SEED", "1")
        .args(arguments)
        .output()
        .expect("footballctl test process should start")
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn admin_seed_command_persists_and_reads_back_one_active_administrator() {
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
        .expect("clean administrator target should accept a PostgreSQL connection");
    run_migrations(&pool, repository_migrations_path())
        .await
        .expect("all migrations should apply to the clean administrator target");
    pool.close().await;

    let administrator_id = "019fe290-3bc5-7cf0-b3d2-0728c077a930";
    let seed_arguments = [
        "admin",
        "seed-development",
        "--id",
        administrator_id,
        "--username",
        "dev-admin",
        "--display-name",
        "Development Admin",
    ];

    let persisted = run_cli(&database_url, &seed_arguments);
    assert!(
        persisted.status.success(),
        "administrator seed failed: {}",
        String::from_utf8_lossy(&persisted.stderr)
    );
    assert!(String::from_utf8_lossy(&persisted.stdout).contains(administrator_id));

    let repeated = run_cli(&database_url, &seed_arguments);
    assert!(repeated.status.success());
    assert!(String::from_utf8_lossy(&repeated.stdout).contains("already persisted"));

    let pool = connect_database(&database_url, DatabasePoolConfig::default())
        .await
        .expect("administrator verification target should accept a PostgreSQL connection");
    let administrator = load_admin_user(
        &pool,
        AdminUserId::from_uuid(
            uuid::Uuid::parse_str(administrator_id).expect("administrator fixture ID should parse"),
        ),
    )
    .await
    .expect("persisted administrator should load")
    .expect("persisted administrator should exist");
    assert_eq!(administrator.username(), "dev-admin");
    assert_eq!(administrator.display_name(), "Development Admin");
    assert!(administrator.is_active());

    let mut conflicting_arguments = seed_arguments;
    conflicting_arguments[5] = "different-admin";
    let conflict = run_cli(&database_url, &conflicting_arguments);
    assert!(!conflict.status.success());
    assert!(String::from_utf8_lossy(&conflict.stderr).contains("already exists"));

    pool.close().await;
}
