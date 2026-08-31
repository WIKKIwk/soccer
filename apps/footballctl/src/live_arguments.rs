use crate::live_error::LiveCommandError;
use football_domain::{
    AdminUserId, AggregateVersion, ClientCommandId, ClockMilliseconds, CommandId, DraftLineup,
    EventId, FixtureId, GoalAwardType, LineupEntry, LineupId, MatchId, MatchPeriod,
    OperatorLeaseId, PlayerId, TeamId, UtcInstant,
};
use std::collections::BTreeMap;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::{Uuid, Version};

#[derive(Debug, Clone, Copy)]
pub(crate) struct LiveCommandMetadata {
    pub(crate) match_id: MatchId,
    pub(crate) operator_id: AdminUserId,
    pub(crate) operator_lease_id: OperatorLeaseId,
    pub(crate) command_id: CommandId,
    pub(crate) client_command_id: ClientCommandId,
    pub(crate) event_id: EventId,
    pub(crate) expected_version: AggregateVersion,
    pub(crate) clock_ms: ClockMilliseconds,
    pub(crate) occurred_at: UtcInstant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LiveCommandAction {
    Schedule,
    SubmitLineup(LineupId),
    ConfirmLineups,
    StartFirstHalf,
    EndFirstHalf,
    StartSecondHalf,
    Finish,
    Goal {
        period: MatchPeriod,
        team_id: TeamId,
        scorer_player_id: Option<PlayerId>,
        award_type: GoalAwardType,
    },
    Card {
        period: MatchPeriod,
        player_id: PlayerId,
        card: CardKind,
    },
    Substitution {
        period: MatchPeriod,
        team_id: TeamId,
        player_on_id: PlayerId,
        player_off_id: PlayerId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CardKind {
    Yellow,
    SecondYellow,
    Red,
}

#[derive(Debug, Clone)]
pub(crate) struct LiveCommandArguments {
    pub(crate) metadata: LiveCommandMetadata,
    pub(crate) action: LiveCommandAction,
}

#[derive(Debug, Clone)]
pub(crate) struct DraftLineupArguments {
    pub(crate) lineup: DraftLineup,
}

type Flags = BTreeMap<String, Vec<String>>;

pub(crate) fn parse_live_command<I>(arguments: I) -> Result<LiveCommandArguments, LiveCommandError>
where
    I: IntoIterator<Item = String>,
{
    let mut arguments = arguments.into_iter();
    if arguments.next().as_deref() != Some("match") {
        return Err(LiveCommandError::invalid("command", "expected match"));
    }
    let action_name = arguments
        .next()
        .ok_or_else(|| LiveCommandError::invalid("command", "live action is required"))?;
    let mut flags = collect_flags(arguments, false)?;
    let metadata = parse_metadata(&mut flags)?;
    let action = match action_name.as_str() {
        "schedule" => LiveCommandAction::Schedule,
        "lineup-submit" => LiveCommandAction::SubmitLineup(LineupId::from_uuid(take_uuid(
            &mut flags,
            "--lineup-id",
        )?)),
        "lineup-confirm" => LiveCommandAction::ConfirmLineups,
        "start-first-half" => LiveCommandAction::StartFirstHalf,
        "end-first-half" => LiveCommandAction::EndFirstHalf,
        "start-second-half" => LiveCommandAction::StartSecondHalf,
        "finish" => LiveCommandAction::Finish,
        "goal" => parse_goal(&mut flags)?,
        "card" => parse_card(&mut flags)?,
        "substitution" => parse_substitution(&mut flags)?,
        _ => return Err(LiveCommandError::invalid("command", "unknown live action")),
    };
    reject_remaining(flags)?;
    Ok(LiveCommandArguments { metadata, action })
}

pub(crate) fn parse_draft_lineup<I>(arguments: I) -> Result<DraftLineupArguments, LiveCommandError>
where
    I: IntoIterator<Item = String>,
{
    let mut arguments = arguments.into_iter();
    if arguments.next().as_deref() != Some("lineup") || arguments.next().as_deref() != Some("save")
    {
        return Err(LiveCommandError::invalid("command", "expected lineup save"));
    }
    let mut flags = collect_flags(arguments, true)?;
    let lineup_id = LineupId::from_uuid(take_uuid(&mut flags, "--lineup-id")?);
    let fixture_id = FixtureId::from_uuid(take_uuid(&mut flags, "--fixture-id")?);
    let team_id = TeamId::from_uuid(take_uuid(&mut flags, "--team-id")?);
    let mut entries = Vec::new();
    for value in flags.remove("--starter").unwrap_or_default() {
        entries.push(LineupEntry::starter(PlayerId::from_uuid(parse_uuidv7(
            "--starter",
            &value,
        )?)));
    }
    for value in flags.remove("--substitute").unwrap_or_default() {
        entries.push(LineupEntry::substitute(PlayerId::from_uuid(parse_uuidv7(
            "--substitute",
            &value,
        )?)));
    }
    reject_remaining(flags)?;
    DraftLineup::try_new(lineup_id, fixture_id, team_id, entries)
        .map(|lineup| DraftLineupArguments { lineup })
        .map_err(LiveCommandError::InvalidLineup)
}

fn parse_metadata(flags: &mut Flags) -> Result<LiveCommandMetadata, LiveCommandError> {
    let client_uuid = take_uuid(flags, "--client-command-id")?;
    let client_command_id = ClientCommandId::try_from_uuid(client_uuid)
        .map_err(|error| LiveCommandError::invalid("--client-command-id", error.to_string()))?;
    let expected_version = take_u64(flags, "--expected-version")?;
    let clock_ms = take_u64(flags, "--clock-ms")?;
    let occurred_at = take_one(flags, "--occurred-at")?;
    let occurred_at = OffsetDateTime::parse(&occurred_at, &Rfc3339)
        .map(UtcInstant::from_offset_datetime)
        .map_err(|_| {
            LiveCommandError::invalid("--occurred-at", "must be RFC3339 with an explicit offset")
        })?;
    Ok(LiveCommandMetadata {
        match_id: MatchId::from_uuid(take_uuid(flags, "--match-id")?),
        operator_id: AdminUserId::from_uuid(take_uuid(flags, "--operator-id")?),
        operator_lease_id: OperatorLeaseId::from_uuid(take_uuid(flags, "--lease-id")?),
        command_id: CommandId::from_uuid(take_uuid(flags, "--command-id")?),
        client_command_id,
        event_id: EventId::from_uuid(take_uuid(flags, "--event-id")?),
        expected_version: AggregateVersion::from_value(expected_version),
        clock_ms: ClockMilliseconds::from_millis(clock_ms),
        occurred_at,
    })
}

fn parse_goal(flags: &mut Flags) -> Result<LiveCommandAction, LiveCommandError> {
    let period = take_period(flags)?;
    let team_id = TeamId::from_uuid(take_uuid(flags, "--team-id")?);
    let scorer_player_id = take_optional(flags, "--scorer-player-id")
        .map(|value| parse_uuidv7("--scorer-player-id", &value))
        .transpose()?
        .map(PlayerId::from_uuid);
    let award_type = match take_one(flags, "--award-type")?.as_str() {
        "regular" => GoalAwardType::Regular,
        "penalty" => GoalAwardType::Penalty,
        "own-goal" => GoalAwardType::OwnGoal,
        _ => {
            return Err(LiveCommandError::invalid(
                "--award-type",
                "expected regular, penalty, or own-goal",
            ));
        }
    };
    Ok(LiveCommandAction::Goal {
        period,
        team_id,
        scorer_player_id,
        award_type,
    })
}

fn parse_card(flags: &mut Flags) -> Result<LiveCommandAction, LiveCommandError> {
    let period = take_period(flags)?;
    let player_id = PlayerId::from_uuid(take_uuid(flags, "--player-id")?);
    let card = match take_one(flags, "--card")?.as_str() {
        "yellow" => CardKind::Yellow,
        "second-yellow" => CardKind::SecondYellow,
        "red" => CardKind::Red,
        _ => {
            return Err(LiveCommandError::invalid(
                "--card",
                "expected yellow, second-yellow, or red",
            ));
        }
    };
    Ok(LiveCommandAction::Card {
        period,
        player_id,
        card,
    })
}

fn parse_substitution(flags: &mut Flags) -> Result<LiveCommandAction, LiveCommandError> {
    Ok(LiveCommandAction::Substitution {
        period: take_period(flags)?,
        team_id: TeamId::from_uuid(take_uuid(flags, "--team-id")?),
        player_on_id: PlayerId::from_uuid(take_uuid(flags, "--player-on-id")?),
        player_off_id: PlayerId::from_uuid(take_uuid(flags, "--player-off-id")?),
    })
}

fn take_period(flags: &mut Flags) -> Result<MatchPeriod, LiveCommandError> {
    match take_one(flags, "--period")?.as_str() {
        "first-half" => Ok(MatchPeriod::FirstHalf),
        "second-half" => Ok(MatchPeriod::SecondHalf),
        _ => Err(LiveCommandError::invalid(
            "--period",
            "expected first-half or second-half",
        )),
    }
}

fn collect_flags<I>(arguments: I, repeated: bool) -> Result<Flags, LiveCommandError>
where
    I: IntoIterator<Item = String>,
{
    let mut arguments = arguments.into_iter();
    let mut flags = Flags::new();
    while let Some(flag) = arguments.next() {
        if !flag.starts_with("--") {
            return Err(LiveCommandError::invalid(flag, "expected a named flag"));
        }
        let value = arguments
            .next()
            .ok_or_else(|| LiveCommandError::invalid(&flag, "value is required"))?;
        let values = flags.entry(flag.clone()).or_default();
        if !repeated && !values.is_empty() {
            return Err(LiveCommandError::invalid(flag, "flag cannot repeat"));
        }
        values.push(value);
    }
    Ok(flags)
}

fn take_one(flags: &mut Flags, name: &'static str) -> Result<String, LiveCommandError> {
    flags
        .remove(name)
        .and_then(|mut values| (values.len() == 1).then(|| values.remove(0)))
        .ok_or_else(|| LiveCommandError::invalid(name, "exactly one value is required"))
}

fn take_optional(flags: &mut Flags, name: &'static str) -> Option<String> {
    flags.remove(name).and_then(|mut values| values.pop())
}

fn take_uuid(flags: &mut Flags, name: &'static str) -> Result<Uuid, LiveCommandError> {
    let value = take_one(flags, name)?;
    parse_uuidv7(name, &value)
}

fn take_u64(flags: &mut Flags, name: &'static str) -> Result<u64, LiveCommandError> {
    take_one(flags, name)?
        .parse::<u64>()
        .map_err(|_| LiveCommandError::invalid(name, "must be a non-negative integer"))
}

fn parse_uuidv7(name: &'static str, value: &str) -> Result<Uuid, LiveCommandError> {
    Uuid::parse_str(value)
        .ok()
        .filter(|uuid| uuid.get_version() == Some(Version::SortRand) && uuid.to_string() == value)
        .ok_or_else(|| LiveCommandError::invalid(name, "must be a canonical UUIDv7"))
}

fn reject_remaining(flags: Flags) -> Result<(), LiveCommandError> {
    match flags.into_keys().next() {
        Some(flag) => Err(LiveCommandError::invalid(flag, "flag is not valid here")),
        None => Ok(()),
    }
}

#[cfg(test)]
#[path = "live_arguments_tests.rs"]
mod tests;
