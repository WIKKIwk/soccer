use football_domain::{PlayerId, TeamId};
use football_infrastructure::{
    DatabasePoolConfig, MIGRATIONS_DIRECTORY, connect_database, load_player_assignment, load_team,
    run_migrations,
};
use std::{path::PathBuf, process::Command};

const DIRECTORY_AUTHORIZATION_ENV: &str = "ALLOW_DEVELOPMENT_DIRECTORY_SEED";

fn repository_migrations_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(MIGRATIONS_DIRECTORY)
}

fn run_cli(
    database_url: &str,
    authorization_env: &str,
    arguments: &[&str],
) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_footballctl"))
        .env("DATABASE_URL", database_url)
        .env(authorization_env, "1")
        .args(arguments)
        .output()
        .expect("footballctl test process should start")
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn directory_import_commands_persist_idempotently_and_reject_conflicts() {
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
        .expect("clean directory import target should accept a PostgreSQL connection");
    run_migrations(&pool, repository_migrations_path())
        .await
        .expect("all migrations should apply to the clean directory import target");
    pool.close().await;

    let first_team_id = "019fe270-3bc5-7cf0-b3d2-0728c077a930";
    let second_team_id = "019fe270-3bc5-7cf1-b3d2-0728c077a930";
    let team_import = [
        "directory",
        "import-teams",
        "--first-id",
        first_team_id,
        "--first-name",
        "Toshkent City",
        "--second-id",
        second_team_id,
        "--second-name",
        "Samarqand United",
    ];
    let team_output = run_cli(&database_url, DIRECTORY_AUTHORIZATION_ENV, &team_import);
    assert!(
        team_output.status.success(),
        "team import failed: {}",
        String::from_utf8_lossy(&team_output.stderr)
    );
    assert!(String::from_utf8_lossy(&team_output.stdout).contains(first_team_id));
    assert!(String::from_utf8_lossy(&team_output.stdout).contains(second_team_id));

    let person_id = "019fe270-3bc5-7cf2-b3d2-0728c077a930";
    let player_id = "019fe270-3bc5-7cf3-b3d2-0728c077a930";
    let player_import = [
        "directory",
        "import-player",
        "--person-id",
        person_id,
        "--player-id",
        player_id,
        "--team-id",
        first_team_id,
        "--name",
        "Otabek Shukurov",
    ];
    let player_output = run_cli(&database_url, DIRECTORY_AUTHORIZATION_ENV, &player_import);
    assert!(
        player_output.status.success(),
        "player import failed: {}",
        String::from_utf8_lossy(&player_output.stderr)
    );
    assert!(String::from_utf8_lossy(&player_output.stdout).contains(player_id));

    let repeated_team_output = run_cli(&database_url, DIRECTORY_AUTHORIZATION_ENV, &team_import);
    assert!(repeated_team_output.status.success());
    let repeated_player_output =
        run_cli(&database_url, DIRECTORY_AUTHORIZATION_ENV, &player_import);
    assert!(repeated_player_output.status.success());

    let pool = connect_database(&database_url, DatabasePoolConfig::default())
        .await
        .expect("directory import verification target should accept a PostgreSQL connection");
    let first_team = load_team(
        &pool,
        TeamId::from_uuid(
            uuid::Uuid::parse_str(first_team_id).expect("first team fixture ID should parse"),
        ),
    )
    .await
    .expect("imported first team should load")
    .expect("imported first team should exist");
    assert_eq!(first_team.name().as_str(), "Toshkent City");

    let player = load_player_assignment(
        &pool,
        PlayerId::from_uuid(
            uuid::Uuid::parse_str(player_id).expect("player fixture ID should parse"),
        ),
        first_team.id(),
    )
    .await
    .expect("imported player assignment should load")
    .expect("imported player assignment should exist");
    assert_eq!(player.person().name().as_str(), "Otabek Shukurov");

    let mut conflicting_team_import = team_import;
    conflicting_team_import[5] = "Renamed Toshkent City";
    let conflict_output = run_cli(
        &database_url,
        DIRECTORY_AUTHORIZATION_ENV,
        &conflicting_team_import,
    );
    assert!(!conflict_output.status.success());
    assert!(String::from_utf8_lossy(&conflict_output.stderr).contains("different name"));

    let mut unknown_team_player_import = player_import;
    unknown_team_player_import[7] = "019fe270-3bc5-7cff-b3d2-0728c077a930";
    let unknown_team_output = run_cli(
        &database_url,
        DIRECTORY_AUTHORIZATION_ENV,
        &unknown_team_player_import,
    );
    assert!(!unknown_team_output.status.success());
    assert!(String::from_utf8_lossy(&unknown_team_output.stderr).contains("does not exist"));

    pool.close().await;
}
