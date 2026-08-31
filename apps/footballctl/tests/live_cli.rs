use std::process::{Command, Output};

const FINISH_ARGUMENTS: &[&str] = &[
    "match",
    "finish",
    "--match-id",
    "019ff800-3bc5-7cf0-b3d2-0728c077a930",
    "--operator-id",
    "019ff800-3bc5-7cf1-b3d2-0728c077a930",
    "--lease-id",
    "019ff800-3bc5-7cf2-b3d2-0728c077a930",
    "--command-id",
    "019ff800-3bc5-7cf3-b3d2-0728c077a930",
    "--client-command-id",
    "019ff800-3bc5-7cf4-b3d2-0728c077a930",
    "--event-id",
    "019ff800-3bc5-7cf5-b3d2-0728c077a930",
    "--expected-version",
    "9",
    "--clock-ms",
    "5400000",
    "--occurred-at",
    "2026-08-31T19:30:00+05:00",
];

const LINEUP_ARGUMENTS: &[&str] = &[
    "lineup",
    "save",
    "--lineup-id",
    "019ff800-3bc5-7cf0-b3d2-0728c077a930",
    "--fixture-id",
    "019ff800-3bc5-7cf1-b3d2-0728c077a930",
    "--team-id",
    "019ff800-3bc5-7cf2-b3d2-0728c077a930",
    "--starter",
    "019ff800-3bc5-7cf3-b3d2-0728c077a930",
    "--substitute",
    "019ff800-3bc5-7cf4-b3d2-0728c077a930",
];

fn run(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_footballctl"))
        .env_clear()
        .args(arguments)
        .output()
        .expect("footballctl test process should start")
}

fn assert_exact_failure(output: Output, message: &str) {
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!("footballctl failed: {message}\n")
    );
}

#[test]
fn live_match_commands_route_through_the_operations_boundary() {
    assert_exact_failure(
        run(FINISH_ARGUMENTS),
        "set ALLOW_DEVELOPMENT_OPERATIONS_SEED=1 to authorize match assignment and claim persistence",
    );
}

#[test]
fn lineup_save_routes_through_the_scheduling_boundary() {
    assert_exact_failure(
        run(LINEUP_ARGUMENTS),
        "set ALLOW_DEVELOPMENT_SCHEDULING_SEED=1 to authorize fixture scheduling",
    );
}
