use football_domain::{CompetitionId, SeasonId};
use football_infrastructure::{
    DatabasePoolConfig, MIGRATIONS_DIRECTORY, connect_database, load_competition, load_season,
    run_migrations,
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
        .env("ALLOW_DEVELOPMENT_COMPETITION_SEED", "1")
        .args(arguments)
        .output()
        .expect("footballctl test process should start")
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn competition_and_season_commands_persist_idempotently_and_preserve_ownership() {
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
        .expect("clean CLI target should accept a PostgreSQL connection");
    run_migrations(&pool, repository_migrations_path())
        .await
        .expect("all migrations should apply to the clean CLI target");
    pool.close().await;

    let competition_id = "019fe260-3bc5-7cf0-b3d2-0728c077a930";
    let season_id = "019fe260-3bc5-7cf1-b3d2-0728c077a930";
    let competition_output = run_cli(
        &database_url,
        &[
            "competition",
            "seed",
            "--id",
            competition_id,
            "--name",
            "O‘zbekiston Superligasi",
        ],
    );
    assert!(
        competition_output.status.success(),
        "competition command failed: {}",
        String::from_utf8_lossy(&competition_output.stderr)
    );
    assert!(String::from_utf8_lossy(&competition_output.stdout).contains(competition_id));

    let season_output = run_cli(
        &database_url,
        &[
            "season",
            "seed",
            "--id",
            season_id,
            "--competition-id",
            competition_id,
            "--year",
            "2026",
        ],
    );
    assert!(
        season_output.status.success(),
        "season command failed: {}",
        String::from_utf8_lossy(&season_output.stderr)
    );
    assert!(String::from_utf8_lossy(&season_output.stdout).contains(season_id));

    let repeated_competition_output = run_cli(
        &database_url,
        &[
            "competition",
            "seed",
            "--id",
            competition_id,
            "--name",
            "O‘zbekiston Superligasi",
        ],
    );
    assert!(repeated_competition_output.status.success());
    let repeated_season_output = run_cli(
        &database_url,
        &[
            "season",
            "seed",
            "--id",
            season_id,
            "--competition-id",
            competition_id,
            "--year",
            "2026",
        ],
    );
    assert!(repeated_season_output.status.success());

    let pool = connect_database(&database_url, DatabasePoolConfig::default())
        .await
        .expect("CLI verification target should accept a PostgreSQL connection");
    let competition = load_competition(
        &pool,
        CompetitionId::from_uuid(
            uuid::Uuid::parse_str(competition_id).expect("competition fixture ID should parse"),
        ),
    )
    .await
    .expect("persisted competition should load")
    .expect("persisted competition should exist");
    assert_eq!(competition.name().as_str(), "O‘zbekiston Superligasi");

    let season = load_season(
        &pool,
        SeasonId::from_uuid(
            uuid::Uuid::parse_str(season_id).expect("season fixture ID should parse"),
        ),
    )
    .await
    .expect("persisted season should load")
    .expect("persisted season should exist");
    assert_eq!(season.competition_id(), competition.id());
    assert_eq!(season.year().value(), 2026);

    let missing_competition_output = run_cli(
        &database_url,
        &[
            "season",
            "seed",
            "--id",
            "019fe260-3bc5-7cf2-b3d2-0728c077a930",
            "--competition-id",
            "019fe260-3bc5-7cff-b3d2-0728c077a930",
            "--year",
            "2026",
        ],
    );
    assert!(!missing_competition_output.status.success());
    assert!(String::from_utf8_lossy(&missing_competition_output.stderr).contains("does not exist"));

    pool.close().await;
}
