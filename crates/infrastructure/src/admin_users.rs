use football_application::identity::DevelopmentAdministrator;
use football_domain::AdminUserId;
use sqlx::{Error as SqlxError, PgPool};
use std::fmt;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedAdminUser {
    id: AdminUserId,
    username: String,
    display_name: String,
    is_active: bool,
}

impl PersistedAdminUser {
    pub const fn id(&self) -> AdminUserId {
        self.id
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    pub const fn is_active(&self) -> bool {
        self.is_active
    }
}

#[derive(Debug)]
pub enum AdminUserRepositoryError {
    Database(SqlxError),
}

impl fmt::Display for AdminUserRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => write!(
                formatter,
                "administrator database operation failed: {error}"
            ),
        }
    }
}

impl std::error::Error for AdminUserRepositoryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct AdminUserRow {
    id: Uuid,
    username: String,
    display_name: String,
    is_active: bool,
}

impl From<AdminUserRow> for PersistedAdminUser {
    fn from(row: AdminUserRow) -> Self {
        Self {
            id: AdminUserId::from_uuid(row.id),
            username: row.username,
            display_name: row.display_name,
            is_active: row.is_active,
        }
    }
}

pub async fn persist_active_admin(
    pool: &PgPool,
    administrator: &DevelopmentAdministrator,
) -> Result<PersistedAdminUser, AdminUserRepositoryError> {
    sqlx::query_as::<_, AdminUserRow>(
        r#"
        INSERT INTO admin_users (id, username, display_name, is_active)
        VALUES ($1, $2, $3, TRUE)
        RETURNING id, username, display_name, is_active
        "#,
    )
    .bind(administrator.id().as_uuid())
    .bind(administrator.username())
    .bind(administrator.display_name())
    .fetch_one(pool)
    .await
    .map(AdminUserRow::into)
    .map_err(AdminUserRepositoryError::Database)
}

pub async fn load_admin_user(
    pool: &PgPool,
    id: AdminUserId,
) -> Result<Option<PersistedAdminUser>, AdminUserRepositoryError> {
    sqlx::query_as::<_, AdminUserRow>(
        r#"
        SELECT id, username, display_name, is_active
        FROM admin_users
        WHERE id = $1
        "#,
    )
    .bind(id.as_uuid())
    .fetch_optional(pool)
    .await
    .map(|row| row.map(AdminUserRow::into))
    .map_err(AdminUserRepositoryError::Database)
}
