use football_domain::{PersonId, PlayerId, TeamId};
use football_infrastructure::{
    DatabasePoolConfig, MIGRATIONS_DIRECTORY, connect_database, load_player_assignment, load_team,
    run_migrations,
};
use std::{path::PathBuf, process::Command};
use uuid::Uuid;

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

fn assert_cli_success(output: &std::process::Output, operation: &str) {
    assert!(
        output.status.success(),
        "{operation} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_cli_failure_contains(
    output: &std::process::Output,
    operation: &str,
    expected_error: &str,
) {
    assert!(
        !output.status.success(),
        "{operation} unexpectedly succeeded: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(expected_error),
        "{operation} did not contain {expected_error:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn seed_player_arguments<'a>(
    person_id: &'a str,
    player_id: &'a str,
    team_id: &'a str,
    name: &'a str,
) -> [&'a str; 10] {
    [
        "directory",
        "seed-player",
        "--person-id",
        person_id,
        "--player-id",
        player_id,
        "--team-id",
        team_id,
        "--name",
        name,
    ]
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn seed_player_persists_idempotently_and_rejects_identity_and_team_conflicts() {
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
        .expect("clean player persistence target should accept a PostgreSQL connection");
    run_migrations(&pool, repository_migrations_path())
        .await
        .expect("all ordered migrations should apply to the clean player persistence target");

    let first_team_id = "019fe2f0-3bc5-7cf0-b3d2-0728c077a930";
    let second_team_id = "019fe2f0-3bc5-7cf1-b3d2-0728c077a930";
    let person_id = "019fe2f0-3bc5-7cf2-b3d2-0728c077a930";
    let player_id = "019fe2f0-3bc5-7cf3-b3d2-0728c077a930";
    let conflicting_person_id = "019fe2f0-3bc5-7cf4-b3d2-0728c077a930";
    let missing_team_player_id = "019fe2f0-3bc5-7cf5-b3d2-0728c077a930";
    let missing_team_id = "019fe2f0-3bc5-7cff-b3d2-0728c077a930";
    let player_name = "Odil Abjumajidov";

    let team_seed = [
        "directory",
        "seed-teams",
        "--first-id",
        first_team_id,
        "--first-name",
        "Andijon",
        "--second-id",
        second_team_id,
        "--second-name",
        "Buxoro",
    ];
    let team_output = run_cli(&database_url, DIRECTORY_AUTHORIZATION_ENV, &team_seed);
    assert_cli_success(&team_output, "explicit team seed");
    assert!(String::from_utf8_lossy(&team_output.stdout).contains(first_team_id));
    assert!(String::from_utf8_lossy(&team_output.stdout).contains(second_team_id));

    let first_team = TeamId::from_uuid(
        Uuid::parse_str(first_team_id).expect("first team fixture ID should parse"),
    );
    let second_team = TeamId::from_uuid(
        Uuid::parse_str(second_team_id).expect("second team fixture ID should parse"),
    );
    let person =
        PersonId::from_uuid(Uuid::parse_str(person_id).expect("person fixture ID should parse"));
    let player =
        PlayerId::from_uuid(Uuid::parse_str(player_id).expect("player fixture ID should parse"));

    let existing_team = load_team(&pool, first_team)
        .await
        .expect("explicit first team lookup should succeed")
        .expect("explicit first team should exist before player seeding");
    assert_eq!(existing_team.name().as_str(), "Andijon");
    assert_eq!(
        load_team(&pool, second_team)
            .await
            .expect("explicit second team lookup should succeed")
            .expect("explicit second team should exist before player seeding")
            .name()
            .as_str(),
        "Buxoro"
    );
    pool.close().await;

    let seed_player = seed_player_arguments(person_id, player_id, first_team_id, player_name);
    let persisted = run_cli(&database_url, DIRECTORY_AUTHORIZATION_ENV, &seed_player);
    assert_cli_success(&persisted, "guarded seed-player persistence");
    assert!(String::from_utf8_lossy(&persisted.stdout).contains(player_id));
    assert!(String::from_utf8_lossy(&persisted.stdout).contains(first_team_id));

    let repeated = run_cli(&database_url, DIRECTORY_AUTHORIZATION_ENV, &seed_player);
    assert_cli_success(&repeated, "exact repeated seed-player persistence");
    assert!(String::from_utf8_lossy(&repeated.stdout).contains(player_id));

    let pool = connect_database(&database_url, DatabasePoolConfig::default())
        .await
        .expect("player persistence verification target should accept a PostgreSQL connection");
    let persisted_after_repeat = load_player_assignment(&pool, player, first_team)
        .await
        .expect("persisted player assignment lookup should succeed")
        .expect("persisted player assignment should exist after exact repetition");
    assert_eq!(persisted_after_repeat.person().id(), person);
    assert_eq!(persisted_after_repeat.person().name().as_str(), player_name);
    assert_eq!(persisted_after_repeat.player().id(), player);
    assert_eq!(persisted_after_repeat.player().person_id(), person);
    assert_eq!(persisted_after_repeat.assignment().team_id(), first_team);
    assert_eq!(persisted_after_repeat.assignment().player_id(), player);
    pool.close().await;

    let conflicting_person =
        seed_player_arguments(person_id, player_id, first_team_id, "Different Source Name");
    let conflict = run_cli(
        &database_url,
        DIRECTORY_AUTHORIZATION_ENV,
        &conflicting_person,
    );
    assert_cli_failure_contains(&conflict, "conflicting person name seed", "different name");

    let conflicting_player =
        seed_player_arguments(conflicting_person_id, player_id, first_team_id, player_name);
    let conflict = run_cli(
        &database_url,
        DIRECTORY_AUTHORIZATION_ENV,
        &conflicting_player,
    );
    assert_cli_failure_contains(
        &conflict,
        "conflicting player ownership seed",
        "different person",
    );

    let conflicting_team = seed_player_arguments(person_id, player_id, second_team_id, player_name);
    let conflict = run_cli(
        &database_url,
        DIRECTORY_AUTHORIZATION_ENV,
        &conflicting_team,
    );
    assert_cli_failure_contains(
        &conflict,
        "conflicting team assignment seed",
        "different team",
    );

    let missing_team = seed_player_arguments(
        person_id,
        missing_team_player_id,
        missing_team_id,
        player_name,
    );
    let rejection = run_cli(&database_url, DIRECTORY_AUTHORIZATION_ENV, &missing_team);
    assert_cli_failure_contains(&rejection, "missing-team seed", "does not exist");
    assert!(String::from_utf8_lossy(&rejection.stderr).contains(missing_team_id));

    let pool = connect_database(&database_url, DatabasePoolConfig::default())
        .await
        .expect(
            "final player persistence verification target should accept a PostgreSQL connection",
        );
    let persisted_after_conflicts = load_player_assignment(&pool, player, first_team)
        .await
        .expect("final persisted player assignment lookup should succeed")
        .expect("original player assignment should remain after rejected seeds");
    assert_eq!(persisted_after_conflicts, persisted_after_repeat);
    assert!(
        load_player_assignment(&pool, player, second_team)
            .await
            .expect("conflicting team lookup should succeed")
            .is_none(),
        "rejected conflicting team seed must not move the assignment"
    );
    pool.close().await;
}
