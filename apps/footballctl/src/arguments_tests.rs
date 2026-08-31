use super::{
    parse_competition_arguments, parse_fixture_schedule_arguments,
    parse_match_assignment_arguments, parse_match_claim_arguments, parse_match_heartbeat_arguments,
    parse_match_release_arguments, parse_match_takeover_arguments, parse_player_arguments,
    parse_season_arguments, parse_seed_arguments, parse_team_arguments,
};
use crate::error::CliError;
use football_domain::{OperatorLeaseExpiryPolicyError, PersonNameError, TeamNameError};

fn valid_arguments() -> Vec<String> {
    vec![
        "admin".to_owned(),
        "seed-development".to_owned(),
        "--id".to_owned(),
        "019fe250-3bc5-7cf0-b3d2-0728c077a930".to_owned(),
        "--username".to_owned(),
        "dev-admin".to_owned(),
        "--display-name".to_owned(),
        "Development Admin".to_owned(),
    ]
}

#[test]
fn parses_explicit_uuidv7_seed_arguments() {
    let parsed = parse_seed_arguments(valid_arguments()).expect("valid seed arguments");
    assert_eq!(parsed.administrator.username(), "dev-admin");
    assert_eq!(parsed.administrator.display_name(), "Development Admin");
}

#[test]
fn rejects_non_uuidv7_seed_id() {
    let mut arguments = valid_arguments();
    arguments[3] = "550e8400-e29b-41d4-a716-446655440000".to_owned();
    assert!(matches!(
        parse_seed_arguments(arguments),
        Err(CliError::UuidMustBeV7)
    ));
}

#[test]
fn rejects_missing_seed_arguments() {
    assert!(matches!(
        parse_seed_arguments(["admin".to_owned()]),
        Err(CliError::Usage)
    ));
}

fn valid_team_arguments() -> Vec<String> {
    vec![
        "directory".to_owned(),
        "seed-teams".to_owned(),
        "--first-id".to_owned(),
        "019fe250-3bc5-7cf0-b3d2-0728c077a930".to_owned(),
        "--first-name".to_owned(),
        "First Team".to_owned(),
        "--second-id".to_owned(),
        "019fe250-3bc5-7cf1-b3d2-0728c077a930".to_owned(),
        "--second-name".to_owned(),
        "Second Team".to_owned(),
    ]
}

#[test]
fn parses_two_explicit_team_arguments() {
    let parsed = parse_team_arguments(valid_team_arguments()).expect("valid team arguments");
    assert_eq!(parsed.first.name().as_str(), "First Team");
    assert_eq!(parsed.second.name().as_str(), "Second Team");
    assert_ne!(parsed.first.id(), parsed.second.id());
}

#[test]
fn parses_team_import_arguments_with_the_existing_persistence_shape() {
    let mut arguments = valid_team_arguments();
    arguments[1] = "import-teams".to_owned();
    let parsed = parse_team_arguments(arguments).expect("valid team import arguments");
    assert_eq!(parsed.first.name().as_str(), "First Team");
    assert_eq!(parsed.second.name().as_str(), "Second Team");
}

#[test]
fn rejects_duplicate_team_ids() {
    let mut arguments = valid_team_arguments();
    arguments[7] = arguments[3].clone();
    assert!(matches!(
        parse_team_arguments(arguments),
        Err(CliError::SameTeamId)
    ));
}

#[test]
fn rejects_blank_team_names() {
    let mut arguments = valid_team_arguments();
    arguments[5] = " ".to_owned();
    assert!(matches!(
        parse_team_arguments(arguments),
        Err(CliError::InvalidTeamName(TeamNameError::Empty))
    ));
}

fn valid_player_arguments() -> Vec<String> {
    vec![
        "directory".to_owned(),
        "seed-player".to_owned(),
        "--person-id".to_owned(),
        "019fe250-3bc5-7cf0-b3d2-0728c077a930".to_owned(),
        "--player-id".to_owned(),
        "019fe250-3bc5-7cf1-b3d2-0728c077a930".to_owned(),
        "--team-id".to_owned(),
        "019fe250-3bc5-7cf2-b3d2-0728c077a930".to_owned(),
        "--name".to_owned(),
        "Otabek Shukurov".to_owned(),
    ]
}

#[test]
fn parses_explicit_player_assignment_arguments() {
    let parsed = parse_player_arguments(valid_player_arguments()).expect("valid player arguments");
    assert_eq!(parsed.person.name().as_str(), "Otabek Shukurov");
    assert_eq!(parsed.player.person_id(), parsed.person.id());
    assert_eq!(
        parsed.team_id.to_string(),
        "019fe250-3bc5-7cf2-b3d2-0728c077a930"
    );
}

#[test]
fn parses_player_import_arguments_with_the_existing_persistence_shape() {
    let mut arguments = valid_player_arguments();
    arguments[1] = "import-player".to_owned();
    let parsed = parse_player_arguments(arguments).expect("valid player import arguments");
    assert_eq!(parsed.person.name().as_str(), "Otabek Shukurov");
    assert_eq!(parsed.player.person_id(), parsed.person.id());
}

#[test]
fn rejects_blank_player_person_name() {
    let mut arguments = valid_player_arguments();
    arguments[9] = " ".to_owned();
    assert!(matches!(
        parse_player_arguments(arguments),
        Err(CliError::InvalidPersonName(PersonNameError::Empty))
    ));
}

fn valid_competition_arguments() -> Vec<String> {
    vec![
        "competition".to_owned(),
        "seed".to_owned(),
        "--id".to_owned(),
        "019fe250-3bc5-7cf0-b3d2-0728c077a930".to_owned(),
        "--name".to_owned(),
        "O‘zbekiston Superligasi".to_owned(),
    ]
}

#[test]
fn parses_competition_seed_arguments() {
    let parsed = parse_competition_arguments(valid_competition_arguments())
        .expect("valid competition arguments");
    assert_eq!(
        parsed.competition.name().as_str(),
        "O‘zbekiston Superligasi"
    );
    assert_eq!(
        parsed.competition.id().to_string(),
        "019fe250-3bc5-7cf0-b3d2-0728c077a930"
    );
}

#[test]
fn rejects_blank_competition_name() {
    let mut arguments = valid_competition_arguments();
    arguments[5] = " ".to_owned();
    assert!(matches!(
        parse_competition_arguments(arguments),
        Err(CliError::InvalidCompetitionName(
            football_domain::CompetitionNameError::Empty
        ))
    ));
}

fn valid_season_arguments() -> Vec<String> {
    vec![
        "season".to_owned(),
        "seed".to_owned(),
        "--id".to_owned(),
        "019fe250-3bc5-7cf1-b3d2-0728c077a930".to_owned(),
        "--competition-id".to_owned(),
        "019fe250-3bc5-7cf0-b3d2-0728c077a930".to_owned(),
        "--year".to_owned(),
        "2026".to_owned(),
    ]
}

#[test]
fn parses_season_seed_arguments_with_competition_ownership() {
    let parsed = parse_season_arguments(valid_season_arguments()).expect("valid season arguments");
    assert_eq!(parsed.season.year().value(), 2026);
    assert_eq!(
        parsed.season.competition_id().to_string(),
        "019fe250-3bc5-7cf0-b3d2-0728c077a930"
    );
}

#[test]
fn rejects_non_numeric_season_year() {
    let mut arguments = valid_season_arguments();
    arguments[7] = "twenty-twenty-six".to_owned();
    assert!(matches!(
        parse_season_arguments(arguments),
        Err(CliError::InvalidSeasonYearValue)
    ));
}

fn valid_fixture_schedule_arguments() -> Vec<String> {
    vec![
        "fixture".to_owned(),
        "schedule".to_owned(),
        "--id".to_owned(),
        "019fe250-3bc5-7cf5-b3d2-0728c077a930".to_owned(),
        "--season-id".to_owned(),
        "019fe250-3bc5-7cf1-b3d2-0728c077a930".to_owned(),
        "--home-team-id".to_owned(),
        "019fe250-3bc5-7cf3-b3d2-0728c077a930".to_owned(),
        "--away-team-id".to_owned(),
        "019fe250-3bc5-7cf4-b3d2-0728c077a930".to_owned(),
        "--kickoff".to_owned(),
        "2026-08-15T19:00:00+05:00".to_owned(),
    ]
}

#[test]
fn parses_fixture_schedule_arguments_and_normalizes_kickoff_to_utc() {
    let parsed = parse_fixture_schedule_arguments(valid_fixture_schedule_arguments())
        .expect("valid fixture schedule arguments");
    assert_eq!(
        parsed.id.to_string(),
        "019fe250-3bc5-7cf5-b3d2-0728c077a930"
    );
    assert_eq!(
        parsed.season_id.to_string(),
        "019fe250-3bc5-7cf1-b3d2-0728c077a930"
    );
    assert_eq!(
        parsed.kickoff.as_offset_datetime().offset(),
        time::UtcOffset::UTC
    );
    assert_eq!(
        parsed.kickoff.unix_timestamp(),
        time::OffsetDateTime::parse(
            "2026-08-15T19:00:00+05:00",
            &time::format_description::well_known::Rfc3339,
        )
        .expect("fixture source timestamp should parse")
        .unix_timestamp()
    );
}

#[test]
fn rejects_fixture_kickoff_without_an_explicit_offset() {
    let mut arguments = valid_fixture_schedule_arguments();
    arguments[11] = "2026-08-15T19:00:00".to_owned();
    assert!(matches!(
        parse_fixture_schedule_arguments(arguments),
        Err(CliError::InvalidKickoff(_))
    ));
}

fn valid_match_assignment_arguments() -> Vec<String> {
    vec![
        "match".to_owned(),
        "assign".to_owned(),
        "--match-id".to_owned(),
        "019fe270-3bc5-7cf0-b3d2-0728c077a930".to_owned(),
        "--fixture-id".to_owned(),
        "019fe270-3bc5-7cf1-b3d2-0728c077a930".to_owned(),
        "--operator-id".to_owned(),
        "019fe270-3bc5-7cf2-b3d2-0728c077a930".to_owned(),
    ]
}

#[test]
fn parses_match_assignment_arguments_with_explicit_identities() {
    let parsed = parse_match_assignment_arguments(valid_match_assignment_arguments())
        .expect("valid match assignment arguments");
    assert_eq!(
        parsed.assignment.match_id().to_string(),
        "019fe270-3bc5-7cf0-b3d2-0728c077a930"
    );
    assert_eq!(
        parsed.assignment.fixture_id().to_string(),
        "019fe270-3bc5-7cf1-b3d2-0728c077a930"
    );
    assert_eq!(
        parsed.assignment.operator_id().to_string(),
        "019fe270-3bc5-7cf2-b3d2-0728c077a930"
    );
}

fn valid_match_claim_arguments() -> Vec<String> {
    vec![
        "match".to_owned(),
        "claim".to_owned(),
        "--match-id".to_owned(),
        "019fe270-3bc5-7cf0-b3d2-0728c077a930".to_owned(),
        "--operator-id".to_owned(),
        "019fe270-3bc5-7cf2-b3d2-0728c077a930".to_owned(),
        "--lease-id".to_owned(),
        "019fe270-3bc5-7cf3-b3d2-0728c077a930".to_owned(),
        "--lease-seconds".to_owned(),
        "60".to_owned(),
    ]
}

#[test]
fn parses_match_claim_arguments_with_positive_lease_duration() {
    let parsed =
        parse_match_claim_arguments(valid_match_claim_arguments()).expect("valid claim args");
    assert_eq!(
        parsed.match_id.to_string(),
        "019fe270-3bc5-7cf0-b3d2-0728c077a930"
    );
    assert_eq!(
        parsed.operator_id.to_string(),
        "019fe270-3bc5-7cf2-b3d2-0728c077a930"
    );
    assert_eq!(
        parsed.lease_id.to_string(),
        "019fe270-3bc5-7cf3-b3d2-0728c077a930"
    );
    assert_eq!(parsed.lease_seconds, 60);
}

#[test]
fn rejects_zero_or_non_numeric_lease_duration() {
    let mut zero = valid_match_claim_arguments();
    *zero.last_mut().expect("lease duration should be present") = "0".to_owned();
    assert!(matches!(
        parse_match_claim_arguments(zero),
        Err(CliError::InvalidLeaseExpiryPolicy(
            OperatorLeaseExpiryPolicyError::DurationMustBePositive
        ))
    ));

    let mut non_numeric = valid_match_claim_arguments();
    *non_numeric
        .last_mut()
        .expect("lease duration should be present") = "soon".to_owned();
    assert!(matches!(
        parse_match_claim_arguments(non_numeric),
        Err(CliError::InvalidLeaseSeconds)
    ));
}

#[test]
fn parses_match_heartbeat_arguments_using_the_claim_identity_contract() {
    let mut arguments = valid_match_claim_arguments();
    arguments[1] = "heartbeat".to_owned();
    let parsed = parse_match_heartbeat_arguments(arguments).expect("valid heartbeat args");
    assert_eq!(
        parsed.match_id.to_string(),
        "019fe270-3bc5-7cf0-b3d2-0728c077a930"
    );
    assert_eq!(parsed.lease_seconds, 60);
}

#[test]
fn parses_match_release_arguments_without_a_new_expiry() {
    let arguments = vec![
        "match".to_owned(),
        "release".to_owned(),
        "--match-id".to_owned(),
        "019fe270-3bc5-7cf0-b3d2-0728c077a930".to_owned(),
        "--operator-id".to_owned(),
        "019fe270-3bc5-7cf2-b3d2-0728c077a930".to_owned(),
        "--lease-id".to_owned(),
        "019fe270-3bc5-7cf3-b3d2-0728c077a930".to_owned(),
    ];
    let parsed = parse_match_release_arguments(arguments).expect("valid release args");
    assert_eq!(
        parsed.match_id.to_string(),
        "019fe270-3bc5-7cf0-b3d2-0728c077a930"
    );
    assert_eq!(
        parsed.operator_id.to_string(),
        "019fe270-3bc5-7cf2-b3d2-0728c077a930"
    );
    assert_eq!(
        parsed.lease_id.to_string(),
        "019fe270-3bc5-7cf3-b3d2-0728c077a930"
    );
}

#[test]
fn parses_match_takeover_arguments_using_a_new_lease_duration() {
    let mut arguments = valid_match_claim_arguments();
    arguments[1] = "takeover".to_owned();
    let parsed = parse_match_takeover_arguments(arguments).expect("valid takeover args");
    assert_eq!(
        parsed.match_id.to_string(),
        "019fe270-3bc5-7cf0-b3d2-0728c077a930"
    );
    assert_eq!(parsed.lease_seconds, 60);
}
