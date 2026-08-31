use football_domain::{GoalAwardType, LineupRole, MatchPeriod};

use super::{CardKind, LiveCommandAction, parse_draft_lineup, parse_live_command};

fn common(action: &str) -> Vec<String> {
    [
        "match",
        action,
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
        "5",
        "--clock-ms",
        "12000",
        "--occurred-at",
        "2026-08-31T10:00:00+05:00",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

#[test]
fn parses_goal_and_card_without_conflating_c1_meanings() {
    let mut goal = common("goal");
    goal.extend(
        [
            "--period",
            "first-half",
            "--team-id",
            "019ff800-3bc5-7cf6-b3d2-0728c077a930",
            "--scorer-player-id",
            "019ff800-3bc5-7cf7-b3d2-0728c077a930",
            "--award-type",
            "penalty",
        ]
        .into_iter()
        .map(str::to_owned),
    );
    let parsed = parse_live_command(goal).unwrap();
    assert_eq!(parsed.metadata.expected_version.value(), 5);
    assert!(matches!(
        parsed.action,
        LiveCommandAction::Goal {
            period: MatchPeriod::FirstHalf,
            award_type: GoalAwardType::Penalty,
            scorer_player_id: Some(_),
            ..
        }
    ));

    let mut card = common("card");
    card.extend(
        [
            "--period",
            "second-half",
            "--player-id",
            "019ff800-3bc5-7cf7-b3d2-0728c077a930",
            "--card",
            "second-yellow",
        ]
        .into_iter()
        .map(str::to_owned),
    );
    assert!(matches!(
        parse_live_command(card).unwrap().action,
        LiveCommandAction::Card {
            period: MatchPeriod::SecondHalf,
            card: CardKind::SecondYellow,
            ..
        }
    ));
}

#[test]
fn rejects_unknown_or_repeated_live_flags() {
    let mut unknown = common("finish");
    unknown.extend(["--score", "2-0"].into_iter().map(str::to_owned));
    assert!(parse_live_command(unknown).is_err());

    let mut repeated = common("schedule");
    repeated.extend(["--clock-ms", "13000"].into_iter().map(str::to_owned));
    assert!(parse_live_command(repeated).is_err());
}

#[test]
fn parses_draft_lineup_entries_in_separate_roles() {
    let arguments = [
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
        "--starter",
        "019ff800-3bc5-7cf4-b3d2-0728c077a930",
        "--substitute",
        "019ff800-3bc5-7cf5-b3d2-0728c077a930",
    ]
    .into_iter()
    .map(str::to_owned);
    let lineup = parse_draft_lineup(arguments).unwrap().lineup;
    assert_eq!(lineup.entries().len(), 3);
    assert_eq!(lineup.entries()[0].role(), LineupRole::Starter);
    assert_eq!(lineup.entries()[2].role(), LineupRole::Substitute);
}
