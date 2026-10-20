use std::time::Duration;

use sqlx::PgPool;
use tracing::warn;

use crate::system::south::port::ReadinessProbe;

#[derive(Clone)]
pub struct PostgresReadiness {
    pool: PgPool,
}

impl PostgresReadiness {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl ReadinessProbe for PostgresReadiness {
    async fn is_ready(&self) -> bool {
        let probe = sqlx::query_scalar::<_, bool>(
            "SELECT schema_generation = 1 FROM application_metadata WHERE singleton = TRUE",
        )
        .fetch_optional(&self.pool);
        match tokio::time::timeout(Duration::from_secs(2), probe).await {
            Ok(Ok(Some(true))) => true,
            Ok(Err(_)) => {
                warn!("database readiness query failed");
                false
            }
            _ => false,
        }
    }
}
