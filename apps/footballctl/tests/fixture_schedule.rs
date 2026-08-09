use football_domain::{FixtureId, TeamId};
use football_infrastructure::{
    DatabasePoolConfig, MIGRATIONS_DIRECTORY, connect_database, load_fixture, run_migrations,
};
use std::{path::PathBuf, process::Command};
use time::{OffsetDateTime, UtcOffset};

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

fn assert_cli_success(output: &std::process::Output, operation: &str) {
    assert!(
        output.status.success(),
        "{operation} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn fixture_schedule_command_persists_utc_and_rejects_conflicts() {
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
        .expect("clean fixture schedule target should accept a PostgreSQL connection");
    run_migrations(&pool, repository_migrations_path())
        .await
        .expect("all migrations should apply to the clean fixture schedule target");
    pool.close().await;

    let competition_id = "019fe280-3bc5-7cf0-b3d2-0728c077a930";
    let season_id = "019fe280-3bc5-7cf1-b3d2-0728c077a930";
    let first_team_id = "019fe280-3bc5-7cf2-b3d2-0728c077a930";
    let second_team_id = "019fe280-3bc5-7cf3-b3d2-0728c077a930";
    let fixture_id = "019fe280-3bc5-7cf4-b3d2-0728c077a930";

    assert_cli_success(
        &run_cli(
            &database_url,
            "ALLOW_DEVELOPMENT_COMPETITION_SEED",
            &[
                "competition",
                "seed",
                "--id",
                competition_id,
                "--name",
                "O‘zbekiston Superligasi",
            ],
        ),
        "competition setup",
    );
    assert_cli_success(
        &run_cli(
            &database_url,
            "ALLOW_DEVELOPMENT_COMPETITION_SEED",
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
        ),
        "season setup",
    );
    assert_cli_success(
        &run_cli(
            &database_url,
            "ALLOW_DEVELOPMENT_DIRECTORY_SEED",
            &[
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
            ],
        ),
        "team setup",
    );

    let fixture_schedule = [
        "fixture",
        "schedule",
        "--id",
        fixture_id,
        "--season-id",
        season_id,
        "--home-team-id",
        first_team_id,
        "--away-team-id",
        second_team_id,
        "--kickoff",
        "2026-08-15T19:00:00+05:00",
    ];
    let scheduled = run_cli(
        &database_url,
        "ALLOW_DEVELOPMENT_SCHEDULING_SEED",
        &fixture_schedule,
    );
    assert_cli_success(&scheduled, "fixture scheduling");
    assert!(String::from_utf8_lossy(&scheduled.stdout).contains(fixture_id));

    let repeated = run_cli(
        &database_url,
        "ALLOW_DEVELOPMENT_SCHEDULING_SEED",
        &fixture_schedule,
    );
    assert_cli_success(&repeated, "repeated fixture scheduling");

    let pool = connect_database(&database_url, DatabasePoolConfig::default())
        .await
        .expect("fixture verification target should accept a PostgreSQL connection");
    let fixture = load_fixture(
        &pool,
        FixtureId::from_uuid(uuid::Uuid::parse_str(fixture_id).expect("fixture ID should parse")),
    )
    .await
    .expect("scheduled fixture should load")
    .expect("scheduled fixture should exist");
    let expected_kickoff = OffsetDateTime::parse(
        "2026-08-15T19:00:00+05:00",
        &time::format_description::well_known::Rfc3339,
    )
    .expect("fixture source kickoff should parse")
    .to_offset(UtcOffset::UTC);
    assert_eq!(fixture.home_team_id().to_string(), first_team_id);
    assert_eq!(fixture.away_team_id().to_string(), second_team_id);
    assert_eq!(fixture.kickoff().as_offset_datetime(), expected_kickoff);

    let mut same_team = fixture_schedule;
    same_team[9] = first_team_id;
    let same_team_output = run_cli(
        &database_url,
        "ALLOW_DEVELOPMENT_SCHEDULING_SEED",
        &same_team,
    );
    assert!(!same_team_output.status.success());
    assert!(String::from_utf8_lossy(&same_team_output.stderr).contains("must be different"));

    let mut changed_fixture = fixture_schedule;
    changed_fixture[11] = "2026-08-15T20:00:00+05:00";
    let changed_fixture_output = run_cli(
        &database_url,
        "ALLOW_DEVELOPMENT_SCHEDULING_SEED",
        &changed_fixture,
    );
    assert!(!changed_fixture_output.status.success());
    assert!(String::from_utf8_lossy(&changed_fixture_output.stderr).contains("different data"));

    let mut unknown_team = fixture_schedule;
    unknown_team[9] = "019fe280-3bc5-7cff-b3d2-0728c077a930";
    let unknown_team_output = run_cli(
        &database_url,
        "ALLOW_DEVELOPMENT_SCHEDULING_SEED",
        &unknown_team,
    );
    assert!(!unknown_team_output.status.success());
    assert!(String::from_utf8_lossy(&unknown_team_output.stderr).contains("does not exist"));

    let unknown_team_id = TeamId::from_uuid(
        uuid::Uuid::parse_str("019fe280-3bc5-7cff-b3d2-0728c077a930")
            .expect("unknown team fixture ID should parse"),
    );
    assert_ne!(fixture.home_team_id(), unknown_team_id);
    pool.close().await;
}
