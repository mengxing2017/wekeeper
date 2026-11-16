//! Route identity for global system administrators; no sending configuration.
use super::*;
use crate::{
    business::Actor,
    source_access::{domain::routing::*, south::port::RoutingStore},
};
#[derive(Clone)]
pub struct RoutingRepository(pub PgPool);
type Tx<'a> = Transaction<'a, Postgres>;
pub(crate) async fn administrator(tx: &mut Tx<'_>, actor: Uuid) -> Result<()> {
    let admin: bool =
        sqlx::query_scalar("SELECT platform_admin FROM accounts WHERE id=$1 AND active FOR SHARE")
            .bind(actor)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(Fault("forbidden"))?;
    ensure(admin, "forbidden")
}
impl RoutingStore for RoutingRepository {
    async fn execute(&self, actor: &Actor, command: Command) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        administrator(&mut tx, actor.id).await?;
        let value = match command {
            Command::Assigned => {
                let rows:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',r.route_id,'route_name',r.route_name,'caller_name',c.unit_name,'business_unit_id',r.business_unit_id,'status',r.status) FROM source_routes r JOIN LATERAL (SELECT unit_name FROM source_credentials WHERE source_id=r.source_id ORDER BY key_id LIMIT 1) c ON true ORDER BY r.route_id").fetch_all(&mut *tx).await?;
                json!({"items":rows})
            }
            Command::Configuration { route } => {
                sqlx::query_scalar("SELECT jsonb_build_object('id',r.route_id,'business_unit_id',r.business_unit_id,'name',u.name,'status',r.status) FROM source_routes r JOIN business_units u ON u.id=r.business_unit_id WHERE r.route_id=$1").bind(&route).fetch_optional(&mut *tx).await?.ok_or(Fault("not_found"))?
            }
        };
        tx.commit().await?;
        Ok(value)
    }
}
