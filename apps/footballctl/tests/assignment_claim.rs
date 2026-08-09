use football_domain::{AdminUserId, FixtureId, MatchId, OperatorLeaseId};
use football_infrastructure::{
    DatabasePoolConfig, MIGRATIONS_DIRECTORY, connect_database, load_match_assignment,
    load_operator_lease, run_migrations,
};
use std::{path::PathBuf, process::Command};

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
async fn assignment_claim_release_and_takeover_commands_preserve_lease_ownership() {
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
        .expect("clean assignment target should accept a PostgreSQL connection");
    run_migrations(&pool, repository_migrations_path())
        .await
        .expect("all migrations should apply to the clean assignment target");
    pool.close().await;

    let operator_id = "019fe2b0-3bc5-7cf0-b3d2-0728c077a930";
    let second_operator_id = "019fe2b0-3bc5-7cf1-b3d2-0728c077a930";
    let competition_id = "019fe2b0-3bc5-7cf2-b3d2-0728c077a930";
    let season_id = "019fe2b0-3bc5-7cf3-b3d2-0728c077a930";
    let first_team_id = "019fe2b0-3bc5-7cf4-b3d2-0728c077a930";
    let second_team_id = "019fe2b0-3bc5-7cf5-b3d2-0728c077a930";
    let fixture_id = "019fe2b0-3bc5-7cf6-b3d2-0728c077a930";
    let match_id = "019fe2b0-3bc5-7cf7-b3d2-0728c077a930";
    let conflicting_match_id = "019fe2b0-3bc5-7cf8-b3d2-0728c077a930";
    let lease_id = "019fe2b0-3bc5-7cf9-b3d2-0728c077a930";
    let conflicting_lease_id = "019fe2b0-3bc5-7cfa-b3d2-0728c077a930";
    let wrong_operator_lease_id = "019fe2b0-3bc5-7cfb-b3d2-0728c077a930";
    let takeover_lease_id = "019fe2b0-3bc5-7cfc-b3d2-0728c077a930";
    let final_takeover_lease_id = "019fe2b0-3bc5-7cfd-b3d2-0728c077a930";

    for (id, username, display_name) in [
        (operator_id, "operator-one", "Operator One"),
        (second_operator_id, "operator-two", "Operator Two"),
    ] {
        let output = run_cli(
            &database_url,
            "ALLOW_DEVELOPMENT_ADMIN_SEED",
            &[
                "admin",
                "seed-development",
                "--id",
                id,
                "--username",
                username,
                "--display-name",
                display_name,
            ],
        );
        assert_cli_success(&output, "operator setup");
    }

    let output = run_cli(
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
    );
    assert_cli_success(&output, "competition setup");

    let output = run_cli(
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
    );
    assert_cli_success(&output, "season setup");

    let output = run_cli(
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
    );
    assert_cli_success(&output, "team setup");

    let output = run_cli(
        &database_url,
        "ALLOW_DEVELOPMENT_SCHEDULING_SEED",
        &[
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
        ],
    );
    assert_cli_success(&output, "fixture setup");

    let assignment = [
        "match",
        "assign",
        "--match-id",
        match_id,
        "--fixture-id",
        fixture_id,
        "--operator-id",
        operator_id,
    ];
    let output = run_cli(
        &database_url,
        "ALLOW_DEVELOPMENT_OPERATIONS_SEED",
        &assignment,
    );
    assert_cli_success(&output, "match assignment");
    assert!(String::from_utf8_lossy(&output.stdout).contains(match_id));

    let repeated_assignment = run_cli(
        &database_url,
        "ALLOW_DEVELOPMENT_OPERATIONS_SEED",
        &assignment,
    );
    assert_cli_success(&repeated_assignment, "repeated match assignment");

    let conflicting_assignment = [
        "match",
        "assign",
        "--match-id",
        conflicting_match_id,
        "--fixture-id",
        fixture_id,
        "--operator-id",
        operator_id,
    ];
    let conflicting_assignment_output = run_cli(
        &database_url,
        "ALLOW_DEVELOPMENT_OPERATIONS_SEED",
        &conflicting_assignment,
    );
    assert!(!conflicting_assignment_output.status.success());
    assert!(
        String::from_utf8_lossy(&conflicting_assignment_output.stderr)
            .contains("already assigned to match")
    );

    let claim = [
        "match",
        "claim",
        "--match-id",
        match_id,
        "--operator-id",
        operator_id,
        "--lease-id",
        lease_id,
        "--lease-seconds",
        "60",
    ];
    let output = run_cli(&database_url, "ALLOW_DEVELOPMENT_OPERATIONS_SEED", &claim);
    assert_cli_success(&output, "operator lease claim");
    assert!(String::from_utf8_lossy(&output.stdout).contains(lease_id));

    let repeated_claim = run_cli(&database_url, "ALLOW_DEVELOPMENT_OPERATIONS_SEED", &claim);
    assert_cli_success(&repeated_claim, "repeated operator lease claim");

    let pool = connect_database(&database_url, DatabasePoolConfig::default())
        .await
        .expect("heartbeat verification target should accept a PostgreSQL connection");
    let match_identity =
        MatchId::from_uuid(uuid::Uuid::parse_str(match_id).expect("match ID should parse"));
    let lease_identity =
        OperatorLeaseId::from_uuid(uuid::Uuid::parse_str(lease_id).expect("lease ID should parse"));
    let initial_lease = load_operator_lease(&pool, match_identity)
        .await
        .expect("claimed lease should load before heartbeat")
        .expect("claimed lease should exist before heartbeat");
    assert_eq!(initial_lease.id(), lease_identity);
    pool.close().await;

    let heartbeat = [
        "match",
        "heartbeat",
        "--match-id",
        match_id,
        "--operator-id",
        operator_id,
        "--lease-id",
        lease_id,
        "--lease-seconds",
        "120",
    ];
    let heartbeat_output = run_cli(
        &database_url,
        "ALLOW_DEVELOPMENT_OPERATIONS_SEED",
        &heartbeat,
    );
    assert_cli_success(&heartbeat_output, "operator lease heartbeat");
    assert!(String::from_utf8_lossy(&heartbeat_output.stdout).contains(lease_id));

    let pool = connect_database(&database_url, DatabasePoolConfig::default())
        .await
        .expect("renewed lease verification target should accept a PostgreSQL connection");
    let renewed_lease = load_operator_lease(&pool, match_identity)
        .await
        .expect("renewed lease should load")
        .expect("renewed lease should exist");
    assert_eq!(renewed_lease.id(), initial_lease.id());
    assert_eq!(renewed_lease.claimed_at(), initial_lease.claimed_at());
    assert!(renewed_lease.expires_at() > initial_lease.expires_at());
    pool.close().await;

    let wrong_lease_heartbeat = [
        "match",
        "heartbeat",
        "--match-id",
        match_id,
        "--operator-id",
        operator_id,
        "--lease-id",
        conflicting_lease_id,
        "--lease-seconds",
        "120",
    ];
    let wrong_lease_heartbeat_output = run_cli(
        &database_url,
        "ALLOW_DEVELOPMENT_OPERATIONS_SEED",
        &wrong_lease_heartbeat,
    );
    assert!(!wrong_lease_heartbeat_output.status.success());
    assert!(
        String::from_utf8_lossy(&wrong_lease_heartbeat_output.stderr).contains("cannot use lease")
    );

    let conflicting_claim = [
        "match",
        "claim",
        "--match-id",
        match_id,
        "--operator-id",
        operator_id,
        "--lease-id",
        conflicting_lease_id,
        "--lease-seconds",
        "60",
    ];
    let conflicting_claim_output = run_cli(
        &database_url,
        "ALLOW_DEVELOPMENT_OPERATIONS_SEED",
        &conflicting_claim,
    );
    assert!(!conflicting_claim_output.status.success());
    assert!(String::from_utf8_lossy(&conflicting_claim_output.stderr).contains("has active lease"));

    let wrong_operator_claim = [
        "match",
        "claim",
        "--match-id",
        match_id,
        "--operator-id",
        second_operator_id,
        "--lease-id",
        wrong_operator_lease_id,
        "--lease-seconds",
        "60",
    ];
    let wrong_operator_output = run_cli(
        &database_url,
        "ALLOW_DEVELOPMENT_OPERATIONS_SEED",
        &wrong_operator_claim,
    );
    assert!(!wrong_operator_output.status.success());
    assert!(String::from_utf8_lossy(&wrong_operator_output.stderr).contains("is not assigned"));

    let release = [
        "match",
        "release",
        "--match-id",
        match_id,
        "--operator-id",
        operator_id,
        "--lease-id",
        lease_id,
    ];
    let release_output = run_cli(&database_url, "ALLOW_DEVELOPMENT_OPERATIONS_SEED", &release);
    assert_cli_success(&release_output, "operator lease release");
    assert!(String::from_utf8_lossy(&release_output.stdout).contains(lease_id));

    let repeated_release = run_cli(&database_url, "ALLOW_DEVELOPMENT_OPERATIONS_SEED", &release);
    assert_cli_success(&repeated_release, "repeated operator lease release");

    let pool = connect_database(&database_url, DatabasePoolConfig::default())
        .await
        .expect("release verification target should accept a PostgreSQL connection");
    assert!(
        load_operator_lease(
            &pool,
            MatchId::from_uuid(uuid::Uuid::parse_str(match_id).expect("match ID should parse")),
        )
        .await
        .expect("released lease lookup should succeed")
        .is_none()
    );
    pool.close().await;

    let takeover = [
        "match",
        "takeover",
        "--match-id",
        match_id,
        "--operator-id",
        second_operator_id,
        "--lease-id",
        takeover_lease_id,
        "--lease-seconds",
        "1",
    ];
    let takeover_output = run_cli(
        &database_url,
        "ALLOW_DEVELOPMENT_OPERATIONS_SEED",
        &takeover,
    );
    assert_cli_success(&takeover_output, "operator lease takeover after release");
    assert!(String::from_utf8_lossy(&takeover_output.stdout).contains(takeover_lease_id));

    let active_takeover = [
        "match",
        "takeover",
        "--match-id",
        match_id,
        "--operator-id",
        operator_id,
        "--lease-id",
        final_takeover_lease_id,
        "--lease-seconds",
        "60",
    ];
    let active_takeover_output = run_cli(
        &database_url,
        "ALLOW_DEVELOPMENT_OPERATIONS_SEED",
        &active_takeover,
    );
    assert!(!active_takeover_output.status.success());
    assert!(String::from_utf8_lossy(&active_takeover_output.stderr).contains("has active lease"));

    std::thread::sleep(std::time::Duration::from_secs(2));
    let final_takeover = [
        "match",
        "takeover",
        "--match-id",
        match_id,
        "--operator-id",
        operator_id,
        "--lease-id",
        final_takeover_lease_id,
        "--lease-seconds",
        "60",
    ];
    let final_takeover_output = run_cli(
        &database_url,
        "ALLOW_DEVELOPMENT_OPERATIONS_SEED",
        &final_takeover,
    );
    assert_cli_success(
        &final_takeover_output,
        "operator lease takeover after expiry",
    );
    assert!(
        String::from_utf8_lossy(&final_takeover_output.stdout).contains(final_takeover_lease_id)
    );

    let pool = connect_database(&database_url, DatabasePoolConfig::default())
        .await
        .expect("assignment verification target should accept a PostgreSQL connection");
    let match_id =
        MatchId::from_uuid(uuid::Uuid::parse_str(match_id).expect("match ID should parse"));
    let fixture_id =
        FixtureId::from_uuid(uuid::Uuid::parse_str(fixture_id).expect("fixture ID should parse"));
    let operator_id = AdminUserId::from_uuid(
        uuid::Uuid::parse_str(operator_id).expect("operator ID should parse"),
    );
    let lease_id = OperatorLeaseId::from_uuid(
        uuid::Uuid::parse_str(final_takeover_lease_id).expect("final lease ID should parse"),
    );

    let persisted_assignment = load_match_assignment(&pool, match_id)
        .await
        .expect("match assignment should load")
        .expect("match assignment should exist");
    assert_eq!(persisted_assignment.match_id(), match_id);
    assert_eq!(persisted_assignment.fixture_id(), fixture_id);
    assert_eq!(persisted_assignment.operator_id(), operator_id);

    let persisted_lease = load_operator_lease(&pool, match_id)
        .await
        .expect("operator lease should load")
        .expect("operator lease should exist");
    assert_eq!(persisted_lease.id(), lease_id);
    assert_eq!(persisted_lease.match_id(), match_id);
    assert_eq!(persisted_lease.operator_id(), operator_id);
    assert!(persisted_lease.expires_at() > persisted_lease.claimed_at());
    pool.close().await;
}
