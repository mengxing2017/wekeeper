pub mod postgres;
impl From<sqlx::Error> for crate::source_access::domain::Fault {
    fn from(error: sqlx::Error) -> Self {
        if let Some(database) = error.as_database_error() {
            let code = database.code().map(|c| c.into_owned()).unwrap_or_default();
            // Schema/query diagnostics contain identifiers, not failed-row values.
            let schema_message = if code.starts_with("42") {
                database.message()
            } else {
                ""
            };
            tracing::error!(sqlstate=%code,table=database.table(),constraint=database.constraint(),schema_message,"source access database operation failed");
        } else {
            let reason = match error {
                sqlx::Error::PoolTimedOut => "pool_timed_out",
                sqlx::Error::PoolClosed => "pool_closed",
                sqlx::Error::Io(_) => "connection_io",
                _ => "database_operation_failed",
            };
            tracing::error!(reason, "source access database operation failed");
        }
        Self("temporarily_unavailable")
    }
}
