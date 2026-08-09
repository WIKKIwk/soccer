use football_infrastructure::{
    DatabasePoolConfig, MIGRATIONS_DIRECTORY, connect_database, run_migrations,
};
use sqlx::{PgPool, query, query_as, query_scalar};
use std::path::PathBuf;
use uuid::Uuid;

fn repository_migrations_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(MIGRATIONS_DIRECTORY)
}

fn uuid(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("match-lineup schema fixture UUID should be valid")
}

async fn seed_fixture_and_players(pool: &PgPool) -> (Uuid, Uuid, Uuid, Uuid, Uuid) {
    let competition_id = uuid("019ff800-3bc5-7cf0-b3d2-0728c077a930");
    let season_id = uuid("019ff801-3bc5-7cf0-b3d2-0728c077a930");
    let home_team_id = uuid("019ff802-3bc5-7cf0-b3d2-0728c077a930");
    let away_team_id = uuid("019ff803-3bc5-7cf0-b3d2-0728c077a930");
    let fixture_id = uuid("019ff804-3bc5-7cf0-b3d2-0728c077a930");
    let home_player_id = uuid("019ff805-3bc5-7cf0-b3d2-0728c077a930");
    let away_player_id = uuid("019ff806-3bc5-7cf0-b3d2-0728c077a930");

    query("INSERT INTO competitions (id, name) VALUES ($1, $2)")
        .bind(competition_id)
        .bind("O‘zbekiston Superligasi")
        .execute(pool)
        .await
        .expect("lineup schema competition should insert");
    query("INSERT INTO seasons (id, competition_id, year) VALUES ($1, $2, $3)")
        .bind(season_id)
        .bind(competition_id)
        .bind(2026_i32)
        .execute(pool)
        .await
        .expect("lineup schema season should insert");
    for (team_id, name) in [(home_team_id, "Lineup Home"), (away_team_id, "Lineup Away")] {
        query("INSERT INTO teams (id, name) VALUES ($1, $2)")
            .bind(team_id)
            .bind(name)
            .execute(pool)
            .await
            .expect("lineup schema team should insert");
    }
    query(
        "INSERT INTO fixtures (id, competition_id, season_id, home_team_id, away_team_id, kickoff_at) \
         VALUES ($1, $2, $3, $4, $5, '2026-08-09T12:00:00Z')",
    )
    .bind(fixture_id)
    .bind(competition_id)
    .bind(season_id)
    .bind(home_team_id)
    .bind(away_team_id)
    .execute(pool)
    .await
    .expect("lineup schema fixture should insert");

    for (person_id, player_id, name) in [
        (
            uuid("019ff807-3bc5-7cf0-b3d2-0728c077a930"),
            home_player_id,
            "Lineup Home Player",
        ),
        (
            uuid("019ff808-3bc5-7cf0-b3d2-0728c077a930"),
            away_player_id,
            "Lineup Away Player",
        ),
    ] {
        query("INSERT INTO persons (id, name) VALUES ($1, $2)")
            .bind(person_id)
            .bind(name)
            .execute(pool)
            .await
            .expect("lineup schema person should insert");
        query("INSERT INTO players (id, person_id) VALUES ($1, $2)")
            .bind(player_id)
            .bind(person_id)
            .execute(pool)
            .await
            .expect("lineup schema player should insert");
    }

    (
        fixture_id,
        home_player_id,
        away_player_id,
        uuid("019ff809-3bc5-7cf0-b3d2-0728c077a930"),
        uuid("019ff80a-3bc5-7cf0-b3d2-0728c077a930"),
    )
}

async fn assert_sqlstate<F>(operation: F, expected_code: &str)
where
    F: std::future::Future<Output = Result<sqlx::postgres::PgQueryResult, sqlx::Error>>,
{
    let error = operation
        .await
        .expect_err("schema invariant should reject the write");
    let code = match error {
        sqlx::Error::Database(database_error) => database_error.code().map(|code| code.to_string()),
        other => panic!("expected a PostgreSQL constraint error, got {other:?}"),
    };
    assert_eq!(code.as_deref(), Some(expected_code));
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn match_lineup_projection_schema_enforces_match_side_status_and_player_roles() {
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
        .expect("clean match-lineup schema target should accept a PostgreSQL connection");
    let history_table_exists: bool =
        query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
            .fetch_one(&pool)
            .await
            .expect("migration history preflight query should succeed");
    assert!(
        !history_table_exists,
        "match-lineup schema target must be clean"
    );

    run_migrations(&pool, repository_migrations_path())
        .await
        .expect("all repository migrations should apply before schema assertions");

    let (fixture_id, home_player_id, away_player_id, match_id, second_match_id) =
        seed_fixture_and_players(&pool).await;
    let second_fixture_id = uuid("019ff80c-3bc5-7cf0-b3d2-0728c077a930");
    query(
        "INSERT INTO fixtures (id, competition_id, season_id, home_team_id, away_team_id, kickoff_at) \
         SELECT $1, competition_id, season_id, home_team_id, away_team_id, kickoff_at + INTERVAL '1 day' \
         FROM fixtures WHERE id = $2",
    )
    .bind(second_fixture_id)
    .bind(fixture_id)
    .execute(&pool)
    .await
    .expect("second lineup schema fixture should insert");
    query("INSERT INTO match_lineup_projections (match_id, fixture_id) VALUES ($1, $2)")
        .bind(match_id)
        .bind(fixture_id)
        .execute(&pool)
        .await
        .expect("valid match lineup projection should insert");
    query(
        "INSERT INTO match_lineup_team_projections (match_id, side, lineup_status) \
         VALUES ($1, 'home', 'provisional'), ($1, 'away', 'confirmed')",
    )
    .bind(match_id)
    .execute(&pool)
    .await
    .expect("valid home and away lineup states should insert");
    query(
        "INSERT INTO match_lineup_entry_projections (match_id, side, player_id, role) \
         VALUES ($1, 'home', $2, 'starter'), ($1, 'away', $3, 'substitute')",
    )
    .bind(match_id)
    .bind(home_player_id)
    .bind(away_player_id)
    .execute(&pool)
    .await
    .expect("valid lineup player roles should insert");
    query("INSERT INTO match_lineup_projections (match_id, fixture_id) VALUES ($1, $2)")
        .bind(second_match_id)
        .bind(second_fixture_id)
        .execute(&pool)
        .await
        .expect("second match lineup projection should insert");

    let rows: Vec<(String, String, Uuid)> = query_as(
        "SELECT team.side, team.lineup_status, entry.player_id \
         FROM match_lineup_team_projections AS team \
         JOIN match_lineup_entry_projections AS entry \
           ON entry.match_id = team.match_id AND entry.side = team.side \
         WHERE team.match_id = $1 ORDER BY team.side",
    )
    .bind(match_id)
    .fetch_all(&pool)
    .await
    .expect("lineup projection rows should be readable");
    assert_eq!(
        rows,
        vec![
            ("away".to_owned(), "confirmed".to_owned(), away_player_id),
            ("home".to_owned(), "provisional".to_owned(), home_player_id),
        ]
    );

    assert_sqlstate(
        query("INSERT INTO match_lineup_projections (match_id, fixture_id) VALUES ($1, $2)")
            .bind(uuid("019ff80d-3bc5-7cf0-b3d2-0728c077a930"))
            .bind(fixture_id)
            .execute(&pool),
        "23505",
    )
    .await;

    assert_sqlstate(
        query(
            "INSERT INTO match_lineup_team_projections (match_id, side) \
             VALUES ($1, 'home')",
        )
        .bind(uuid("019ff80e-3bc5-7cf0-b3d2-0728c077a930"))
        .execute(&pool),
        "23503",
    )
    .await;

    assert_sqlstate(
        query(
            "INSERT INTO match_lineup_team_projections (match_id, side) \
             VALUES ($1, 'neutral')",
        )
        .bind(match_id)
        .execute(&pool),
        "23514",
    )
    .await;

    assert_sqlstate(
        query(
            "INSERT INTO match_lineup_team_projections (match_id, side, lineup_status) \
             VALUES ($1, 'home', 'draft')",
        )
        .bind(second_match_id)
        .execute(&pool),
        "23514",
    )
    .await;

    assert_sqlstate(
        query(
            "INSERT INTO match_lineup_entry_projections (match_id, side, player_id, role) \
             VALUES ($1, 'home', $2, 'starter')",
        )
        .bind(match_id)
        .bind(home_player_id)
        .execute(&pool),
        "23505",
    )
    .await;

    assert_sqlstate(
        query(
            "INSERT INTO match_lineup_entry_projections (match_id, side, player_id, role) \
             VALUES ($1, 'home', $2, 'starter')",
        )
        .bind(match_id)
        .bind(uuid("019ff80b-3bc5-7cf0-b3d2-0728c077a930"))
        .execute(&pool),
        "23503",
    )
    .await;

    assert_sqlstate(
        query(
            "INSERT INTO match_lineup_entry_projections (match_id, side, player_id, role) \
             VALUES ($1, 'home', $2, 'captain')",
        )
        .bind(match_id)
        .bind(away_player_id)
        .execute(&pool),
        "23514",
    )
    .await;

    assert_sqlstate(
        query(
            "INSERT INTO match_lineup_entry_projections (match_id, side, player_id, role) \
             VALUES ($1, 'away', $2, 'starter')",
        )
        .bind(match_id)
        .bind(home_player_id)
        .execute(&pool),
        "23505",
    )
    .await;

    query("DELETE FROM match_lineup_projections WHERE match_id = $1")
        .bind(match_id)
        .execute(&pool)
        .await
        .expect("projection root should be deletable for rebuild semantics");
    let remaining_team_rows: i64 =
        query_scalar("SELECT COUNT(*) FROM match_lineup_team_projections WHERE match_id = $1")
            .bind(match_id)
            .fetch_one(&pool)
            .await
            .expect("cascaded team projection count should be readable");
    let remaining_entry_rows: i64 =
        query_scalar("SELECT COUNT(*) FROM match_lineup_entry_projections WHERE match_id = $1")
            .bind(match_id)
            .fetch_one(&pool)
            .await
            .expect("cascaded entry projection count should be readable");
    assert_eq!(remaining_team_rows, 0);
    assert_eq!(remaining_entry_rows, 0);

    pool.close().await;
}
