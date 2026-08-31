use crate::error::CliError;
use football_application::identity::DevelopmentAdministrator;
use football_domain::{
    AdminUserId, Competition, CompetitionId, CompetitionName, FixtureId, MatchAssignment, MatchId,
    OperatorLeaseExpiryPolicy, OperatorLeaseId, Person, PersonId, PersonName, Player, PlayerId,
    Season, SeasonId, SeasonYear, Team, TeamId, TeamName, UtcInstant,
};
use uuid::{Uuid, Version};

pub(crate) struct SeedArguments {
    pub(crate) administrator: DevelopmentAdministrator,
}

pub(crate) struct TeamSeedArguments {
    pub(crate) first: Team,
    pub(crate) second: Team,
}

pub(crate) struct PlayerSeedArguments {
    pub(crate) person: Person,
    pub(crate) player: Player,
    pub(crate) team_id: TeamId,
}

pub(crate) struct CompetitionSeedArguments {
    pub(crate) competition: Competition,
}

pub(crate) struct SeasonSeedArguments {
    pub(crate) season: Season,
}

pub(crate) struct FixtureScheduleArguments {
    pub(crate) id: FixtureId,
    pub(crate) season_id: SeasonId,
    pub(crate) home_team_id: TeamId,
    pub(crate) away_team_id: TeamId,
    pub(crate) kickoff: UtcInstant,
}

pub(crate) struct MatchAssignmentArguments {
    pub(crate) assignment: MatchAssignment,
}

pub(crate) struct MatchClaimArguments {
    pub(crate) match_id: MatchId,
    pub(crate) operator_id: AdminUserId,
    pub(crate) lease_id: OperatorLeaseId,
    pub(crate) lease_seconds: u32,
}

pub(crate) struct MatchReleaseArguments {
    pub(crate) match_id: MatchId,
    pub(crate) operator_id: AdminUserId,
    pub(crate) lease_id: OperatorLeaseId,
}

pub(crate) fn parse_seed_arguments<I>(arguments: I) -> Result<SeedArguments, CliError>
where
    I: IntoIterator<Item = String>,
{
    let mut arguments = arguments.into_iter();
    if arguments.next().as_deref() != Some("admin")
        || arguments.next().as_deref() != Some("seed-development")
    {
        return Err(CliError::Usage);
    }

    let mut id = None;
    let mut username = None;
    let mut display_name = None;
    while let Some(flag) = arguments.next() {
        let value = arguments.next().ok_or(CliError::Usage)?;
        match flag.as_str() {
            "--id" if id.is_none() => id = Some(value),
            "--username" if username.is_none() => username = Some(value),
            "--display-name" if display_name.is_none() => display_name = Some(value),
            _ => return Err(CliError::Usage),
        }
    }

    let id = id.ok_or(CliError::Usage)?;
    let uuid = Uuid::parse_str(&id).map_err(|_| CliError::InvalidUuid)?;
    if uuid.get_version() != Some(Version::SortRand) {
        return Err(CliError::UuidMustBeV7);
    }

    let administrator = DevelopmentAdministrator::new(
        AdminUserId::from_uuid(uuid),
        username.ok_or(CliError::Usage)?,
        display_name.ok_or(CliError::Usage)?,
    )
    .map_err(CliError::InvalidAdministrator)?;

    Ok(SeedArguments { administrator })
}

pub(crate) fn parse_team_arguments<I>(arguments: I) -> Result<TeamSeedArguments, CliError>
where
    I: IntoIterator<Item = String>,
{
    let mut arguments = arguments.into_iter();
    if arguments.next().as_deref() != Some("directory")
        || !matches!(
            arguments.next().as_deref(),
            Some("seed-teams") | Some("import-teams")
        )
    {
        return Err(CliError::Usage);
    }

    let mut first_id = None;
    let mut first_name = None;
    let mut second_id = None;
    let mut second_name = None;
    while let Some(flag) = arguments.next() {
        let value = arguments.next().ok_or(CliError::Usage)?;
        match flag.as_str() {
            "--first-id" if first_id.is_none() => first_id = Some(value),
            "--first-name" if first_name.is_none() => first_name = Some(value),
            "--second-id" if second_id.is_none() => second_id = Some(value),
            "--second-name" if second_name.is_none() => second_name = Some(value),
            _ => return Err(CliError::Usage),
        }
    }

    let first_id = TeamId::from_uuid(parse_uuidv7(first_id.ok_or(CliError::Usage)?)?);
    let second_id = TeamId::from_uuid(parse_uuidv7(second_id.ok_or(CliError::Usage)?)?);
    if first_id == second_id {
        return Err(CliError::SameTeamId);
    }

    let first = Team::new(
        first_id,
        TeamName::try_new(first_name.ok_or(CliError::Usage)?).map_err(CliError::InvalidTeamName)?,
    );
    let second = Team::new(
        second_id,
        TeamName::try_new(second_name.ok_or(CliError::Usage)?)
            .map_err(CliError::InvalidTeamName)?,
    );

    Ok(TeamSeedArguments { first, second })
}

pub(crate) fn parse_player_arguments<I>(arguments: I) -> Result<PlayerSeedArguments, CliError>
where
    I: IntoIterator<Item = String>,
{
    let mut arguments = arguments.into_iter();
    if arguments.next().as_deref() != Some("directory")
        || !matches!(
            arguments.next().as_deref(),
            Some("seed-player") | Some("import-player")
        )
    {
        return Err(CliError::Usage);
    }

    let mut person_id = None;
    let mut player_id = None;
    let mut team_id = None;
    let mut name = None;
    while let Some(flag) = arguments.next() {
        let value = arguments.next().ok_or(CliError::Usage)?;
        match flag.as_str() {
            "--person-id" if person_id.is_none() => person_id = Some(value),
            "--player-id" if player_id.is_none() => player_id = Some(value),
            "--team-id" if team_id.is_none() => team_id = Some(value),
            "--name" if name.is_none() => name = Some(value),
            _ => return Err(CliError::Usage),
        }
    }

    let person = Person::new(
        PersonId::from_uuid(parse_uuidv7(person_id.ok_or(CliError::Usage)?)?),
        PersonName::try_new(name.ok_or(CliError::Usage)?).map_err(CliError::InvalidPersonName)?,
    );
    let player = Player::new(
        PlayerId::from_uuid(parse_uuidv7(player_id.ok_or(CliError::Usage)?)?),
        person.id(),
    );
    let team_id = TeamId::from_uuid(parse_uuidv7(team_id.ok_or(CliError::Usage)?)?);

    Ok(PlayerSeedArguments {
        person,
        player,
        team_id,
    })
}

pub(crate) fn parse_competition_arguments<I>(
    arguments: I,
) -> Result<CompetitionSeedArguments, CliError>
where
    I: IntoIterator<Item = String>,
{
    let mut arguments = arguments.into_iter();
    if arguments.next().as_deref() != Some("competition")
        || arguments.next().as_deref() != Some("seed")
    {
        return Err(CliError::Usage);
    }

    let mut id = None;
    let mut name = None;
    while let Some(flag) = arguments.next() {
        let value = arguments.next().ok_or(CliError::Usage)?;
        match flag.as_str() {
            "--id" if id.is_none() => id = Some(value),
            "--name" if name.is_none() => name = Some(value),
            _ => return Err(CliError::Usage),
        }
    }

    let competition = Competition::new(
        CompetitionId::from_uuid(parse_uuidv7(id.ok_or(CliError::Usage)?)?),
        CompetitionName::try_new(name.ok_or(CliError::Usage)?)
            .map_err(CliError::InvalidCompetitionName)?,
    );

    Ok(CompetitionSeedArguments { competition })
}

pub(crate) fn parse_season_arguments<I>(arguments: I) -> Result<SeasonSeedArguments, CliError>
where
    I: IntoIterator<Item = String>,
{
    let mut arguments = arguments.into_iter();
    if arguments.next().as_deref() != Some("season") || arguments.next().as_deref() != Some("seed")
    {
        return Err(CliError::Usage);
    }

    let mut id = None;
    let mut competition_id = None;
    let mut year = None;
    while let Some(flag) = arguments.next() {
        let value = arguments.next().ok_or(CliError::Usage)?;
        match flag.as_str() {
            "--id" if id.is_none() => id = Some(value),
            "--competition-id" if competition_id.is_none() => competition_id = Some(value),
            "--year" if year.is_none() => year = Some(value),
            _ => return Err(CliError::Usage),
        }
    }

    let year = year
        .ok_or(CliError::Usage)?
        .parse::<u16>()
        .map_err(|_| CliError::InvalidSeasonYearValue)?;
    let season = Season::new(
        SeasonId::from_uuid(parse_uuidv7(id.ok_or(CliError::Usage)?)?),
        CompetitionId::from_uuid(parse_uuidv7(competition_id.ok_or(CliError::Usage)?)?),
        SeasonYear::try_new(year).map_err(CliError::InvalidSeasonYear)?,
    );

    Ok(SeasonSeedArguments { season })
}

pub(crate) fn parse_fixture_schedule_arguments<I>(
    arguments: I,
) -> Result<FixtureScheduleArguments, CliError>
where
    I: IntoIterator<Item = String>,
{
    let mut arguments = arguments.into_iter();
    if arguments.next().as_deref() != Some("fixture")
        || arguments.next().as_deref() != Some("schedule")
    {
        return Err(CliError::Usage);
    }

    let mut id = None;
    let mut season_id = None;
    let mut home_team_id = None;
    let mut away_team_id = None;
    let mut kickoff = None;
    while let Some(flag) = arguments.next() {
        let value = arguments.next().ok_or(CliError::Usage)?;
        match flag.as_str() {
            "--id" if id.is_none() => id = Some(value),
            "--season-id" if season_id.is_none() => season_id = Some(value),
            "--home-team-id" if home_team_id.is_none() => home_team_id = Some(value),
            "--away-team-id" if away_team_id.is_none() => away_team_id = Some(value),
            "--kickoff" if kickoff.is_none() => kickoff = Some(value),
            _ => return Err(CliError::Usage),
        }
    }

    let kickoff = time::OffsetDateTime::parse(
        &kickoff.ok_or(CliError::Usage)?,
        &time::format_description::well_known::Rfc3339,
    )
    .map_err(CliError::InvalidKickoff)
    .map(UtcInstant::from_offset_datetime)?;

    Ok(FixtureScheduleArguments {
        id: FixtureId::from_uuid(parse_uuidv7(id.ok_or(CliError::Usage)?)?),
        season_id: SeasonId::from_uuid(parse_uuidv7(season_id.ok_or(CliError::Usage)?)?),
        home_team_id: TeamId::from_uuid(parse_uuidv7(home_team_id.ok_or(CliError::Usage)?)?),
        away_team_id: TeamId::from_uuid(parse_uuidv7(away_team_id.ok_or(CliError::Usage)?)?),
        kickoff,
    })
}

pub(crate) fn parse_match_assignment_arguments<I>(
    arguments: I,
) -> Result<MatchAssignmentArguments, CliError>
where
    I: IntoIterator<Item = String>,
{
    let mut arguments = arguments.into_iter();
    if arguments.next().as_deref() != Some("match") || arguments.next().as_deref() != Some("assign")
    {
        return Err(CliError::Usage);
    }

    let mut match_id = None;
    let mut fixture_id = None;
    let mut operator_id = None;
    while let Some(flag) = arguments.next() {
        let value = arguments.next().ok_or(CliError::Usage)?;
        match flag.as_str() {
            "--match-id" if match_id.is_none() => match_id = Some(value),
            "--fixture-id" if fixture_id.is_none() => fixture_id = Some(value),
            "--operator-id" if operator_id.is_none() => operator_id = Some(value),
            _ => return Err(CliError::Usage),
        }
    }

    Ok(MatchAssignmentArguments {
        assignment: MatchAssignment::new(
            MatchId::from_uuid(parse_uuidv7(match_id.ok_or(CliError::Usage)?)?),
            FixtureId::from_uuid(parse_uuidv7(fixture_id.ok_or(CliError::Usage)?)?),
            AdminUserId::from_uuid(parse_uuidv7(operator_id.ok_or(CliError::Usage)?)?),
        ),
    })
}

pub(crate) fn parse_match_claim_arguments<I>(arguments: I) -> Result<MatchClaimArguments, CliError>
where
    I: IntoIterator<Item = String>,
{
    parse_match_lease_arguments(arguments, "claim")
}

pub(crate) fn parse_match_heartbeat_arguments<I>(
    arguments: I,
) -> Result<MatchClaimArguments, CliError>
where
    I: IntoIterator<Item = String>,
{
    parse_match_lease_arguments(arguments, "heartbeat")
}

pub(crate) fn parse_match_takeover_arguments<I>(
    arguments: I,
) -> Result<MatchClaimArguments, CliError>
where
    I: IntoIterator<Item = String>,
{
    parse_match_lease_arguments(arguments, "takeover")
}

pub(crate) fn parse_match_release_arguments<I>(
    arguments: I,
) -> Result<MatchReleaseArguments, CliError>
where
    I: IntoIterator<Item = String>,
{
    let mut arguments = arguments.into_iter();
    if arguments.next().as_deref() != Some("match")
        || arguments.next().as_deref() != Some("release")
    {
        return Err(CliError::Usage);
    }

    let mut match_id = None;
    let mut operator_id = None;
    let mut lease_id = None;
    while let Some(flag) = arguments.next() {
        let value = arguments.next().ok_or(CliError::Usage)?;
        match flag.as_str() {
            "--match-id" if match_id.is_none() => match_id = Some(value),
            "--operator-id" if operator_id.is_none() => operator_id = Some(value),
            "--lease-id" if lease_id.is_none() => lease_id = Some(value),
            _ => return Err(CliError::Usage),
        }
    }

    Ok(MatchReleaseArguments {
        match_id: MatchId::from_uuid(parse_uuidv7(match_id.ok_or(CliError::Usage)?)?),
        operator_id: AdminUserId::from_uuid(parse_uuidv7(operator_id.ok_or(CliError::Usage)?)?),
        lease_id: OperatorLeaseId::from_uuid(parse_uuidv7(lease_id.ok_or(CliError::Usage)?)?),
    })
}

fn parse_match_lease_arguments<I>(
    arguments: I,
    action: &str,
) -> Result<MatchClaimArguments, CliError>
where
    I: IntoIterator<Item = String>,
{
    let mut arguments = arguments.into_iter();
    if arguments.next().as_deref() != Some("match") || arguments.next().as_deref() != Some(action) {
        return Err(CliError::Usage);
    }

    let mut match_id = None;
    let mut operator_id = None;
    let mut lease_id = None;
    let mut lease_seconds = None;
    while let Some(flag) = arguments.next() {
        let value = arguments.next().ok_or(CliError::Usage)?;
        match flag.as_str() {
            "--match-id" if match_id.is_none() => match_id = Some(value),
            "--operator-id" if operator_id.is_none() => operator_id = Some(value),
            "--lease-id" if lease_id.is_none() => lease_id = Some(value),
            "--lease-seconds" if lease_seconds.is_none() => lease_seconds = Some(value),
            _ => return Err(CliError::Usage),
        }
    }

    let lease_seconds = lease_seconds
        .ok_or(CliError::Usage)?
        .parse::<u32>()
        .map_err(|_| CliError::InvalidLeaseSeconds)?;
    OperatorLeaseExpiryPolicy::validate_duration_seconds(lease_seconds)
        .map_err(CliError::InvalidLeaseExpiryPolicy)?;

    Ok(MatchClaimArguments {
        match_id: MatchId::from_uuid(parse_uuidv7(match_id.ok_or(CliError::Usage)?)?),
        operator_id: AdminUserId::from_uuid(parse_uuidv7(operator_id.ok_or(CliError::Usage)?)?),
        lease_id: OperatorLeaseId::from_uuid(parse_uuidv7(lease_id.ok_or(CliError::Usage)?)?),
        lease_seconds,
    })
}

fn parse_uuidv7(value: String) -> Result<Uuid, CliError> {
    let uuid = Uuid::parse_str(&value).map_err(|_| CliError::InvalidUuid)?;
    if uuid.get_version() != Some(Version::SortRand) {
        return Err(CliError::UuidMustBeV7);
    }

    Ok(uuid)
}

#[cfg(test)]
#[path = "arguments_tests.rs"]
mod tests;
