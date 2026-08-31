use std::process::{Command, Output};

const ADMIN_SEED_ARGUMENTS: &[&str] = &[
    "admin",
    "seed-development",
    "--id",
    "019fe250-3bc5-7cf0-b3d2-0728c077a930",
    "--username",
    "dev-admin",
    "--display-name",
    "Development Admin",
];

const DIRECTORY_SEED_ARGUMENTS: &[&str] = &[
    "directory",
    "seed-teams",
    "--first-id",
    "019fe250-3bc5-7cf0-b3d2-0728c077a930",
    "--first-name",
    "First Team",
    "--second-id",
    "019fe250-3bc5-7cf1-b3d2-0728c077a930",
    "--second-name",
    "Second Team",
];

const COMPETITION_SEED_ARGUMENTS: &[&str] = &[
    "competition",
    "seed",
    "--id",
    "019fe250-3bc5-7cf0-b3d2-0728c077a930",
    "--name",
    "Competition",
];

const SCHEDULING_ARGUMENTS: &[&str] = &[
    "fixture",
    "schedule",
    "--id",
    "019fe250-3bc5-7cf0-b3d2-0728c077a930",
    "--season-id",
    "019fe250-3bc5-7cf1-b3d2-0728c077a930",
    "--home-team-id",
    "019fe250-3bc5-7cf2-b3d2-0728c077a930",
    "--away-team-id",
    "019fe250-3bc5-7cf3-b3d2-0728c077a930",
    "--kickoff",
    "2026-08-15T19:00:00+00:00",
];

const OPERATIONS_ARGUMENTS: &[&str] = &[
    "match",
    "assign",
    "--match-id",
    "019fe250-3bc5-7cf0-b3d2-0728c077a930",
    "--fixture-id",
    "019fe250-3bc5-7cf1-b3d2-0728c077a930",
    "--operator-id",
    "019fe250-3bc5-7cf2-b3d2-0728c077a930",
];

fn run_cli(arguments: &[&str], environment: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_footballctl"));
    command.env_clear();
    for &(name, value) in environment {
        command.env(name, value);
    }

    command
        .args(arguments)
        .output()
        .expect("footballctl test process should start")
}

fn assert_exact_failure(output: &Output, expected_message: &str) {
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!("footballctl failed: {expected_message}\n")
    );
}

#[test]
fn development_authorization_boundaries_preserve_exact_messages() {
    let cases = [
        (
            ADMIN_SEED_ARGUMENTS,
            "set ALLOW_DEVELOPMENT_ADMIN_SEED=1 to authorize development-admin persistence",
        ),
        (
            DIRECTORY_SEED_ARGUMENTS,
            "set ALLOW_DEVELOPMENT_DIRECTORY_SEED=1 to authorize directory persistence",
        ),
        (
            COMPETITION_SEED_ARGUMENTS,
            "set ALLOW_DEVELOPMENT_COMPETITION_SEED=1 to authorize competition and season persistence",
        ),
        (
            SCHEDULING_ARGUMENTS,
            "set ALLOW_DEVELOPMENT_SCHEDULING_SEED=1 to authorize fixture scheduling",
        ),
        (
            OPERATIONS_ARGUMENTS,
            "set ALLOW_DEVELOPMENT_OPERATIONS_SEED=1 to authorize match assignment and claim persistence",
        ),
    ];

    for (arguments, expected_message) in cases {
        let output = run_cli(arguments, &[]);
        assert_exact_failure(&output, expected_message);
    }
}

#[test]
fn missing_database_url_is_reported_before_database_connection() {
    let output = run_cli(
        ADMIN_SEED_ARGUMENTS,
        &[("ALLOW_DEVELOPMENT_ADMIN_SEED", "1")],
    );

    assert_exact_failure(
        &output,
        "required environment variable DATABASE_URL is missing",
    );
}

#[test]
fn empty_database_url_is_reported_before_database_connection() {
    let output = run_cli(
        ADMIN_SEED_ARGUMENTS,
        &[("ALLOW_DEVELOPMENT_ADMIN_SEED", "1"), ("DATABASE_URL", "")],
    );

    assert_exact_failure(
        &output,
        "required environment variable DATABASE_URL is empty",
    );
}

#[test]
fn invalid_optional_bind_address_is_reported_before_database_connection() {
    let output = run_cli(
        ADMIN_SEED_ARGUMENTS,
        &[
            ("ALLOW_DEVELOPMENT_ADMIN_SEED", "1"),
            ("DATABASE_URL", "postgres://app@127.0.0.1/football"),
            ("ADMIN_API_BIND_ADDR", "not-a-socket"),
        ],
    );

    assert_exact_failure(
        &output,
        "environment variable ADMIN_API_BIND_ADDR is not a valid socket address",
    );
}
