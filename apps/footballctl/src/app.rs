use crate::{
    arguments::{
        parse_competition_arguments, parse_fixture_schedule_arguments,
        parse_match_assignment_arguments, parse_match_claim_arguments,
        parse_match_heartbeat_arguments, parse_match_release_arguments,
        parse_match_takeover_arguments, parse_player_arguments, parse_season_arguments,
        parse_seed_arguments, parse_team_arguments,
    },
    auth::{self, AuthorizationScope},
    config::CliRuntimeConfig,
    error::CliError,
    live_commands,
};
use football_domain::{Fixture, OperatorLeaseExpiryPolicy, UtcInstant};
use football_infrastructure::{
    DatabasePool, claim_operator_lease, heartbeat_operator_lease, load_admin_user,
    load_competition, load_fixture, load_match_assignment, load_operator_lease,
    load_player_assignment, load_season, load_team, persist_active_admin, persist_competition,
    persist_fixture, persist_match_assignment, persist_player_for_team, persist_season,
    persist_two_teams, release_operator_lease, takeover_operator_lease,
};

pub(crate) async fn run<I>(arguments: I) -> Result<(), CliError>
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
        Some("lineup") => live_commands::run_lineup(arguments).await,
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
        Some(
            "schedule" | "lineup-submit" | "lineup-confirm" | "start-first-half" | "end-first-half"
            | "start-second-half" | "finish" | "goal" | "card" | "substitution",
        ) => live_commands::run_match(arguments).await,
        _ => Err(CliError::Usage),
    }
}

async fn run_admin_seed(arguments: Vec<String>) -> Result<(), CliError> {
    let seed = parse_seed_arguments(arguments)?;
    require_authorization(AuthorizationScope::DevelopmentAdmin)?;
    let pool = connect_configured_database().await?;

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
    require_authorization(AuthorizationScope::Directory)?;
    let pool = connect_configured_database().await?;

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
    require_authorization(AuthorizationScope::Directory)?;
    let pool = connect_configured_database().await?;

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
    require_authorization(AuthorizationScope::Competition)?;
    let pool = connect_configured_database().await?;

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
    require_authorization(AuthorizationScope::Competition)?;
    let pool = connect_configured_database().await?;

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
    require_authorization(AuthorizationScope::Scheduling)?;
    let pool = connect_configured_database().await?;

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
    require_authorization(AuthorizationScope::Operations)?;
    let pool = connect_configured_database().await?;

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
    require_authorization(AuthorizationScope::Operations)?;
    let claimed_at = current_utc_instant()?;
    let expires_at = OperatorLeaseExpiryPolicy::expiry_at(claimed_at, claim.lease_seconds)
        .map_err(CliError::InvalidLeaseExpiryPolicy)?;
    let pool = connect_configured_database().await?;

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
    require_authorization(AuthorizationScope::Operations)?;
    let heartbeat_at = current_utc_instant()?;
    let expires_at = OperatorLeaseExpiryPolicy::expiry_at(heartbeat_at, heartbeat.lease_seconds)
        .map_err(CliError::InvalidLeaseExpiryPolicy)?;
    let pool = connect_configured_database().await?;

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
    require_authorization(AuthorizationScope::Operations)?;
    let pool = connect_configured_database().await?;

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
    require_authorization(AuthorizationScope::Operations)?;
    let claimed_at = current_utc_instant()?;
    let expires_at = OperatorLeaseExpiryPolicy::expiry_at(claimed_at, takeover.lease_seconds)
        .map_err(CliError::InvalidLeaseExpiryPolicy)?;
    let pool = connect_configured_database().await?;

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

fn require_authorization(scope: AuthorizationScope) -> Result<(), CliError> {
    auth::require(scope).map_err(CliError::from)
}

async fn connect_configured_database() -> Result<DatabasePool, CliError> {
    let config = CliRuntimeConfig::from_env().map_err(CliError::Configuration)?;
    config
        .connect_database()
        .await
        .map_err(CliError::DatabaseConnection)
}

fn current_utc_instant() -> Result<UtcInstant, CliError> {
    let now = time::OffsetDateTime::now_utc();
    let microsecond_precision = (now.nanosecond() / 1_000) * 1_000;
    let now = now
        .replace_nanosecond(microsecond_precision)
        .map_err(CliError::InvalidCurrentTime)?;
    Ok(UtcInstant::from_offset_datetime(now))
}
