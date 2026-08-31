use football_domain::{
    Competition, CompetitionId, CompetitionName, DraftLineup, Fixture, FixtureId, LineupEntry,
    LineupId, LineupValidationError, Person, PersonId, PersonName, Player, PlayerId, Season,
    SeasonId, SeasonYear, Team, TeamId, TeamName, UtcInstant,
};
use football_infrastructure::{
    DatabasePoolConfig, LineupRepositoryError, MIGRATIONS_DIRECTORY, connect_database,
    load_draft_lineup, load_fixture, persist_competition, persist_draft_lineup,
    persist_player_for_team, persist_season, persist_two_teams, run_migrations,
};
use std::path::PathBuf;
use uuid::Uuid;

fn repository_migrations_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(MIGRATIONS_DIRECTORY)
}

fn fixture_uuid(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("deterministic lineup fixture UUID should parse")
}

fn player_fixture(player_id: &str, person_id: &str, name: &str) -> (Person, Player) {
    let person_id = PersonId::from_uuid(fixture_uuid(person_id));
    let person = Person::new(
        person_id,
        PersonName::try_new(name).expect("deterministic player name should be valid"),
    );
    let player = Player::new(PlayerId::from_uuid(fixture_uuid(player_id)), person_id);
    (person, player)
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn persists_and_loads_a_draft_lineup_with_ownership_and_identity_guards() {
    let confirmation = std::env::var("ALLOW_CLEAN_DATABASE_MIGRATION")
        .expect("set ALLOW_CLEAN_DATABASE_MIGRATION=1 to authorize this database operation");
    assert_eq!(
        confirmation, "1",
        "set ALLOW_CLEAN_DATABASE_MIGRATION=1 to authorize this database operation"
    );

    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must point to an explicitly approved clean database");
    let migration_pool = connect_database(&database_url, DatabasePoolConfig::default())
        .await
        .expect("clean lineup target should accept a PostgreSQL connection");
    run_migrations(&migration_pool, repository_migrations_path())
        .await
        .expect("all migrations should apply to the clean lineup target");
    migration_pool.close().await;

    let pool = connect_database(&database_url, DatabasePoolConfig::default())
        .await
        .expect("lineup verification target should accept a PostgreSQL connection");

    let competition = Competition::new(
        CompetitionId::from_uuid(fixture_uuid("019fe3a0-3bc5-7cf0-b3d2-0728c077a930")),
        CompetitionName::try_new("O‘zbekiston Superligasi")
            .expect("deterministic competition name should be valid"),
    );
    persist_competition(&pool, &competition)
        .await
        .expect("test competition should persist");

    let season = Season::new(
        SeasonId::from_uuid(fixture_uuid("019fe3a0-3bc5-7cf1-b3d2-0728c077a930")),
        competition.id(),
        SeasonYear::try_new(2026).expect("test season year should be valid"),
    );
    persist_season(&pool, season)
        .await
        .expect("test season should persist");

    let home_team = Team::new(
        TeamId::from_uuid(fixture_uuid("019fe3a0-3bc5-7cf2-b3d2-0728c077a930")),
        TeamName::try_new("Andijon").expect("source-backed home team name should be valid"),
    );
    let away_team = Team::new(
        TeamId::from_uuid(fixture_uuid("019fe3a0-3bc5-7cf3-b3d2-0728c077a930")),
        TeamName::try_new("Buxoro").expect("source-backed away team name should be valid"),
    );
    persist_two_teams(&pool, &home_team, &away_team)
        .await
        .expect("test fixture teams should persist");

    let (home_starter_person, home_starter) = player_fixture(
        "019fe3a1-3bc5-7cf0-b3d2-0728c077a930",
        "019fe3a1-3bc5-7cf1-b3d2-0728c077a930",
        "Odil Abjumajidov",
    );
    let (home_substitute_person, home_substitute) = player_fixture(
        "019fe3a2-3bc5-7cf0-b3d2-0728c077a930",
        "019fe3a2-3bc5-7cf1-b3d2-0728c077a930",
        "Hamidullo Karimov",
    );
    let (home_replacement_person, home_replacement) = player_fixture(
        "019fe3a3-3bc5-7cf0-b3d2-0728c077a930",
        "019fe3a3-3bc5-7cf1-b3d2-0728c077a930",
        "LIN-003 Home Replacement",
    );
    let (away_player_person, away_player) = player_fixture(
        "019fe3a4-3bc5-7cf0-b3d2-0728c077a930",
        "019fe3a4-3bc5-7cf1-b3d2-0728c077a930",
        "Buxoro Draft Player",
    );
    persist_player_for_team(&pool, &home_starter_person, home_starter, home_team.id())
        .await
        .expect("home starter assignment should persist");
    persist_player_for_team(
        &pool,
        &home_substitute_person,
        home_substitute,
        home_team.id(),
    )
    .await
    .expect("home substitute assignment should persist");
    persist_player_for_team(
        &pool,
        &home_replacement_person,
        home_replacement,
        home_team.id(),
    )
    .await
    .expect("home replacement assignment should persist");
    persist_player_for_team(&pool, &away_player_person, away_player, away_team.id())
        .await
        .expect("away lineup player assignment should persist");
    let fixture = Fixture::try_new(
        FixtureId::from_uuid(fixture_uuid("019fe3b0-3bc5-7cf0-b3d2-0728c077a930")),
        season,
        home_team.id(),
        away_team.id(),
        UtcInstant::from_unix_timestamp(1_785_164_400)
            .expect("source-backed fixture kickoff should be representable"),
    )
    .expect("distinct fixture teams should be accepted");

    let lineup = DraftLineup::try_new(
        LineupId::from_uuid(fixture_uuid("019fe3b0-3bc5-7cf1-b3d2-0728c077a930")),
        fixture.id(),
        home_team.id(),
        vec![
            LineupEntry::substitute(home_substitute.id()),
            LineupEntry::starter(home_starter.id()),
        ],
    )
    .expect("distinct assigned players should create a draft lineup");

    let persisted = persist_draft_lineup(&pool, &fixture, &lineup)
        .await
        .expect("fixture and draft lineup should persist together");
    assert_eq!(persisted, lineup);

    let persisted_fixture = load_fixture(&pool, fixture.id())
        .await
        .expect("persisted fixture should be readable")
        .expect("persisted fixture should exist");
    assert_eq!(persisted_fixture, fixture);

    let loaded = load_draft_lineup(&pool, lineup.id())
        .await
        .expect("persisted draft lineup should be readable")
        .expect("persisted draft lineup should exist");
    assert_eq!(loaded.id(), lineup.id());
    assert_eq!(loaded.fixture_id(), fixture.id());
    assert_eq!(loaded.team_id(), home_team.id());
    assert_eq!(
        loaded.entries(),
        &[
            LineupEntry::starter(home_starter.id()),
            LineupEntry::substitute(home_substitute.id()),
        ]
    );

    let repeated = persist_draft_lineup(&pool, &fixture, &lineup)
        .await
        .expect("an exact draft lineup repeat should be idempotent");
    assert_eq!(repeated, lineup);
    let repeated_loaded = load_draft_lineup(&pool, lineup.id())
        .await
        .expect("repeated draft lineup should remain readable")
        .expect("repeated draft lineup should still exist");
    assert_eq!(
        repeated_loaded.entries(),
        &[
            LineupEntry::starter(home_starter.id()),
            LineupEntry::substitute(home_substitute.id()),
        ]
    );

    let wrong_team_id = TeamId::from_uuid(fixture_uuid("019fe3b0-3bc5-7cf1-b3d2-0728c077a930"));
    let wrong_team_lineup = DraftLineup::try_new(
        LineupId::from_uuid(fixture_uuid("019fe3b0-3bc5-7cf2-b3d2-0728c077a930")),
        fixture.id(),
        wrong_team_id,
        Vec::new(),
    )
    .expect("an ownership fixture may use an empty entry collection");
    let error = persist_draft_lineup(&pool, &fixture, &wrong_team_lineup)
        .await
        .expect_err("a lineup for a third team should be rejected");
    assert!(matches!(
        error,
        LineupRepositoryError::FixtureTeamMismatch {
            fixture_id,
            team_id
        } if fixture_id == fixture.id() && team_id == wrong_team_id
    ));

    let other_fixture_id =
        FixtureId::from_uuid(fixture_uuid("019fe3b0-3bc5-7cf3-b3d2-0728c077a930"));
    let mismatched_fixture_lineup = DraftLineup::try_new(
        LineupId::from_uuid(fixture_uuid("019fe3b0-3bc5-7cf4-b3d2-0728c077a930")),
        other_fixture_id,
        home_team.id(),
        Vec::new(),
    )
    .expect("an ownership fixture may use an empty entry collection");
    let error = persist_draft_lineup(&pool, &fixture, &mismatched_fixture_lineup)
        .await
        .expect_err("a lineup owned by another fixture should be rejected");
    assert!(matches!(
        error,
        LineupRepositoryError::LineupFixtureMismatch {
            lineup_id,
            expected,
            actual
        } if lineup_id == mismatched_fixture_lineup.id()
            && expected == fixture.id()
            && actual == other_fixture_id
    ));

    let unassigned_player_lineup = DraftLineup::try_new(
        LineupId::from_uuid(fixture_uuid("019fe3b0-3bc5-7cf5-b3d2-0728c077a930")),
        fixture.id(),
        away_team.id(),
        vec![LineupEntry::starter(home_starter.id())],
    )
    .expect("a player assigned to the other team is a valid draft input");
    let error = persist_draft_lineup(&pool, &fixture, &unassigned_player_lineup)
        .await
        .expect_err("a player not assigned to the lineup team should be rejected");
    assert!(matches!(
        error,
        LineupRepositoryError::PlayerNotAssignedToTeam {
            player_id,
            team_id
        } if player_id == home_starter.id() && team_id == away_team.id()
    ));

    let away_lineup = DraftLineup::try_new(
        LineupId::from_uuid(fixture_uuid("019fe3b0-3bc5-7cf8-b3d2-0728c077a930")),
        fixture.id(),
        away_team.id(),
        vec![LineupEntry::starter(away_player.id())],
    )
    .expect("a valid away-side draft lineup should be accepted");
    assert_eq!(
        persist_draft_lineup(&pool, &fixture, &away_lineup)
            .await
            .expect("the second fixture-side lineup should persist"),
        away_lineup
    );
    assert_eq!(
        load_draft_lineup(&pool, away_lineup.id())
            .await
            .expect("the second fixture-side lineup should load")
            .expect("the second fixture-side lineup should exist"),
        away_lineup
    );

    let duplicate_lineup_id =
        LineupId::from_uuid(fixture_uuid("019fe3b0-3bc5-7cf6-b3d2-0728c077a930"));
    assert_eq!(
        DraftLineup::try_new(
            duplicate_lineup_id,
            fixture.id(),
            home_team.id(),
            vec![
                LineupEntry::starter(home_starter.id()),
                LineupEntry::substitute(home_starter.id()),
            ],
        ),
        Err(LineupValidationError::DuplicatePlayer {
            player_id: home_starter.id(),
        })
    );

    let changed_fixture = Fixture::try_new(
        fixture.id(),
        season,
        home_team.id(),
        away_team.id(),
        UtcInstant::from_unix_timestamp(1_786_003_600)
            .expect("changed fixture kickoff should be representable"),
    )
    .expect("changed fixture teams should remain valid");
    let error = persist_draft_lineup(&pool, &changed_fixture, &lineup)
        .await
        .expect_err("changing an existing fixture identity should conflict");
    assert!(matches!(
        error,
        LineupRepositoryError::ExistingFixtureConflict { id } if id == fixture.id()
    ));

    let changed_lineup = DraftLineup::try_new(
        lineup.id(),
        fixture.id(),
        home_team.id(),
        vec![
            LineupEntry::starter(home_replacement.id()),
            LineupEntry::substitute(home_substitute.id()),
        ],
    )
    .expect("changed assigned players should create a valid draft lineup");
    let error = persist_draft_lineup(&pool, &fixture, &changed_lineup)
        .await
        .expect_err("changing an existing lineup identity should conflict");
    assert!(matches!(
        error,
        LineupRepositoryError::ExistingLineupConflict { id } if id == lineup.id()
    ));

    let duplicate_team_lineup = DraftLineup::try_new(
        LineupId::from_uuid(fixture_uuid("019fe3b0-3bc5-7cf7-b3d2-0728c077a930")),
        fixture.id(),
        home_team.id(),
        lineup.entries().to_vec(),
    )
    .expect("a second lineup identity for the same fixture side is structurally valid");
    let error = persist_draft_lineup(&pool, &fixture, &duplicate_team_lineup)
        .await
        .expect_err("a fixture side should not accept a second lineup identity");
    assert!(matches!(
        error,
        LineupRepositoryError::ExistingFixtureTeamLineupConflict {
            fixture_id,
            team_id,
            existing_id
        } if fixture_id == fixture.id()
            && team_id == home_team.id()
            && existing_id == lineup.id()
    ));

    let final_loaded = load_draft_lineup(&pool, lineup.id())
        .await
        .expect("conflict checks should leave the original lineup readable")
        .expect("original lineup should still exist");
    assert_eq!(
        final_loaded.entries(),
        &[
            LineupEntry::starter(home_starter.id()),
            LineupEntry::substitute(home_substitute.id()),
        ]
    );
    assert_eq!(
        load_fixture(&pool, fixture.id())
            .await
            .expect("conflict checks should leave the original fixture readable")
            .expect("original fixture should still exist"),
        fixture
    );

    pool.close().await;
}
