use crate::system::{north::local::SystemService, south::adapter::postgres::PostgresReadiness};
use sqlx::PgPool;
#[derive(Clone)]
pub struct Options {
    pub public_origin: String,
    pub insecure_local_auth: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            public_origin: std::env::var("WEKEEPER_PUBLIC_ORIGIN")
                .unwrap_or_else(|_| "http://localhost:5173".into()),
            insecure_local_auth: std::env::var("WEKEEPER_INSECURE_LOCAL_AUTH")
                .is_ok_and(|v| v == "true"),
        }
    }
}
#[derive(Clone)]
pub struct Application {
    pub system: SystemService<PostgresReadiness>,
    pub pool: PgPool,
    pub legacy: crate::legacy::south::adapter::postgres::Repository,
    pub options: Options,
    pub identity: crate::identity::north::local::Service,
    pub schools: crate::schools::north::local::Service,
    pub community: crate::community::north::local::Service,
    pub distribution: crate::distribution::north::local::Service,
    pub integrations: crate::integrations::north::local::Service,
    pub media: crate::media::north::local::Service,
}
pub fn build_application(pool: PgPool) -> Application {
    Application {
        legacy: crate::legacy::south::adapter::postgres::Repository(pool.clone()),
        system: SystemService::new(PostgresReadiness::new(pool.clone())),
        options: Options::default(),
        identity: crate::identity::north::local::Service(
            crate::identity::south::adapter::postgres::Repository(pool.clone()),
        ),
        schools: crate::schools::north::local::Service(
            crate::schools::south::adapter::postgres::Repository(pool.clone()),
        ),
        community: crate::community::north::local::Service(
            crate::community::south::adapter::postgres::Repository(pool.clone()),
        ),
        distribution: crate::distribution::north::local::Service(
            crate::distribution::south::adapter::postgres::Repository(pool.clone()),
        ),
        integrations: crate::integrations::north::local::Service::new(pool.clone()),
        media: crate::media::north::local::Service(
            crate::media::south::adapter::postgres::Repository(pool.clone()),
        ),
        pool,
    }
}
