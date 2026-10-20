use sqlx::PgPool;

use crate::system::{north::local::SystemService, south::adapter::postgres::PostgresReadiness};

#[derive(Clone)]
pub struct Application {
    pub system: SystemService<PostgresReadiness>,
}

pub fn build_application(pool: PgPool) -> Application {
    Application {
        system: SystemService::new(PostgresReadiness::new(pool)),
    }
}
