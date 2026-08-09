#![forbid(unsafe_code)]

use football_application::identity::DevelopmentAdministrator;
use football_domain::{
    AdminUserId, Competition, CompetitionId, CompetitionName, CompetitionNameError, Fixture,
    FixtureError, FixtureId, MatchAssignment, MatchId, OperatorLeaseExpiryPolicy,
    OperatorLeaseExpiryPolicyError, OperatorLeaseId, Person, PersonId, PersonName, PersonNameError,
    Player, PlayerId, Season, SeasonId, SeasonYear, SeasonYearError, Team, TeamId, TeamName,
    TeamNameError, UtcInstant,
};
use football_infrastructure::{
    CompetitionRepositoryError, DatabasePoolConfig, FixtureRepositoryError,
    OperatorAssignmentRepositoryError, PlayerRepositoryError, TeamRepositoryError,
    claim_operator_lease, connect_database, heartbeat_operator_lease, load_admin_user,
    load_competition, load_fixture, load_match_assignment, load_operator_lease,
    load_player_assignment, load_season, load_team, persist_active_admin, persist_competition,
    persist_fixture, persist_match_assignment, persist_player_for_team, persist_season,
    persist_two_teams, release_operator_lease, takeover_operator_lease,
};
use football_platform::ApplicationConfig;
use std::{env, error::Error, fmt};
use uuid::{Uuid, Version};

const SEED_AUTHORIZATION_ENV: &str = "ALLOW_DEVELOPMENT_ADMIN_SEED";
const SEED_AUTHORIZATION_VALUE: &str = "1";
const DIRECTORY_SEED_AUTHORIZATION_ENV: &str = "ALLOW_DEVELOPMENT_DIRECTORY_SEED";
const COMPETITION_SEED_AUTHORIZATION_ENV: &str = "ALLOW_DEVELOPMENT_COMPETITION_SEED";
const SCHEDULING_SEED_AUTHORIZATION_ENV: &str = "ALLOW_DEVELOPMENT_SCHEDULING_SEED";
const OPERATIONS_SEED_AUTHORIZATION_ENV: &str = "ALLOW_DEVELOPMENT_OPERATIONS_SEED";

#[tokio::main]
async fn main() {
    if let Err(error) = run(env::args().skip(1)).await {
        eprintln!("footballctl failed: {error}");
        std::process::exit(1);
    }
}

async fn run<I>(arguments: I) -> Result<(), CliError>
where
    I: IntoIterator<Item = String>,
{
    let arguments: Vec<_> = arguments.into_iter().collect();
    match arguments.first().map(String::as_str) {
        Some("admin") => run_admin_seed(arguments).await,
        Some("directory") => run_directory_command(arguments).await,
        Some("competition") => run_competition_command(arguments).await,
        Some("season") => run_season_command(arguments).await,
        Some("fixture") => run_fixture_command(arguments).await,
        Some("match") => run_match_command(arguments).await,
        _ => Err(CliError::Usage),
    }
}

async fn run_directory_command(arguments: Vec<String>) -> Result<(), CliError> {
    match arguments.get(1).map(String::as_str) {
        Some("seed-teams") | Some("import-teams") => run_team_seed(arguments).await,
        Some("seed-player") | Some("import-player") => run_player_seed(arguments).await,
        _ => Err(CliError::Usage),
    }
}

async fn run_competition_command(arguments: Vec<String>) -> Result<(), CliError> {
    match arguments.get(1).map(String::as_str) {
        Some("seed") => run_competition_seed(arguments).await,
        _ => Err(CliError::Usage),
    }
}

async fn run_season_command(arguments: Vec<String>) -> Result<(), CliError> {
    match arguments.get(1).map(String::as_str) {
        Some("seed") => run_season_seed(arguments).await,
        _ => Err(CliError::Usage),
    }
}

async fn run_fixture_command(arguments: Vec<String>) -> Result<(), CliError> {
    match arguments.get(1).map(String::as_str) {
        Some("schedule") => run_fixture_schedule(arguments).await,
        _ => Err(CliError::Usage),
    }
}

async fn run_match_command(arguments: Vec<String>) -> Result<(), CliError> {
    match arguments.get(1).map(String::as_str) {
        Some("assign") => run_match_assign(arguments).await,
        Some("claim") => run_match_claim(arguments).await,
        Some("heartbeat") => run_match_heartbeat(arguments).await,
        Some("release") => run_match_release(arguments).await,
        Some("takeover") => run_match_takeover(arguments).await,
        _ => Err(CliError::Usage),
    }
}

async fn run_admin_seed(arguments: Vec<String>) -> Result<(), CliError> {
    let seed = parse_seed_arguments(arguments)?;
    require_seed_authorization()?;

    let config = ApplicationConfig::from_env().map_err(CliError::Configuration)?;
    let pool = connect_database(config.database_url(), DatabasePoolConfig::default())
        .await
        .map_err(CliError::DatabaseConnection)?;

    if let Some(existing) = load_admin_user(&pool, seed.administrator.id())
        .await
        .map_err(CliError::AdminUserRepository)?
    {
        if existing.is_active()
            && existing.username() == seed.administrator.username()
            && existing.display_name() == seed.administrator.display_name()
        {
            println!(
                "development administrator already persisted: {}",
                seed.administrator.id()
            );
            pool.close().await;
            return Ok(());
        }

        return Err(CliError::ExistingAdministratorConflict {
            id: seed.administrator.id(),
        });
    }

    let persisted = persist_active_admin(&pool, &seed.administrator)
        .await
        .map_err(CliError::AdminUserRepository)?;
    let read_back = load_admin_user(&pool, persisted.id())
        .await
        .map_err(CliError::AdminUserRepository)?
        .ok_or(CliError::ReadBackMissing { id: persisted.id() })?;

    if read_back != persisted {
        return Err(CliError::ReadBackMismatch { id: persisted.id() });
    }

    println!("development administrator persisted: {}", persisted.id());
    pool.close().await;
    Ok(())
}

async fn run_team_seed(arguments: Vec<String>) -> Result<(), CliError> {
    let seed = parse_team_arguments(arguments)?;
    require_directory_seed_authorization()?;

    let config = ApplicationConfig::from_env().map_err(CliError::Configuration)?;
    let pool = connect_database(config.database_url(), DatabasePoolConfig::default())
        .await
        .map_err(CliError::DatabaseConnection)?;

    let (first, second) = persist_two_teams(&pool, &seed.first, &seed.second)
        .await
        .map_err(CliError::TeamRepository)?;

    for persisted in [first, second] {
        let read_back = load_team(&pool, persisted.id())
            .await
            .map_err(CliError::TeamRepository)?
            .ok_or(CliError::TeamReadBackMissing { id: persisted.id() })?;

        if read_back != persisted {
            return Err(CliError::TeamReadBackMismatch { id: persisted.id() });
        }

        println!("team persisted and verified: {}", persisted.id());
    }

    pool.close().await;
    Ok(())
}

async fn run_player_seed(arguments: Vec<String>) -> Result<(), CliError> {
    let seed = parse_player_arguments(arguments)?;
    require_directory_seed_authorization()?;

    let config = ApplicationConfig::from_env().map_err(CliError::Configuration)?;
    let pool = connect_database(config.database_url(), DatabasePoolConfig::default())
        .await
        .map_err(CliError::DatabaseConnection)?;

    let persisted = persist_player_for_team(&pool, &seed.person, seed.player, seed.team_id)
        .await
        .map_err(CliError::PlayerRepository)?;
    let read_back = load_player_assignment(&pool, seed.player.id(), seed.team_id)
        .await
        .map_err(CliError::PlayerRepository)?
        .ok_or(CliError::PlayerReadBackMissing {
            id: seed.player.id(),
            team_id: seed.team_id,
        })?;

    if read_back != persisted {
        return Err(CliError::PlayerReadBackMismatch {
            id: seed.player.id(),
            team_id: seed.team_id,
        });
    }

    println!(
        "player persisted and verified: {} for team {}",
        persisted.player().id(),
        persisted.assignment().team_id()
    );
    pool.close().await;
    Ok(())
}

async fn run_competition_seed(arguments: Vec<String>) -> Result<(), CliError> {
    let seed = parse_competition_arguments(arguments)?;
    require_competition_seed_authorization()?;

    let config = ApplicationConfig::from_env().map_err(CliError::Configuration)?;
    let pool = connect_database(config.database_url(), DatabasePoolConfig::default())
        .await
        .map_err(CliError::DatabaseConnection)?;

    let persisted = persist_competition(&pool, &seed.competition)
        .await
        .map_err(CliError::CompetitionRepository)?;
    let read_back = load_competition(&pool, persisted.id())
        .await
        .map_err(CliError::CompetitionRepository)?
        .ok_or(CliError::CompetitionReadBackMissing { id: persisted.id() })?;

    if read_back != persisted {
        return Err(CliError::CompetitionReadBackMismatch { id: persisted.id() });
    }

    println!("competition persisted and verified: {}", persisted.id());
    pool.close().await;
    Ok(())
}

async fn run_season_seed(arguments: Vec<String>) -> Result<(), CliError> {
    let seed = parse_season_arguments(arguments)?;
    require_competition_seed_authorization()?;

    let config = ApplicationConfig::from_env().map_err(CliError::Configuration)?;
    let pool = connect_database(config.database_url(), DatabasePoolConfig::default())
        .await
        .map_err(CliError::DatabaseConnection)?;

    let persisted = persist_season(&pool, seed.season)
        .await
        .map_err(CliError::CompetitionRepository)?;
    let read_back = load_season(&pool, persisted.id())
        .await
        .map_err(CliError::CompetitionRepository)?
        .ok_or(CliError::SeasonReadBackMissing { id: persisted.id() })?;

    if read_back != persisted {
        return Err(CliError::SeasonReadBackMismatch { id: persisted.id() });
    }

    println!("season persisted and verified: {}", persisted.id());
    pool.close().await;
    Ok(())
}

async fn run_fixture_schedule(arguments: Vec<String>) -> Result<(), CliError> {
    let schedule = parse_fixture_schedule_arguments(arguments)?;
    require_scheduling_seed_authorization()?;

    let config = ApplicationConfig::from_env().map_err(CliError::Configuration)?;
    let pool = connect_database(config.database_url(), DatabasePoolConfig::default())
        .await
        .map_err(CliError::DatabaseConnection)?;
    let season = load_season(&pool, schedule.season_id)
        .await
        .map_err(CliError::CompetitionRepository)?
        .ok_or(CliError::FixtureSeasonNotFound {
            id: schedule.season_id,
        })?;
    let fixture = Fixture::try_new(
        schedule.id,
        season,
        schedule.home_team_id,
        schedule.away_team_id,
        schedule.kickoff,
    )
    .map_err(CliError::InvalidFixture)?;

    let persisted = persist_fixture(&pool, &fixture)
        .await
        .map_err(CliError::FixtureRepository)?;
    let read_back = load_fixture(&pool, persisted.id())
        .await
        .map_err(CliError::FixtureRepository)?
        .ok_or(CliError::FixtureReadBackMissing { id: persisted.id() })?;
    if read_back != persisted {
        return Err(CliError::FixtureReadBackMismatch { id: persisted.id() });
    }

    println!("fixture scheduled and verified: {}", persisted.id());
    pool.close().await;
    Ok(())
}

async fn run_match_assign(arguments: Vec<String>) -> Result<(), CliError> {
    let assignment = parse_match_assignment_arguments(arguments)?;
    require_operations_seed_authorization()?;

    let config = ApplicationConfig::from_env().map_err(CliError::Configuration)?;
    let pool = connect_database(config.database_url(), DatabasePoolConfig::default())
        .await
        .map_err(CliError::DatabaseConnection)?;

    let persisted = persist_match_assignment(&pool, &assignment.assignment)
        .await
        .map_err(CliError::OperationsRepository)?;
    let read_back = load_match_assignment(&pool, persisted.match_id())
        .await
        .map_err(CliError::OperationsRepository)?
        .ok_or(CliError::AssignmentReadBackMissing {
            id: persisted.match_id(),
        })?;
    if read_back != persisted {
        return Err(CliError::AssignmentReadBackMismatch {
            id: persisted.match_id(),
        });
    }

    println!(
        "match assignment persisted and verified: {}",
        persisted.match_id()
    );
    pool.close().await;
    Ok(())
}

async fn run_match_claim(arguments: Vec<String>) -> Result<(), CliError> {
    let claim = parse_match_claim_arguments(arguments)?;
    require_operations_seed_authorization()?;

    let claimed_at = current_utc_instant()?;
    let expires_at = OperatorLeaseExpiryPolicy::expiry_at(claimed_at, claim.lease_seconds)
        .map_err(CliError::InvalidLeaseExpiryPolicy)?;

    let config = ApplicationConfig::from_env().map_err(CliError::Configuration)?;
    let pool = connect_database(config.database_url(), DatabasePoolConfig::default())
        .await
        .map_err(CliError::DatabaseConnection)?;

    let persisted = claim_operator_lease(
        &pool,
        claim.match_id,
        claim.operator_id,
        claim.lease_id,
        claimed_at,
        expires_at,
    )
    .await
    .map_err(CliError::OperationsRepository)?;
    let read_back = load_operator_lease(&pool, persisted.match_id())
        .await
        .map_err(CliError::OperationsRepository)?
        .ok_or(CliError::LeaseReadBackMissing { id: persisted.id() })?;
    if read_back != persisted {
        return Err(CliError::LeaseReadBackMismatch { id: persisted.id() });
    }

    println!(
        "operator lease claimed and verified: {} expires_at={}",
        persisted.id(),
        persisted.expires_at().as_offset_datetime()
    );
    pool.close().await;
    Ok(())
}

async fn run_match_heartbeat(arguments: Vec<String>) -> Result<(), CliError> {
    let heartbeat = parse_match_heartbeat_arguments(arguments)?;
    require_operations_seed_authorization()?;

    let heartbeat_at = current_utc_instant()?;
    let expires_at = OperatorLeaseExpiryPolicy::expiry_at(heartbeat_at, heartbeat.lease_seconds)
        .map_err(CliError::InvalidLeaseExpiryPolicy)?;

    let config = ApplicationConfig::from_env().map_err(CliError::Configuration)?;
    let pool = connect_database(config.database_url(), DatabasePoolConfig::default())
        .await
        .map_err(CliError::DatabaseConnection)?;

    let persisted = heartbeat_operator_lease(
        &pool,
        heartbeat.match_id,
        heartbeat.operator_id,
        heartbeat.lease_id,
        heartbeat_at,
        expires_at,
    )
    .await
    .map_err(CliError::OperationsRepository)?;
    let read_back = load_operator_lease(&pool, persisted.match_id())
        .await
        .map_err(CliError::OperationsRepository)?
        .ok_or(CliError::HeartbeatReadBackMissing { id: persisted.id() })?;
    if read_back != persisted {
        return Err(CliError::HeartbeatReadBackMismatch { id: persisted.id() });
    }

    println!(
        "operator lease heartbeat persisted and verified: {} expires_at={}",
        persisted.id(),
        persisted.expires_at().as_offset_datetime()
    );
    pool.close().await;
    Ok(())
}

async fn run_match_release(arguments: Vec<String>) -> Result<(), CliError> {
    let release = parse_match_release_arguments(arguments)?;
    require_operations_seed_authorization()?;

    let config = ApplicationConfig::from_env().map_err(CliError::Configuration)?;
    let pool = connect_database(config.database_url(), DatabasePoolConfig::default())
        .await
        .map_err(CliError::DatabaseConnection)?;

    release_operator_lease(
        &pool,
        release.match_id,
        release.operator_id,
        release.lease_id,
    )
    .await
    .map_err(CliError::OperationsRepository)?;
    if load_operator_lease(&pool, release.match_id)
        .await
        .map_err(CliError::OperationsRepository)?
        .is_some()
    {
        return Err(CliError::ReleaseReadBackMismatch {
            id: release.lease_id,
        });
    }

    println!("operator lease released and verified: {}", release.lease_id);
    pool.close().await;
    Ok(())
}

async fn run_match_takeover(arguments: Vec<String>) -> Result<(), CliError> {
    let takeover = parse_match_takeover_arguments(arguments)?;
    require_operations_seed_authorization()?;

    let claimed_at = current_utc_instant()?;
    let expires_at = OperatorLeaseExpiryPolicy::expiry_at(claimed_at, takeover.lease_seconds)
        .map_err(CliError::InvalidLeaseExpiryPolicy)?;

    let config = ApplicationConfig::from_env().map_err(CliError::Configuration)?;
    let pool = connect_database(config.database_url(), DatabasePoolConfig::default())
        .await
        .map_err(CliError::DatabaseConnection)?;
    let persisted = takeover_operator_lease(
        &pool,
        takeover.match_id,
        takeover.operator_id,
        takeover.lease_id,
        claimed_at,
        expires_at,
    )
    .await
    .map_err(CliError::OperationsRepository)?;
    let read_back = load_operator_lease(&pool, persisted.match_id())
        .await
        .map_err(CliError::OperationsRepository)?
        .ok_or(CliError::TakeoverReadBackMissing { id: persisted.id() })?;
    if read_back != persisted {
        return Err(CliError::TakeoverReadBackMismatch { id: persisted.id() });
    }

    println!(
        "operator lease takeover persisted and verified: {} operator={} expires_at={}",
        persisted.id(),
        persisted.operator_id(),
        persisted.expires_at().as_offset_datetime()
    );
    pool.close().await;
    Ok(())
}

fn current_utc_instant() -> Result<UtcInstant, CliError> {
    let now = time::OffsetDateTime::now_utc();
    let microsecond_precision = (now.nanosecond() / 1_000) * 1_000;
    let now = now
        .replace_nanosecond(microsecond_precision)
        .map_err(CliError::InvalidCurrentTime)?;
    Ok(UtcInstant::from_offset_datetime(now))
}

struct SeedArguments {
    administrator: DevelopmentAdministrator,
}

fn parse_seed_arguments<I>(arguments: I) -> Result<SeedArguments, CliError>
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

struct TeamSeedArguments {
    first: Team,
    second: Team,
}

struct PlayerSeedArguments {
    person: Person,
    player: Player,
    team_id: TeamId,
}

struct CompetitionSeedArguments {
    competition: Competition,
}

struct SeasonSeedArguments {
    season: Season,
}

struct FixtureScheduleArguments {
    id: FixtureId,
    season_id: SeasonId,
    home_team_id: TeamId,
    away_team_id: TeamId,
    kickoff: UtcInstant,
}

struct MatchAssignmentArguments {
    assignment: MatchAssignment,
}

struct MatchClaimArguments {
    match_id: MatchId,
    operator_id: AdminUserId,
    lease_id: OperatorLeaseId,
    lease_seconds: u32,
}

struct MatchReleaseArguments {
    match_id: MatchId,
    operator_id: AdminUserId,
    lease_id: OperatorLeaseId,
}

fn parse_team_arguments<I>(arguments: I) -> Result<TeamSeedArguments, CliError>
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

fn parse_player_arguments<I>(arguments: I) -> Result<PlayerSeedArguments, CliError>
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

fn parse_competition_arguments<I>(arguments: I) -> Result<CompetitionSeedArguments, CliError>
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

fn parse_season_arguments<I>(arguments: I) -> Result<SeasonSeedArguments, CliError>
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

fn parse_fixture_schedule_arguments<I>(arguments: I) -> Result<FixtureScheduleArguments, CliError>
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

fn parse_match_assignment_arguments<I>(arguments: I) -> Result<MatchAssignmentArguments, CliError>
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

fn parse_match_claim_arguments<I>(arguments: I) -> Result<MatchClaimArguments, CliError>
where
    I: IntoIterator<Item = String>,
{
    parse_match_lease_arguments(arguments, "claim")
}

fn parse_match_heartbeat_arguments<I>(arguments: I) -> Result<MatchClaimArguments, CliError>
where
    I: IntoIterator<Item = String>,
{
    parse_match_lease_arguments(arguments, "heartbeat")
}

fn parse_match_takeover_arguments<I>(arguments: I) -> Result<MatchClaimArguments, CliError>
where
    I: IntoIterator<Item = String>,
{
    parse_match_lease_arguments(arguments, "takeover")
}

fn parse_match_release_arguments<I>(arguments: I) -> Result<MatchReleaseArguments, CliError>
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

fn require_seed_authorization() -> Result<(), CliError> {
    match env::var(SEED_AUTHORIZATION_ENV).ok().as_deref() {
        Some(SEED_AUTHORIZATION_VALUE) => Ok(()),
        _ => Err(CliError::SeedNotAuthorized),
    }
}

fn require_directory_seed_authorization() -> Result<(), CliError> {
    match env::var(DIRECTORY_SEED_AUTHORIZATION_ENV).ok().as_deref() {
        Some(SEED_AUTHORIZATION_VALUE) => Ok(()),
        _ => Err(CliError::DirectorySeedNotAuthorized),
    }
}

fn require_competition_seed_authorization() -> Result<(), CliError> {
    match env::var(COMPETITION_SEED_AUTHORIZATION_ENV).ok().as_deref() {
        Some(SEED_AUTHORIZATION_VALUE) => Ok(()),
        _ => Err(CliError::CompetitionSeedNotAuthorized),
    }
}

fn require_scheduling_seed_authorization() -> Result<(), CliError> {
    match env::var(SCHEDULING_SEED_AUTHORIZATION_ENV).ok().as_deref() {
        Some(SEED_AUTHORIZATION_VALUE) => Ok(()),
        _ => Err(CliError::SchedulingSeedNotAuthorized),
    }
}

fn require_operations_seed_authorization() -> Result<(), CliError> {
    match env::var(OPERATIONS_SEED_AUTHORIZATION_ENV).ok().as_deref() {
        Some(SEED_AUTHORIZATION_VALUE) => Ok(()),
        _ => Err(CliError::OperationsSeedNotAuthorized),
    }
}

#[derive(Debug)]
enum CliError {
    Usage,
    InvalidUuid,
    UuidMustBeV7,
    SeedNotAuthorized,
    DirectorySeedNotAuthorized,
    CompetitionSeedNotAuthorized,
    SchedulingSeedNotAuthorized,
    OperationsSeedNotAuthorized,
    InvalidCompetitionName(CompetitionNameError),
    InvalidSeasonYear(SeasonYearError),
    InvalidSeasonYearValue,
    InvalidKickoff(time::error::Parse),
    InvalidCurrentTime(time::error::ComponentRange),
    InvalidLeaseSeconds,
    InvalidLeaseExpiryPolicy(OperatorLeaseExpiryPolicyError),
    InvalidFixture(FixtureError),
    InvalidAdministrator(football_application::identity::DevelopmentAdministratorError),
    InvalidPersonName(PersonNameError),
    InvalidTeamName(TeamNameError),
    Configuration(football_platform::ConfigError),
    DatabaseConnection(football_infrastructure::DatabasePoolError),
    AdminUserRepository(football_infrastructure::AdminUserRepositoryError),
    CompetitionRepository(CompetitionRepositoryError),
    FixtureRepository(FixtureRepositoryError),
    PlayerRepository(PlayerRepositoryError),
    TeamRepository(TeamRepositoryError),
    OperationsRepository(OperatorAssignmentRepositoryError),
    ExistingAdministratorConflict { id: AdminUserId },
    ReadBackMissing { id: AdminUserId },
    ReadBackMismatch { id: AdminUserId },
    SameTeamId,
    PlayerReadBackMissing { id: PlayerId, team_id: TeamId },
    PlayerReadBackMismatch { id: PlayerId, team_id: TeamId },
    TeamReadBackMissing { id: TeamId },
    TeamReadBackMismatch { id: TeamId },
    CompetitionReadBackMissing { id: CompetitionId },
    CompetitionReadBackMismatch { id: CompetitionId },
    SeasonReadBackMissing { id: SeasonId },
    SeasonReadBackMismatch { id: SeasonId },
    FixtureSeasonNotFound { id: SeasonId },
    FixtureReadBackMissing { id: FixtureId },
    FixtureReadBackMismatch { id: FixtureId },
    AssignmentReadBackMissing { id: MatchId },
    AssignmentReadBackMismatch { id: MatchId },
    LeaseReadBackMissing { id: OperatorLeaseId },
    LeaseReadBackMismatch { id: OperatorLeaseId },
    HeartbeatReadBackMissing { id: OperatorLeaseId },
    HeartbeatReadBackMismatch { id: OperatorLeaseId },
    ReleaseReadBackMismatch { id: OperatorLeaseId },
    TakeoverReadBackMissing { id: OperatorLeaseId },
    TakeoverReadBackMismatch { id: OperatorLeaseId },
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage => formatter.write_str("usage: footballctl admin seed-development ... | footballctl directory seed-teams ... | footballctl directory seed-player ... | footballctl directory import-teams ... | footballctl directory import-player ... | footballctl competition seed ... | footballctl season seed ... | footballctl fixture schedule ... | footballctl match assign ... | footballctl match claim ... | footballctl match heartbeat ... | footballctl match release ... | footballctl match takeover ..."),
            Self::InvalidUuid => formatter.write_str("--id must be a valid UUID"),
            Self::UuidMustBeV7 => formatter.write_str("--id values must be UUIDv7"),
            Self::SeedNotAuthorized => write!(
                formatter,
                "set {SEED_AUTHORIZATION_ENV}={SEED_AUTHORIZATION_VALUE} to authorize development-admin persistence"
            ),
            Self::DirectorySeedNotAuthorized => write!(
                formatter,
                "set {DIRECTORY_SEED_AUTHORIZATION_ENV}={SEED_AUTHORIZATION_VALUE} to authorize directory persistence"
            ),
            Self::CompetitionSeedNotAuthorized => write!(
                formatter,
                "set {COMPETITION_SEED_AUTHORIZATION_ENV}={SEED_AUTHORIZATION_VALUE} to authorize competition and season persistence"
            ),
            Self::SchedulingSeedNotAuthorized => write!(
                formatter,
                "set {SCHEDULING_SEED_AUTHORIZATION_ENV}={SEED_AUTHORIZATION_VALUE} to authorize fixture scheduling"
            ),
            Self::OperationsSeedNotAuthorized => write!(
                formatter,
                "set {OPERATIONS_SEED_AUTHORIZATION_ENV}={SEED_AUTHORIZATION_VALUE} to authorize match assignment and claim persistence"
            ),
            Self::InvalidCompetitionName(error) => error.fmt(formatter),
            Self::InvalidSeasonYear(error) => error.fmt(formatter),
            Self::InvalidSeasonYearValue => formatter.write_str("--year must be a positive integer"),
            Self::InvalidKickoff(error) => {
                write!(formatter, "--kickoff must be an RFC3339 timestamp with an explicit offset: {error}")
            }
            Self::InvalidCurrentTime(error) => {
                write!(formatter, "current UTC time could not be normalized: {error}")
            }
            Self::InvalidLeaseSeconds => {
                formatter.write_str("--lease-seconds must be a positive integer")
            }
            Self::InvalidLeaseExpiryPolicy(error) => error.fmt(formatter),
            Self::InvalidFixture(error) => error.fmt(formatter),
            Self::InvalidAdministrator(error) => error.fmt(formatter),
            Self::InvalidPersonName(error) => error.fmt(formatter),
            Self::InvalidTeamName(error) => error.fmt(formatter),
            Self::Configuration(error) => error.fmt(formatter),
            Self::DatabaseConnection(error) => error.fmt(formatter),
            Self::AdminUserRepository(error) => error.fmt(formatter),
            Self::CompetitionRepository(error) => error.fmt(formatter),
            Self::FixtureRepository(error) => error.fmt(formatter),
            Self::PlayerRepository(error) => error.fmt(formatter),
            Self::TeamRepository(error) => error.fmt(formatter),
            Self::OperationsRepository(error) => error.fmt(formatter),
            Self::ExistingAdministratorConflict { id } => {
                write!(formatter, "administrator {id} already exists with different data or is inactive")
            }
            Self::ReadBackMissing { id } => write!(formatter, "administrator {id} was not readable after insert"),
            Self::ReadBackMismatch { id } => write!(formatter, "administrator {id} read-back did not match insert"),
            Self::SameTeamId => formatter.write_str("the two team IDs must be different"),
            Self::PlayerReadBackMissing { id, team_id } => {
                write!(formatter, "player {id} was not readable after assignment to team {team_id}")
            }
            Self::PlayerReadBackMismatch { id, team_id } => {
                write!(formatter, "player {id} read-back did not match assignment to team {team_id}")
            }
            Self::TeamReadBackMissing { id } => {
                write!(formatter, "team {id} was not readable after insert")
            }
            Self::TeamReadBackMismatch { id } => {
                write!(formatter, "team {id} read-back did not match insert")
            }
            Self::CompetitionReadBackMissing { id } => {
                write!(formatter, "competition {id} was not readable after insert")
            }
            Self::CompetitionReadBackMismatch { id } => {
                write!(formatter, "competition {id} read-back did not match insert")
            }
            Self::SeasonReadBackMissing { id } => {
                write!(formatter, "season {id} was not readable after insert")
            }
            Self::SeasonReadBackMismatch { id } => {
                write!(formatter, "season {id} read-back did not match insert")
            }
            Self::FixtureSeasonNotFound { id } => {
                write!(formatter, "season {id} does not exist")
            }
            Self::FixtureReadBackMissing { id } => {
                write!(formatter, "fixture {id} was not readable after scheduling")
            }
            Self::FixtureReadBackMismatch { id } => {
                write!(formatter, "fixture {id} read-back did not match scheduling")
            }
            Self::AssignmentReadBackMissing { id } => {
                write!(formatter, "match assignment {id} was not readable after persistence")
            }
            Self::AssignmentReadBackMismatch { id } => {
                write!(formatter, "match assignment {id} read-back did not match persistence")
            }
            Self::LeaseReadBackMissing { id } => {
                write!(formatter, "operator lease {id} was not readable after claim")
            }
            Self::LeaseReadBackMismatch { id } => {
                write!(formatter, "operator lease {id} read-back did not match claim")
            }
            Self::HeartbeatReadBackMissing { id } => {
                write!(formatter, "operator lease {id} was not readable after heartbeat")
            }
            Self::HeartbeatReadBackMismatch { id } => {
                write!(formatter, "operator lease {id} read-back did not match heartbeat")
            }
            Self::ReleaseReadBackMismatch { id } => {
                write!(formatter, "operator lease {id} still exists after release")
            }
            Self::TakeoverReadBackMissing { id } => {
                write!(formatter, "operator lease {id} was not readable after takeover")
            }
            Self::TakeoverReadBackMismatch { id } => {
                write!(formatter, "operator lease {id} read-back did not match takeover")
            }
        }
    }
}

impl Error for CliError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidCompetitionName(error) => Some(error),
            Self::InvalidSeasonYear(error) => Some(error),
            Self::InvalidKickoff(error) => Some(error),
            Self::InvalidCurrentTime(error) => Some(error),
            Self::InvalidLeaseExpiryPolicy(error) => Some(error),
            Self::InvalidFixture(error) => Some(error),
            Self::InvalidAdministrator(error) => Some(error),
            Self::InvalidPersonName(error) => Some(error),
            Self::InvalidTeamName(error) => Some(error),
            Self::Configuration(error) => Some(error),
            Self::DatabaseConnection(error) => Some(error),
            Self::AdminUserRepository(error) => Some(error),
            Self::CompetitionRepository(error) => Some(error),
            Self::FixtureRepository(error) => Some(error),
            Self::PlayerRepository(error) => Some(error),
            Self::TeamRepository(error) => Some(error),
            Self::OperationsRepository(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CliError, OperatorLeaseExpiryPolicyError, PersonNameError, TeamNameError,
        parse_competition_arguments, parse_fixture_schedule_arguments,
        parse_match_assignment_arguments, parse_match_claim_arguments,
        parse_match_heartbeat_arguments, parse_match_release_arguments,
        parse_match_takeover_arguments, parse_player_arguments, parse_season_arguments,
        parse_seed_arguments, parse_team_arguments,
    };

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
        let parsed =
            parse_player_arguments(valid_player_arguments()).expect("valid player arguments");

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
        let parsed =
            parse_season_arguments(valid_season_arguments()).expect("valid season arguments");

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
}
