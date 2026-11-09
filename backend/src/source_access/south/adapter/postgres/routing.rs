use super::*;
use crate::distribution::south::adapter::postgres::routing as resources;
use crate::{
    business::Actor,
    source_access::{domain::routing::*, south::port::RoutingStore},
};
#[derive(Clone)]
pub struct RoutingRepository(pub PgPool);
type Tx<'a> = Transaction<'a, Postgres>;
fn resource_error(e: crate::business::Error) -> Fault {
    match e {
        crate::business::Error::Forbidden => Fault("forbidden"),
        _ => Fault("temporarily_unavailable"),
    }
}
pub(crate) async fn administrator(tx: &mut Tx<'_>, actor: Uuid) -> Result<bool> {
    sqlx::query_scalar("SELECT platform_admin FROM accounts WHERE id=$1 AND active FOR SHARE")
        .bind(actor)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(Fault("forbidden"))
}
async fn lock_route_mode(
    tx: &mut Tx<'_>,
    route: &str,
    control: bool,
) -> Result<sqlx::postgres::PgRow> {
    let source: Uuid = sqlx::query_scalar("SELECT source_id FROM source_routes WHERE route_id=$1")
        .bind(route)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(Fault("not_found"))?;
    sqlx::query(if control {
        "SELECT source_id FROM source_callers WHERE source_id=$1 FOR UPDATE"
    } else {
        "SELECT source_id FROM source_callers WHERE source_id=$1 FOR SHARE"
    })
    .bind(source)
    .fetch_one(&mut **tx)
    .await?;
    sqlx::query("SELECT * FROM source_routes WHERE route_id=$1 FOR UPDATE")
        .bind(route)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(Fault("not_found"))
}
async fn lock_route(tx: &mut Tx<'_>, route: &str) -> Result<sqlx::postgres::PgRow> {
    lock_route_mode(tx, route, true).await
}
async fn latest(tx: &mut Tx<'_>, route: &str) -> Result<Option<RouteSnapshot>> {
    let value:Option<Value>=sqlx::query_scalar("SELECT snapshot FROM source_route_revisions WHERE route_id=$1 ORDER BY revision DESC LIMIT 1").bind(route).fetch_optional(&mut **tx).await?;
    value
        .map(serde_json::from_value)
        .transpose()
        .map_err(Into::into)
}
pub(crate) async fn access(tx: &mut Tx<'_>, actor: Uuid, route: &str, admin: bool) -> Result<()> {
    if admin {
        return Ok(());
    }
    let allowed: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM source_route_managers WHERE route_id=$1 AND account_id=$2)",
    )
    .bind(route)
    .bind(actor)
    .fetch_one(&mut **tx)
    .await?;
    ensure(allowed, "forbidden")?;
    let config = latest(tx, route)
        .await?
        .and_then(|s| s.execution)
        .ok_or(Fault("forbidden"))?;
    resources::authorize(tx, actor, config.channel.as_str(), config.workspace_id)
        .await
        .map_err(resource_error)
}
async fn view(tx: &mut Tx<'_>, row: &sqlx::postgres::PgRow) -> Result<Value> {
    let route: String = row.get("route_id");
    let mut value = json!({"id":route,"revision":row.get::<i64,_>("revision"),"config_revision":row.get::<i64,_>("config_revision"),"unit_id":row.get::<Option<Uuid>,_>("unit_id"),"group_id":row.get::<Option<Uuid>,_>("group_id"),"status":row.get::<String,_>("status"),"execution_state":row.get::<String,_>("execution_state"),"retired_at":row.get::<Option<chrono::DateTime<chrono::Utc>>,_>("retired_at"),"snapshot":latest(tx,&route).await?});
    value["managers"]=json!(sqlx::query_scalar::<_,Value>("SELECT jsonb_build_object('id',m.account_id,'username',a.username,'revision',m.revision,'can_recheck',m.can_recheck) FROM source_route_managers m JOIN accounts a ON a.id=m.account_id WHERE route_id=$1 ORDER BY a.username").bind(&route).fetch_all(&mut **tx).await?);
    Ok(value)
}
async fn current_targets(
    tx: &mut Tx<'_>,
    execution: &ExecutionConfig,
) -> Result<Vec<TargetSnapshot>> {
    resources::targets(tx, execution.channel.as_str(), execution.workspace_id)
        .await
        .map_err(resource_error)?
        .into_iter()
        .map(|v| serde_json::from_value(v).map_err(Into::into))
        .collect()
}
async fn check(
    tx: &mut Tx<'_>,
    row: &sqlx::postgres::PgRow,
    snapshot: Option<&RouteSnapshot>,
) -> Result<CheckResult> {
    let mut issues = Vec::new();
    let Some(s) = snapshot else {
        return Ok(CheckResult::new(vec![
            "route_configuration_required".into(),
        ]));
    };
    if row
        .get::<Option<chrono::DateTime<chrono::Utc>>, _>("retired_at")
        .is_some()
    {
        issues.push("route_retired".into());
    }
    if row.get::<String, _>("status") != "active" {
        issues.push("route_disabled".into());
    }
    let authorized:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM source_key_routes k JOIN source_credentials c USING(key_id,source_id) WHERE k.route_id=$1 AND k.status='active' AND c.status='active' AND c.valid_from<=now() AND (c.valid_until IS NULL OR c.valid_until>now()))").bind(&s.route_id).fetch_one(&mut **tx).await?;
    if !authorized {
        issues.push("source_authorization_revoked".into());
    }
    let unit: Option<bool> =
        sqlx::query_scalar("SELECT active FROM routing_units WHERE id=$1 FOR SHARE")
            .bind(s.unit_id)
            .fetch_optional(&mut **tx)
            .await?;
    let group: Option<bool> = sqlx::query_scalar(
        "SELECT active FROM routing_groups WHERE id=$1 AND unit_id=$2 FOR SHARE",
    )
    .bind(s.group_id)
    .bind(s.unit_id)
    .fetch_optional(&mut **tx)
    .await?;
    if unit != Some(true) {
        issues.push("unit_disabled".into());
    }
    if group != Some(true) {
        issues.push("group_disabled".into());
    }
    let manager:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM accounts a WHERE a.id=$1 AND a.active AND (a.platform_admin OR EXISTS(SELECT 1 FROM source_route_managers m WHERE m.route_id=$2 AND m.account_id=a.id)))").bind(s.configured_by).bind(&s.route_id).fetch_one(&mut **tx).await?;
    if !manager {
        issues.push("workspace_authorization_revoked".into());
    }
    if let Some(c) = &s.execution {
        match resources::authorize(tx, s.configured_by, c.channel.as_str(), c.workspace_id).await {
            Ok(()) => {
                let targets = current_targets(tx, c).await?;
                let chosen: Vec<_> = targets
                    .iter()
                    .filter(|t| c.target_ids.contains(&t.id))
                    .collect();
                if chosen.is_empty() {
                    issues.push("route_no_targets".into());
                }
                if chosen.iter().any(|t| !t.resource_available) {
                    issues.push(
                        match c.channel {
                            Channel::Mobile => "mobile_candidates_missing",
                            Channel::Wecom => "channel_account_missing",
                        }
                        .into(),
                    );
                }
            }
            Err(crate::business::Error::Forbidden) => {
                issues.push("workspace_authorization_revoked".into())
            }
            Err(e) => return Err(resource_error(e)),
        }
    } else {
        issues.push("route_configuration_required".into());
    }
    Ok(CheckResult::new(issues))
}
async fn record(
    tx: &mut Tx<'_>,
    actor: &Actor,
    route: Option<&str>,
    action: &str,
    detail: Value,
) -> Result<()> {
    let resource = route.and_then(|r| r.parse().ok()).unwrap_or(actor.id);
    crate::audit::record(tx, actor, Uuid::nil(), action, resource, detail.clone())
        .await
        .map_err(resource_error)?;
    if let Some(route) = route {
        let source: Option<Uuid> =
            sqlx::query_scalar("SELECT source_id FROM source_routes WHERE route_id=$1")
                .bind(route)
                .fetch_optional(&mut **tx)
                .await?;
        if let Some(source) = source {
            audit(
                tx,
                source,
                None,
                Some(actor.id),
                action,
                &actor.request_id,
                detail,
            )
            .await?;
        }
    }
    Ok(())
}
async fn wake(tx: &mut Tx<'_>, route: Option<&str>) -> Result<()> {
    sqlx::query("UPDATE source_inbox i SET next_attempt_at=now() FROM source_receipts r WHERE r.receipt_id=i.receipt_id AND i.state='waiting' AND ($1::text IS NULL OR r.route_id=$1)").bind(route).execute(&mut **tx).await?;
    sqlx::query("UPDATE source_routing_waits SET next_attempt_at=now(),updated_at=now() WHERE NOT stopped AND ($1::text IS NULL OR route_id=$1)").bind(route).execute(&mut **tx).await?;
    Ok(())
}
async fn entity(
    tx: &mut Tx<'_>,
    actor: &Actor,
    kind: &str,
    id: Option<Uuid>,
    input: EntityInput,
) -> Result<Value> {
    ensure(
        matches!(kind, "routing-units" | "routing-groups"),
        "invalid_request",
    )?;
    let group = kind == "routing-groups";
    ensure(group == input.unit_id.is_some(), "invalid_request")?;
    let table = if group {
        "routing_groups"
    } else {
        "routing_units"
    };
    let object = id.unwrap_or_else(Uuid::now_v7);
    // Unit before group is the same order used by route checks.
    if let Some(unit) = input.unit_id {
        sqlx::query("SELECT id FROM routing_units WHERE id=$1 FOR SHARE")
            .bind(unit)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(Fault("not_found"))?;
    }
    if id.is_some() {
        let old = sqlx::query(&format!("SELECT * FROM {table} WHERE id=$1 FOR UPDATE"))
            .bind(object)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(Fault("not_found"))?;
        ensure(
            input.revision == Some(old.get("revision")),
            "revision_conflict",
        )?;
        if group && input.unit_id != Some(old.get("unit_id")) {
            let referenced:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM source_routes WHERE group_id=$1 UNION ALL SELECT 1 FROM routing_group_targets WHERE group_id=$1 AND removed_at IS NULL)").bind(object).fetch_one(&mut **tx).await?;
            ensure(!referenced, "group_in_use")?;
        }
        let query = if group {
            "UPDATE routing_groups SET name=$2,description=$3,active=$4,unit_id=$5,revision=revision+1,updated_by=$6,updated_at=now() WHERE id=$1"
        } else {
            "UPDATE routing_units SET name=$2,description=$3,active=$4,revision=revision+1,updated_by=$6,updated_at=now() WHERE id=$1 AND $5::uuid IS NULL"
        };
        sqlx::query(query)
            .bind(object)
            .bind(input.name.trim())
            .bind(input.description)
            .bind(input.active)
            .bind(input.unit_id)
            .bind(actor.id)
            .execute(&mut **tx)
            .await?;
    } else {
        ensure(input.revision.is_none(), "invalid_request")?;
        let query = if group {
            "INSERT INTO routing_groups(id,name,description,active,unit_id,created_by,updated_by) VALUES($1,$2,$3,$4,$5,$6,$6)"
        } else {
            "INSERT INTO routing_units(id,name,description,active,created_by,updated_by) SELECT $1,$2,$3,$4,$6,$6 WHERE $5::uuid IS NULL"
        };
        sqlx::query(query)
            .bind(object)
            .bind(input.name.trim())
            .bind(input.description)
            .bind(input.active)
            .bind(input.unit_id)
            .bind(actor.id)
            .execute(&mut **tx)
            .await?;
    }
    let value: Value =
        sqlx::query_scalar(&format!("SELECT to_jsonb(t) FROM {table} t WHERE id=$1"))
            .bind(object)
            .fetch_one(&mut **tx)
            .await?;
    sqlx::query("INSERT INTO routing_entity_history(id,kind,entity_id,revision,snapshot,actor_id) VALUES($1,$2,$3,$4,$5,$6)").bind(Uuid::now_v7()).bind(if group{"group"}else{"unit"}).bind(object).bind(value["revision"].as_i64().unwrap()).bind(&value).bind(actor.id).execute(&mut **tx).await?;
    record(
        tx,
        actor,
        None,
        "routing.entity.save",
        json!({"kind":kind,"snapshot":value}),
    )
    .await?;
    wake(tx, None).await?;
    Ok(value)
}
async fn configure(
    tx: &mut Tx<'_>,
    actor: &Actor,
    row: &sqlx::postgres::PgRow,
    input: ConfigurationInput,
) -> Result<Value> {
    ensure(
        row.get::<i64, _>("revision") == input.revision,
        "revision_conflict",
    )?;
    ensure(
        row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("retired_at")
            .is_none(),
        "route_retired",
    )?;
    if let Some(unit) = row.get::<Option<Uuid>, _>("unit_id") {
        ensure(
            unit == input.unit_id && row.get::<Option<Uuid>, _>("group_id") == Some(input.group_id),
            "scope_immutable",
        )?;
    }
    let unit: String = sqlx::query_scalar("SELECT name FROM routing_units WHERE id=$1 FOR SHARE")
        .bind(input.unit_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(Fault("not_found"))?;
    let group: String =
        sqlx::query_scalar("SELECT name FROM routing_groups WHERE id=$1 AND unit_id=$2 FOR SHARE")
            .bind(input.group_id)
            .bind(input.unit_id)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(Fault("not_found"))?;
    let route: String = row.get("route_id");
    let version = row.get::<i64, _>("config_revision") + 1;
    let mut selected = Vec::new();
    if let Some(c) = &input.execution {
        resources::authorize(tx, actor.id, c.channel.as_str(), c.workspace_id)
            .await
            .map_err(resource_error)?;
        let targets = current_targets(tx, c).await?;
        for id in &c.target_ids {
            selected.push(
                targets
                    .iter()
                    .find(|t| t.id == *id)
                    .ok_or(Fault("target_unavailable"))?
                    .clone(),
            );
        }
    }
    let snapshot = RouteSnapshot {
        route_id: route.clone(),
        revision: version,
        source_id: row.get("source_id"),
        unit_id: input.unit_id,
        unit_name: unit,
        group_id: input.group_id,
        group_name: group,
        execution: input.execution,
        targets: selected,
        configured_by: actor.id,
    };
    // Saving an incomplete configuration is allowed only as a draft/paused chain.
    if row.get::<String, _>("execution_state") == "running" {
        ensure(
            check(tx, row, Some(&snapshot)).await?.ready,
            "route_configuration_required",
        )?;
    }
    sqlx::query("UPDATE source_routes SET unit_id=$2,group_id=$3,config_revision=$4,revision=revision+1,updated_at=now() WHERE route_id=$1").bind(&route).bind(input.unit_id).bind(input.group_id).bind(version).execute(&mut **tx).await?;
    sqlx::query("INSERT INTO source_route_revisions(route_id,revision,source_id,snapshot,actor_id) VALUES($1,$2,$3,$4,$5)").bind(&route).bind(version).bind(snapshot.source_id).bind(json!(snapshot)).bind(actor.id).execute(&mut **tx).await?;
    sqlx::query(
        "UPDATE source_route_targets SET removed_at=now() WHERE route_id=$1 AND removed_at IS NULL",
    )
    .bind(&route)
    .execute(&mut **tx)
    .await?;
    if let Some(c) = &snapshot.execution {
        for id in &c.target_ids {
            sqlx::query("INSERT INTO source_route_targets(id,route_id,channel,workspace_id,target_id,added_by) VALUES($1,$2,$3,$4,$5,$6)").bind(Uuid::now_v7()).bind(&route).bind(c.channel.as_str()).bind(c.workspace_id).bind(id).bind(actor.id).execute(&mut **tx).await?;
        }
    }
    sqlx::query(
        "UPDATE source_callers SET revision=revision+1,updated_at=now() WHERE source_id=$1",
    )
    .bind(snapshot.source_id)
    .execute(&mut **tx)
    .await?;
    record(
        tx,
        actor,
        Some(&route),
        "routing.configuration.save",
        json!({"config_revision":version,"snapshot":snapshot}),
    )
    .await?;
    wake(tx, Some(&route)).await?;
    Ok(json!({"id":route,"revision":input.revision+1,"config_revision":version}))
}
impl RoutingRepository {
    async fn execute_inner(&self, actor: &Actor, command: Command) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        let admin = administrator(&mut tx, actor.id).await?;
        let value = match command {
            Command::Assigned => {
                let candidates:Vec<String>=sqlx::query_scalar("SELECT r.route_id FROM source_routes r WHERE $2 OR EXISTS(SELECT 1 FROM source_route_managers m WHERE m.route_id=r.route_id AND m.account_id=$1) ORDER BY r.route_id").bind(actor.id).bind(admin).fetch_all(&mut *tx).await?;
                let mut items = Vec::new();
                for route in candidates {
                    match access(&mut tx, actor.id, &route, admin).await {
                        Ok(()) => {
                            let item:Option<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',r.route_id,'route_name',r.route_name,'caller_name',c.unit_name,'execution_state',r.execution_state,'retired_at',r.retired_at) FROM source_routes r JOIN LATERAL (SELECT unit_name FROM source_credentials WHERE source_id=r.source_id ORDER BY key_id LIMIT 1) c ON true WHERE r.route_id=$1").bind(route).fetch_optional(&mut *tx).await?;
                            if let Some(item) = item {
                                items.push(item);
                            }
                        }
                        Err(Fault("forbidden")) => {}
                        Err(error) => return Err(error),
                    }
                }
                json!({"items":items})
            }
            Command::Entities { kind } => {
                ensure(
                    matches!(kind.as_str(), "routing-units" | "routing-groups"),
                    "invalid_request",
                )?;
                let (table, scope) = if kind == "routing-units" {
                    ("routing_units", "unit_id")
                } else {
                    ("routing_groups", "group_id")
                };
                let query = format!(
                    "SELECT to_jsonb(t) FROM {table} t WHERE $1 OR EXISTS(SELECT 1 FROM source_routes r JOIN source_route_managers m USING(route_id) WHERE r.{scope}=t.id AND m.account_id=$2) ORDER BY t.name,t.id"
                );
                let mut items: Vec<Value> = sqlx::query_scalar(&query)
                    .bind(admin)
                    .bind(actor.id)
                    .fetch_all(&mut *tx)
                    .await?;
                if !admin {
                    let routes:Vec<String>=sqlx::query_scalar("SELECT route_id FROM source_route_managers WHERE account_id=$1 ORDER BY route_id").bind(actor.id).fetch_all(&mut *tx).await?;
                    let mut visible = Vec::new();
                    for route in routes {
                        match access(&mut tx, actor.id, &route, false).await {
                            Ok(()) => {
                                if let Some(snapshot) = latest(&mut tx, &route).await? {
                                    visible.push(if kind == "routing-units" {
                                        snapshot.unit_id
                                    } else {
                                        snapshot.group_id
                                    });
                                }
                            }
                            Err(Fault("forbidden")) => {}
                            Err(error) => return Err(error),
                        }
                    }
                    items.retain(|item| {
                        item["id"]
                            .as_str()
                            .and_then(|id| id.parse::<Uuid>().ok())
                            .is_some_and(|id| visible.contains(&id))
                    });
                }
                json!({"items":items})
            }
            Command::Entity { kind, id, input } => {
                ensure(admin, "forbidden")?;
                entity(&mut tx, actor, &kind, id, input).await?
            }
            Command::Resources { channel, workspace } => {
                if let Some(w) = workspace {
                    resources::authorize(&mut tx, actor.id, channel.as_str(), w)
                        .await
                        .map_err(resource_error)?;
                    json!({"items":resources::targets(&mut tx,channel.as_str(),w).await.map_err(resource_error)?})
                } else {
                    json!({"items":resources::workspaces(&mut tx,actor.id,channel.as_str()).await.map_err(resource_error)?})
                }
            }
            Command::GroupTargets { group } => {
                ensure(admin, "forbidden")?;
                let items:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(t) FROM routing_group_targets t WHERE group_id=$1 AND removed_at IS NULL ORDER BY added_at,id").bind(group).fetch_all(&mut *tx).await?;
                json!({"items":items})
            }
            Command::SetGroupTargets { group, input } => {
                ensure(admin, "forbidden")?;
                let revision: i64 = sqlx::query_scalar(
                    "SELECT revision FROM routing_groups WHERE id=$1 FOR UPDATE",
                )
                .bind(group)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(Fault("not_found"))?;
                ensure(revision == input.revision, "revision_conflict")?;
                let c = &input.execution;
                resources::authorize(&mut tx, actor.id, c.channel.as_str(), c.workspace_id)
                    .await
                    .map_err(resource_error)?;
                let targets = current_targets(&mut tx, c).await?;
                ensure(
                    c.target_ids
                        .iter()
                        .all(|id| targets.iter().any(|t| t.id == *id)),
                    "target_unavailable",
                )?;
                let removed:Vec<Uuid>=sqlx::query_scalar("SELECT target_id FROM routing_group_targets WHERE group_id=$1 AND channel=$2 AND workspace_id=$3 AND removed_at IS NULL AND NOT(target_id=ANY($4))").bind(group).bind(c.channel.as_str()).bind(c.workspace_id).bind(&c.target_ids).fetch_all(&mut *tx).await?;
                let used:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM source_route_targets t JOIN source_routes r USING(route_id) WHERE r.group_id=$1 AND t.channel=$2 AND t.workspace_id=$3 AND t.target_id=ANY($4) AND t.removed_at IS NULL)").bind(group).bind(c.channel.as_str()).bind(c.workspace_id).bind(removed).fetch_one(&mut *tx).await?;
                ensure(!used, "group_in_use")?;
                sqlx::query("UPDATE routing_group_targets SET removed_at=now() WHERE group_id=$1 AND channel=$2 AND workspace_id=$3 AND removed_at IS NULL AND NOT(target_id=ANY($4))").bind(group).bind(c.channel.as_str()).bind(c.workspace_id).bind(&c.target_ids).execute(&mut *tx).await?;
                for target in &c.target_ids {
                    sqlx::query("INSERT INTO routing_group_targets(id,group_id,channel,workspace_id,target_id,added_by) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING").bind(Uuid::now_v7()).bind(group).bind(c.channel.as_str()).bind(c.workspace_id).bind(target).bind(actor.id).execute(&mut *tx).await?;
                }
                let snapshot:Value=sqlx::query_scalar("UPDATE routing_groups SET revision=revision+1,updated_by=$2,updated_at=now() WHERE id=$1 RETURNING to_jsonb(routing_groups)").bind(group).bind(actor.id).fetch_one(&mut *tx).await?;
                sqlx::query("INSERT INTO routing_entity_history(id,kind,entity_id,revision,snapshot,actor_id) VALUES($1,'group',$2,$3,$4,$5)").bind(Uuid::now_v7()).bind(group).bind(input.revision+1).bind(&snapshot).bind(actor.id).execute(&mut *tx).await?;
                record(
                    &mut tx,
                    actor,
                    None,
                    "routing.group.targets",
                    json!({"group_id":group,"execution":c}),
                )
                .await?;
                snapshot
            }
            other => {
                let route = match &other {
                    Command::Configuration { route }
                    | Command::Check { route }
                    | Command::Managers { route }
                    | Command::Revisions { route, .. }
                    | Command::Handoff { route }
                    | Command::Configure { route, .. }
                    | Command::Lifecycle { route, .. }
                    | Command::Manager { route, .. } => route.clone(),
                    _ => unreachable!(),
                };
                let row = lock_route(&mut tx, &route).await?;
                access(&mut tx, actor.id, &route, admin).await?;
                let result = match other {
                    Command::Configuration { .. } => view(&mut tx, &row).await?,
                    Command::Check { .. } => {
                        let s = latest(&mut tx, &route).await?;
                        json!(check(&mut tx, &row, s.as_ref()).await?)
                    }
                    Command::Configure { input, .. } => {
                        configure(&mut tx, actor, &row, input).await?
                    }
                    Command::Lifecycle { input, .. } => {
                        ensure(
                            row.get::<i64, _>("revision") == input.revision,
                            "revision_conflict",
                        )?;
                        let retired = row
                            .get::<Option<chrono::DateTime<chrono::Utc>>, _>("retired_at")
                            .is_some();
                        let s = latest(&mut tx, &route).await?;
                        let ready = if matches!(
                            input.action,
                            LifecycleAction::Enable | LifecycleAction::Resume
                        ) {
                            check(&mut tx, &row, s.as_ref()).await?.ready
                        } else {
                            false
                        };
                        let state = input.next_state(
                            &row.get::<String, _>("execution_state"),
                            retired,
                            ready,
                        )?;
                        let retire = matches!(input.action, LifecycleAction::Retire);
                        sqlx::query("UPDATE source_routes SET execution_state=$2,authorized_by=CASE WHEN $2='running' THEN $4 ELSE authorized_by END,revision=revision+1,updated_at=now(),retired_at=CASE WHEN $3 THEN now() ELSE retired_at END,status=CASE WHEN $3 THEN 'disabled' ELSE status END WHERE route_id=$1").bind(&route).bind(state).bind(retire).bind(actor.id).execute(&mut *tx).await?;
                        if retire {
                            sqlx::query("INSERT INTO source_route_tombstones(route_id,source_id,reason,config_revision) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING").bind(&route).bind(row.get::<Uuid,_>("source_id")).bind("route_retired").bind(row.get::<i64,_>("config_revision")).execute(&mut *tx).await?;
                            sqlx::query("UPDATE source_routing_waits SET stopped=true,reason_code='route_retired',updated_at=now() WHERE route_id=$1").bind(&route).execute(&mut *tx).await?;
                        }
                        sqlx::query("UPDATE source_callers SET revision=revision+1,updated_at=now() WHERE source_id=$1").bind(row.get::<Uuid,_>("source_id")).execute(&mut *tx).await?;
                        record(
                            &mut tx,
                            actor,
                            Some(&route),
                            "routing.lifecycle",
                            json!(input),
                        )
                        .await?;
                        wake(&mut tx, Some(&route)).await?;
                        json!({"id":route,"revision":input.revision+1,"execution_state":state})
                    }
                    Command::Revisions { page, .. } => {
                        ensure((1..=100_000).contains(&page), "invalid_request")?;
                        let rows:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('revision',revision,'snapshot',snapshot,'actor_id',actor_id,'created_at',created_at) FROM source_route_revisions WHERE route_id=$1 ORDER BY revision DESC LIMIT 31 OFFSET $2").bind(&route).bind((page-1)*30).fetch_all(&mut *tx).await?;
                        json!({"has_more":rows.len()>30,"items":rows.into_iter().take(30).collect::<Vec<_>>()})
                    }
                    Command::Managers { .. } => {
                        ensure(admin, "forbidden")?;
                        json!({"items":view(&mut tx,&row).await?["managers"]})
                    }
                    Command::Manager { input, .. } => {
                        ensure(
                            row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("retired_at")
                                .is_none(),
                            "route_retired",
                        )?;
                        ensure(admin, "forbidden")?;
                        ensure(
                            row.get::<i64, _>("revision") == input.revision,
                            "revision_conflict",
                        )?;
                        let account: Uuid = sqlx::query_scalar(
                            "SELECT id FROM accounts WHERE username=$1 AND active FOR SHARE",
                        )
                        .bind(input.username.trim())
                        .fetch_optional(&mut *tx)
                        .await?
                        .ok_or(Fault("not_found"))?;
                        if input.granted {
                            sqlx::query("INSERT INTO source_route_managers(route_id,account_id,granted_by,can_recheck) VALUES($1,$2,$3,$4) ON CONFLICT(route_id,account_id) DO UPDATE SET revision=source_route_managers.revision+1,granted_by=$3,can_recheck=$4").bind(&route).bind(account).bind(actor.id).bind(input.can_recheck).execute(&mut *tx).await?;
                        } else {
                            sqlx::query("DELETE FROM source_route_managers WHERE route_id=$1 AND account_id=$2").bind(&route).bind(account).execute(&mut *tx).await?;
                        }
                        sqlx::query("UPDATE source_routes SET revision=revision+1,updated_at=now() WHERE route_id=$1").bind(&route).execute(&mut *tx).await?;
                        record(
                            &mut tx,
                            actor,
                            Some(&route),
                            "routing.manager",
                            json!(input),
                        )
                        .await?;
                        json!({"id":route,"revision":input.revision+1})
                    }
                    Command::Handoff { .. } => {
                        ensure(admin, "forbidden")?;
                        record(&mut tx,actor,Some(&route),"routing.handoff",json!({"contract_version":"source-v2-3","config_revision":row.get::<i64,_>("config_revision")})).await?;
                        json!({"id":route,"route_id":route,"receiver_path":"/api/v2/source-events","authentication":"HMAC-SHA256","signature_versions":[2],"schema_versions":[2],"contract_version":"source-v2-3","config_revision":row.get::<i64,_>("config_revision")})
                    }
                    _ => unreachable!(),
                };
                record(
                    &mut tx,
                    actor,
                    Some(&route),
                    "routing.management",
                    json!({"route_id":route}),
                )
                .await?;
                result
            }
        };
        tx.commit().await?;
        Ok(value)
    }
}
impl RoutingStore for RoutingRepository {
    async fn execute(&self, actor: &Actor, command: Command) -> Result<Value> {
        let result = self.execute_inner(actor, command).await;
        if let Err(e) = &result {
            let mut tx = self.0.begin().await?;
            record(
                &mut tx,
                actor,
                None,
                "routing.rejected",
                json!({"code":e.0}),
            )
            .await?;
            tx.commit().await?;
        }
        result
    }
    async fn resolve(&self, receipt: Uuid) -> Result<Resolution> {
        let mut tx = self.0.begin().await?;
        // Source lock precedes route lock. Receipt lock then serializes duplicate decisions.
        let route: String =
            sqlx::query_scalar("SELECT route_id FROM source_receipts WHERE receipt_id=$1")
                .bind(receipt)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(Fault("not_found"))?;
        let row = match lock_route_mode(&mut tx, &route, false).await {
            Ok(row) => Some(row),
            Err(Fault("not_found")) => None,
            Err(e) => return Err(e),
        };
        sqlx::query("SELECT receipt_id FROM source_receipts WHERE receipt_id=$1 FOR UPDATE")
            .bind(receipt)
            .fetch_one(&mut *tx)
            .await?;
        let existing: Option<Value> =
            sqlx::query_scalar("SELECT snapshot FROM source_routing_decisions WHERE receipt_id=$1")
                .bind(receipt)
                .fetch_optional(&mut *tx)
                .await?;
        if let Some(existing) = existing {
            return Ok(Resolution::Ready {
                snapshot: serde_json::from_value(existing)?,
            });
        }
        let stopped: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM source_routing_waits WHERE receipt_id=$1 AND stopped)",
        )
        .bind(receipt)
        .fetch_one(&mut *tx)
        .await?;
        let mut reason = if stopped {
            Some("message_stopped".into())
        } else {
            None
        };
        let mut snapshot = None;
        if reason.is_none() {
            if let Some(row) = row.as_ref() {
                let s = latest(&mut tx, &route).await?;
                if row
                    .get::<Option<chrono::DateTime<chrono::Utc>>, _>("retired_at")
                    .is_some()
                {
                    reason = Some("route_retired".into());
                } else if row.get::<String, _>("execution_state") == "paused" {
                    reason = Some("route_paused".into());
                } else if row.get::<String, _>("execution_state") == "draft" {
                    reason = Some("route_configuration_required".into());
                } else {
                    reason = check(&mut tx, row, s.as_ref())
                        .await?
                        .issues
                        .into_iter()
                        .next();
                }
                snapshot = s;
            } else {
                reason = Some("route_retired".into());
            }
        }
        if let Some(reason) = reason {
            sqlx::query("INSERT INTO source_routing_waits(receipt_id,route_id,reason_code,wake_key,next_attempt_at,stopped) VALUES($1,$2,$3,$2,now()+interval '30 seconds',$4) ON CONFLICT(receipt_id) DO UPDATE SET reason_code=$3,next_attempt_at=EXCLUDED.next_attempt_at,updated_at=now(),stopped=source_routing_waits.stopped OR EXCLUDED.stopped").bind(receipt).bind(&route).bind(&reason).bind(stopped||reason=="route_retired").execute(&mut *tx).await?;
            tx.commit().await?;
            return Ok(Resolution::Waiting {
                reason_code: reason,
            });
        }
        let mut s = snapshot.ok_or(Fault("route_configuration_required"))?;
        if let Some(c) = &s.execution {
            s.targets = current_targets(&mut tx, c)
                .await?
                .into_iter()
                .filter(|t| c.target_ids.contains(&t.id))
                .collect();
        }
        sqlx::query("INSERT INTO source_routing_decisions(receipt_id,source_id,route_id,config_revision,snapshot,target_digest) VALUES($1,$2,$3,$4,$5,$6)").bind(receipt).bind(s.source_id).bind(&route).bind(s.revision).bind(json!(s)).bind(digest(json!(s.targets).to_string().as_bytes())).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM source_routing_waits WHERE receipt_id=$1")
            .bind(receipt)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Resolution::Ready { snapshot: s })
    }
}

impl RoutingRepository {
    /// Revisit only decisions explicitly deferred by a consumer. This never consumes
    /// a fresh inbox or substitutes for content projection, filtering or task creation.
    pub async fn retry_waiting(&self) -> Result<()> {
        let receipts: Vec<Uuid> = sqlx::query_scalar("SELECT receipt_id FROM source_routing_waits WHERE NOT stopped AND next_attempt_at<=now() ORDER BY next_attempt_at,receipt_id LIMIT 50")
            .fetch_all(&self.0).await?;
        for receipt in receipts {
            if let Err(error) = self.resolve(receipt).await {
                tracing::warn!(%receipt,code=error.0,"routing configuration resolution will retry");
            }
        }
        Ok(())
    }
}

/// Execution adapters must call this inside the transaction that submits or signs
/// a source task. The held source/route/target locks order it with control changes.
/// The result narrows a frozen decision; it never authorizes another target/candidate.
pub async fn guard_submission(
    tx: &mut Tx<'_>,
    receipt: Uuid,
    target: Uuid,
) -> Result<TargetSnapshot> {
    let decision: Value =
        sqlx::query_scalar("SELECT snapshot FROM source_routing_decisions WHERE receipt_id=$1")
            .bind(receipt)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(Fault("not_found"))?;
    let frozen: RouteSnapshot = serde_json::from_value(decision)?;
    sqlx::query("SELECT source_id FROM source_callers WHERE source_id=$1 FOR SHARE")
        .bind(frozen.source_id)
        .fetch_one(&mut **tx)
        .await?;
    let live_route = lock_route_mode(tx, &frozen.route_id, false).await;
    let current:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM source_receipts r JOIN source_inbox i USING(receipt_id) JOIN source_content_versions v ON v.source_id=r.source_id AND v.route_id=r.route_id AND v.external_content_id=convert_from(i.request_body,'UTF8')::jsonb #>> '{data,attributes,external_content_id}' WHERE r.receipt_id=$1 AND v.receipt_id=$1 AND NOT v.withdrawn)").bind(receipt).fetch_one(&mut **tx).await?;
    ensure(current, "content_version_superseded")?;
    crate::advertising::south::adapter::postgres::pipeline::current_guard(tx, receipt, &frozen)
        .await?;
    super::keyword_filter::guard_admission(tx, receipt, &frozen).await?;
    let config = frozen
        .execution
        .as_ref()
        .ok_or(Fault("route_configuration_required"))?;
    match live_route {
        Ok(row) => {
            ensure(
                row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("retired_at")
                    .is_none(),
                "route_retired",
            )?;
            ensure(
                row.get::<String, _>("execution_state") == "running",
                "route_paused",
            )?;
            let controls = check(tx, &row, Some(&frozen)).await?;
            ensure(
                controls.issues.iter().all(|code| {
                    matches!(
                        code.as_str(),
                        "mobile_candidates_missing"
                            | "channel_account_missing"
                            | "route_no_targets"
                    )
                }),
                "route_configuration_required",
            )?;
            let member:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM source_route_targets WHERE route_id=$1 AND channel=$2 AND workspace_id=$3 AND target_id=$4 AND removed_at IS NULL)").bind(&frozen.route_id).bind(config.channel.as_str()).bind(config.workspace_id).bind(target).fetch_one(&mut **tx).await?;
            ensure(member, "target_removed")?;
        }
        Err(Fault("not_found")) => {
            // Credential deletion stops new intake, not already-authorized historical tasks.
            // A retired route has a different tombstone reason and remains stopped.
            let authorizer:Option<Uuid>=sqlx::query_scalar("SELECT l.authorization_actor_id FROM source_execution_tasks l JOIN source_route_tombstones t ON t.route_id=$2 AND t.reason='credential_deleted' WHERE l.receipt_id=$1").bind(receipt).bind(&frozen.route_id).fetch_optional(&mut **tx).await?;
            let authorizer = authorizer.ok_or(Fault("route_retired"))?;
            resources::authorize(tx, authorizer, config.channel.as_str(), config.workspace_id)
                .await
                .map_err(resource_error)?;
            let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM routing_units u JOIN routing_groups g ON g.unit_id=u.id WHERE u.id=$1 AND g.id=$2 AND u.active AND g.active)").bind(frozen.unit_id).bind(frozen.group_id).fetch_one(&mut **tx).await?;
            ensure(active, "unit_disabled")?;
        }
        Err(error) => return Err(error),
    }
    let original = frozen
        .targets
        .into_iter()
        .find(|t| t.id == target)
        .ok_or(Fault("target_unavailable"))?;
    let live = current_targets(tx, config)
        .await?
        .into_iter()
        .find(|t| t.id == target)
        .ok_or(Fault("target_unavailable"))?;
    ensure(live.name == original.name, "target_identity_changed")?;
    let mut result = original;
    match config.channel {
        Channel::Wecom => {
            ensure(
                live.identity["corp_id"] == result.identity["corp_id"]
                    && live.identity["chat_id"] == result.identity["chat_id"],
                "target_unavailable",
            )?;
            let selected = result.identity["senders"]
                .as_array()
                .and_then(|v| v.first())
                .ok_or(Fault("target_unavailable"))?;
            ensure(
                live.identity["senders"]
                    .as_array()
                    .is_some_and(|senders| senders.contains(selected)),
                "target_unavailable",
            )?;
        }
        Channel::Mobile => {
            let old = result.identity["candidates"]
                .as_array()
                .ok_or(Fault("target_unavailable"))?;
            let live = live.identity["candidates"]
                .as_array()
                .ok_or(Fault("target_unavailable"))?;
            let narrowed: Vec<_> = old
                .iter()
                .filter(|candidate| live.iter().any(|current| crate::distribution::south::adapter::postgres::shared_send_v2::routing_candidate_identity(current)==crate::distribution::south::adapter::postgres::shared_send_v2::routing_candidate_identity(candidate)))
                .cloned()
                .collect();
            ensure(!narrowed.is_empty(), "target_unavailable")?;
            result.identity["candidates"] = json!(narrowed);
        }
    }
    Ok(result)
}
