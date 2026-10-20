use sqlx::{PgPool, migrate::Migrator};

pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    MIGRATOR.run(pool).await
}

impl From<sqlx::Error> for crate::business::Error {
    fn from(e: sqlx::Error) -> Self {
        if e.as_database_error()
            .is_some_and(|x| x.is_unique_violation())
        {
            Self::Conflict
        } else {
            Self::Database(e.to_string())
        }
    }
}
