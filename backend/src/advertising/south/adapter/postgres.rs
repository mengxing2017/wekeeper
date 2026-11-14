use sqlx::PgPool;
#[derive(Clone)]
pub struct Repository(pub PgPool);
pub mod decisions;
pub mod materials;
pub mod targeting;
