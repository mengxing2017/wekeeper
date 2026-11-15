//! Route identity and explicit management access; no sending configuration.
use super::*;
use crate::{
    business::Actor,
    source_access::{domain::routing::*, south::port::RoutingStore},
};
#[derive(Clone)]
pub struct RoutingRepository(pub PgPool);
type Tx<'a> = Transaction<'a, Postgres>;
pub(crate) async fn administrator(tx: &mut Tx<'_>, actor: Uuid) -> Result<bool> {
    sqlx::query_scalar("SELECT platform_admin FROM accounts WHERE id=$1 AND active FOR SHARE")
        .bind(actor)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(Fault("forbidden"))
}
pub(crate) async fn access(tx: &mut Tx<'_>, actor: Uuid, route: &str, admin: bool) -> Result<()> {
    ensure(admin || sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM source_route_managers WHERE route_id=$1 AND account_id=$2)").bind(route).bind(actor).fetch_one(&mut **tx).await?,"forbidden")
}
impl RoutingStore for RoutingRepository {
    async fn execute(&self, actor: &Actor, command: Command) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        let admin = administrator(&mut tx, actor.id).await?;
        let value = match command {
            Command::Assigned => {
                let rows:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',r.route_id,'route_name',r.route_name,'caller_name',c.unit_name,'business_unit_id',r.business_unit_id,'status',r.status) FROM source_routes r JOIN LATERAL (SELECT unit_name FROM source_credentials WHERE source_id=r.source_id ORDER BY key_id LIMIT 1) c ON true WHERE $2 OR EXISTS(SELECT 1 FROM source_route_managers m WHERE m.route_id=r.route_id AND m.account_id=$1) ORDER BY r.route_id").bind(actor.id).bind(admin).fetch_all(&mut *tx).await?;
                json!({"items":rows})
            }
            Command::Configuration { route } | Command::Managers { route } => {
                access(&mut tx, actor.id, &route, admin).await?;
                let mut row:Value=sqlx::query_scalar("SELECT jsonb_build_object('id',r.route_id,'business_unit_id',r.business_unit_id,'name',u.name,'status',r.status) FROM source_routes r JOIN business_units u ON u.id=r.business_unit_id WHERE r.route_id=$1").bind(&route).fetch_optional(&mut *tx).await?.ok_or(Fault("not_found"))?;
                row["managers"]=json!(sqlx::query_scalar::<_,Value>("SELECT jsonb_build_object('id',a.id,'username',a.username) FROM source_route_managers m JOIN accounts a ON a.id=m.account_id WHERE m.route_id=$1 ORDER BY a.username").bind(&route).fetch_all(&mut *tx).await?);
                row
            }
            Command::Manager { route, input } => {
                ensure(admin && !input.username.trim().is_empty(), "forbidden")?;
                let account: Uuid =
                    sqlx::query_scalar("SELECT id FROM accounts WHERE username=$1 AND active")
                        .bind(&input.username)
                        .fetch_optional(&mut *tx)
                        .await?
                        .ok_or(Fault("not_found"))?;
                if input.granted {
                    sqlx::query("INSERT INTO source_route_managers(route_id,account_id,granted_by) VALUES($1,$2,$3) ON CONFLICT DO NOTHING").bind(&route).bind(account).bind(actor.id).execute(&mut *tx).await?;
                } else {
                    sqlx::query(
                        "DELETE FROM source_route_managers WHERE route_id=$1 AND account_id=$2",
                    )
                    .bind(&route)
                    .bind(account)
                    .execute(&mut *tx)
                    .await?;
                }
                audit(
                    &mut tx,
                    Uuid::nil(),
                    None,
                    Some(actor.id),
                    "source.manager.changed",
                    &actor.request_id,
                    json!({"route_id":route,"account_id":account,"granted":input.granted}),
                )
                .await?;
                json!({"id":route})
            }
        };
        tx.commit().await?;
        Ok(value)
    }
}
