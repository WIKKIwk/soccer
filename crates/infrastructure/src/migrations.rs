use sqlx::{
    migrate::{MigrateError, Migrator},
    postgres::PgPool,
};
use std::path::Path;

/// Repository-relative directory containing ordered forward-only SQL migrations.
pub const MIGRATIONS_DIRECTORY: &str = "migrations";

/// Resolve the checked-in migration set from an explicit directory.
pub async fn load_migrator(source: impl AsRef<Path>) -> Result<Migrator, MigrateError> {
    Migrator::new(source.as_ref()).await
}

/// Apply the resolved migration set to an already-created PostgreSQL pool.
///
/// The caller owns when this operation is run. It is intentionally not invoked
/// from request handlers or implicitly during pool creation.
pub async fn run_migrations(pool: &PgPool, source: impl AsRef<Path>) -> Result<(), MigrateError> {
    let migrator = load_migrator(source).await?;
    migrator.run(pool).await
}

#[cfg(test)]
mod tests {
    use super::{MIGRATIONS_DIRECTORY, load_migrator};
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[tokio::test]
    async fn resolves_repository_migrations_directory() {
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(MIGRATIONS_DIRECTORY);

        load_migrator(source)
            .await
            .expect("repository migration directory should be resolvable");
    }

    #[tokio::test]
    async fn resolves_positive_versions_in_order_without_a_database() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_nanos();
        let source = std::env::temp_dir().join(format!(
            "football-infrastructure-migrations-{}-{suffix}",
            std::process::id()
        ));

        fs::create_dir(&source).expect("temporary migration directory should be creatable");
        fs::write(source.join("0002_second.sql"), "SELECT 1;")
            .expect("second migration fixture should be writable");
        fs::write(source.join("0001_first.sql"), "SELECT 1;")
            .expect("first migration fixture should be writable");

        let migrator = load_migrator(&source)
            .await
            .expect("valid migration filenames should resolve");
        let versions: Vec<_> = migrator.iter().map(|migration| migration.version).collect();

        assert_eq!(versions, vec![1, 2]);
        fs::remove_dir_all(source).expect("temporary migration directory should be removable");
    }
}
