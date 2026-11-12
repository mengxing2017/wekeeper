use crate::{
    advertising::{domain::*, south::port::AdvertisingStore},
    business::Actor,
    source_access::{
        domain::{Fault, Result, ensure},
        south::adapter::postgres::routing as routes,
    },
};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;
type Tx<'a> = Transaction<'a, Postgres>;
#[derive(Clone)]
pub struct Repository(pub PgPool);
pub mod materials;
pub mod pipeline;
pub(crate) mod pools;
pub(crate) mod queue;
pub mod targeting;
pub(crate) async fn guard(tx: &mut Tx<'_>) -> Result<()> {
    crate::distribution::south::adapter::postgres::routing::source_guard(tx)
        .await
        .map_err(|_| Fault("temporarily_unavailable"))
}
async fn capabilities(
    tx: &mut Tx<'_>,
    actor: Uuid,
    admin: bool,
    unit: Uuid,
) -> Result<(bool, bool)> {
    if admin {
        return Ok((true, true));
    }
    Ok(sqlx::query_as("SELECT can_edit,can_share FROM advertisement_unit_managers WHERE unit_id=$1 AND account_id=$2 FOR SHARE").bind(unit).bind(actor).fetch_optional(&mut **tx).await?.unwrap_or((false,false)))
}
async fn route_snapshot(
    tx: &mut Tx<'_>,
    route: &str,
) -> Result<crate::source_access::domain::routing::RouteSnapshot> {
    let v:Value=sqlx::query_scalar("SELECT snapshot FROM source_route_revisions WHERE route_id=$1 ORDER BY revision DESC LIMIT 1").bind(route).fetch_optional(&mut **tx).await?.ok_or(Fault("route_configuration_required"))?;
    Ok(serde_json::from_value(v)?)
}
async fn available_routes(
    tx: &mut Tx<'_>,
    actor: Uuid,
    admin: bool,
) -> Result<Vec<crate::source_access::domain::routing::RouteSnapshot>> {
    let ids:Vec<String>=sqlx::query_scalar("SELECT route_id FROM source_routes WHERE retired_at IS NULL AND ($2 OR EXISTS(SELECT 1 FROM source_route_managers m WHERE m.route_id=source_routes.route_id AND m.account_id=$1)) ORDER BY route_id").bind(actor).bind(admin).fetch_all(&mut **tx).await?;
    let mut out = vec![];
    for id in ids {
        match routes::access(tx, actor, &id, admin).await {
            Ok(()) => match route_snapshot(tx, &id).await {
                Ok(s) => out.push(s),
                Err(Fault("route_configuration_required")) => {}
                Err(e) => return Err(e),
            },
            Err(Fault("forbidden")) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(out)
}
async fn scope_validate(tx: &mut Tx<'_>, scope: &Scope) -> Result<()> {
    let exists = match scope.kind.as_str() {
        "unit" => {
            sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM routing_units WHERE id::text=$1 AND active)",
            )
            .bind(&scope.id)
            .fetch_one(&mut **tx)
            .await?
        }
        "group" => {
            sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM routing_groups WHERE id::text=$1 AND active)",
            )
            .bind(&scope.id)
            .fetch_one(&mut **tx)
            .await?
        }
        "route" => sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM source_routes WHERE route_id=$1 AND retired_at IS NULL)",
        )
        .bind(&scope.id)
        .fetch_one(&mut **tx)
        .await?,
        _ => false,
    };
    ensure(exists, "invalid_ad_scope")?;
    if let Some(channel) = &scope.channel {
        let table = if channel == "mobile" {
            "mobile_workspaces"
        } else {
            "wecom_workspaces"
        };
        ensure(
            sqlx::query_scalar::<_, bool>(&format!(
                "SELECT EXISTS(SELECT 1 FROM {table} WHERE id=$1 AND active)"
            ))
            .bind(scope.workspace_id)
            .fetch_one(&mut **tx)
            .await?,
            "invalid_ad_scope",
        )?;
    }
    Ok(())
}
async fn ad_row(tx: &mut Tx<'_>, id: Uuid) -> Result<sqlx::postgres::PgRow> {
    sqlx::query("SELECT a.*,u.name AS owner_name,c.name,c.body,c.link,p.policy FROM advertisements a JOIN routing_units u ON u.id=a.owner_unit_id LEFT JOIN advertisement_content_versions c ON c.id=a.content_version LEFT JOIN advertisement_policy_versions p ON p.id=a.policy_version WHERE a.id=$1 FOR SHARE OF a").bind(id).fetch_optional(&mut **tx).await?.ok_or(Fault("not_found"))
}
fn ad_exclusion(
    r: &sqlx::postgres::PgRow,
    s: &crate::source_access::domain::routing::RouteSnapshot,
) -> Result<Option<&'static str>> {
    if r.get::<bool, _>("archived") {
        return Ok(Some("ad_archived"));
    }
    let policy: Option<Value> = r.get("policy");
    let Some(policy) = policy else {
        return Ok(Some("ad_unpublished"));
    };
    let policy: Policy = serde_json::from_value(policy)?;
    Ok(policy.exclusion(s, chrono::Utc::now()))
}
async fn visible(
    tx: &mut Tx<'_>,
    actor: Uuid,
    admin: bool,
    r: &sqlx::postgres::PgRow,
) -> Result<(bool, bool, bool)> {
    let (edit, share) = capabilities(tx, actor, admin, r.get("owner_unit_id")).await?;
    let mut usable = false;
    for s in available_routes(tx, actor, admin).await? {
        if ad_exclusion(r, &s)?.is_none() {
            usable = true;
            break;
        }
    }
    ensure(edit || share || usable, "not_found")?;
    Ok((edit, share, usable))
}
fn view(r: &sqlx::postgres::PgRow, edit: bool, share: bool, usable: bool) -> Value {
    json!({"id":r.get::<Uuid,_>("id"),"owner_unit_id":r.get::<Uuid,_>("owner_unit_id"),"owner_name":r.get::<String,_>("owner_name"),"revision":r.get::<i64,_>("revision"),"content_version":r.get::<Option<Uuid>,_>("content_version"),"policy_version":r.get::<Option<Uuid>,_>("policy_version"),"archived":r.get::<bool,_>("archived"),"content":{"name":r.get::<Option<String>,_>("name"),"body":r.get::<Option<String>,_>("body"),"link":r.get::<Option<String>,_>("link")},"policy":if edit||share{r.get::<Option<Value>,_>("policy")}else{None},"can_edit":edit,"can_manage_sharing":share,"can_use":usable})
}
async fn revision(tx: &mut Tx<'_>, id: Uuid, expected: i64) -> Result<i64> {
    let n: i64 = sqlx::query_scalar("SELECT revision FROM advertisements WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_one(&mut **tx)
        .await?;
    ensure(n == expected, "revision_conflict")?;
    Ok(n + 1)
}
async fn publish_content(
    tx: &mut Tx<'_>,
    actor: Uuid,
    id: Uuid,
    revision: i64,
    c: &Content,
    reason: &str,
) -> Result<Uuid> {
    let version = Uuid::now_v7();
    sqlx::query("INSERT INTO advertisement_content_versions(id,ad_id,revision,name,body,link,actor_id,reason) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(version).bind(id).bind(revision).bind(&c.name).bind(&c.body).bind(&c.link).bind(actor).bind(reason).execute(&mut **tx).await?;
    sqlx::query("UPDATE advertisements SET content_version=$2,revision=$3 WHERE id=$1")
        .bind(id)
        .bind(version)
        .bind(revision)
        .execute(&mut **tx)
        .await?;
    Ok(version)
}
async fn publish_policy(
    tx: &mut Tx<'_>,
    actor: Uuid,
    id: Uuid,
    revision: i64,
    p: &Policy,
    reason: &str,
) -> Result<Uuid> {
    p.validate().map_err(|e| Fault(e.0))?;
    for s in p.allows.iter().chain(&p.denies) {
        scope_validate(tx, s).await?;
    }
    let version = Uuid::now_v7();
    sqlx::query("INSERT INTO advertisement_policy_versions(id,ad_id,revision,policy,actor_id,reason) VALUES($1,$2,$3,$4,$5,$6)").bind(version).bind(id).bind(revision).bind(json!(p)).bind(actor).bind(reason).execute(&mut **tx).await?;
    sqlx::query("UPDATE advertisements SET policy_version=$2,revision=$3 WHERE id=$1")
        .bind(id)
        .bind(version)
        .bind(revision)
        .execute(&mut **tx)
        .await?;
    enqueue(tx, Some(id), None, chrono::Utc::now()).await?;
    for due in [Some(p.valid_from), p.valid_until]
        .into_iter()
        .flatten()
        .filter(|t| *t > chrono::Utc::now())
    {
        enqueue(tx, Some(id), None, due).await?;
    }
    Ok(version)
}
pub(crate) async fn enqueue(
    tx: &mut Tx<'_>,
    ad: Option<Uuid>,
    route: Option<&str>,
    due: chrono::DateTime<chrono::Utc>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO advertisement_recheck_jobs(id,ad_id,route_id,due) VALUES($1,$2,$3,$4)",
    )
    .bind(Uuid::now_v7())
    .bind(ad)
    .bind(route)
    .bind(due)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
impl AdvertisingStore for Repository {
    async fn execute(
        &self,
        actor: &Actor,
        command: Command,
    ) -> crate::advertising::domain::Result<Value> {
        self.execute_inner(actor, command)
            .await
            .map_err(|e| crate::advertising::domain::AdvertisingError(e.0))
    }
}
impl Repository {
    async fn execute_inner(&self, actor: &Actor, command: Command) -> Result<Value> {
        if let Command::Preview(route, input) = command {
            return preview(&self.0, actor, &route, input).await;
        }
        let mut tx = self.0.begin().await?;
        guard(&mut tx).await?;
        // Mutations take their final row lock before reading the asset, avoiding SHARE upgrades.
        if let Command::Content(id, _) | Command::Policy(id, _) | Command::Archive(id, _) = &command
        {
            sqlx::query("SELECT id FROM advertisements WHERE id=$1 FOR UPDATE")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(Fault("not_found"))?;
        }
        let admin = routes::administrator(&mut tx, actor.id).await?;
        let action = match &command {
            Command::Create(_) => "create",
            Command::Content(..) => "publish_content",
            Command::Policy(..) => "publish_policy",
            Command::Archive(..) => "archive",
            Command::Manager(..) => "manager",
            Command::Configure(..) => "configure",
            Command::PublishPool(..) => "publish_pool",
            Command::ResetPool(..) => "reset_pool",
            Command::CopyPool(..) => "copy_pool",
            Command::Preview(..) => "preview",
            _ => "read",
        };
        let mut audit_context = match &command {
            Command::Content(id, v) => {
                json!({"ad_id":id,"expected_revision":v.expected_revision,"reason":v.reason,"body_digest":crate::advertising::domain::digest(v.content.body.as_bytes())})
            }
            Command::Policy(id, v) => {
                json!({"ad_id":id,"expected_revision":v.expected_revision,"reason":v.reason,"policy":v.policy})
            }
            Command::Archive(id, v) => {
                json!({"ad_id":id,"expected_revision":v.expected_revision,"reason":v.reason})
            }
            Command::Manager(unit, v) => {
                json!({"unit_id":unit,"username":v.username,"can_edit":v.can_edit,"can_share":v.can_share,"expected_revision":v.expected_revision})
            }
            Command::Configure(route, v) => {
                json!({"route_id":route,"expected_revision":v.expected_revision,"reason":v.reason,"config":v.config})
            }
            Command::Compositions(route, receipt, page) => {
                json!({"route_id":route,"receipt_id":receipt,"page":page})
            }
            Command::Create(v) => json!({"owner_unit_id":v.owner_unit_id,"reason":v.reason}),
            Command::Detail(id) | Command::Versions(id, _) => json!({"ad_id":id}),
            Command::Candidates(route, _) | Command::Config(route) => json!({"route_id":route}),
            Command::Managers(unit) => json!({"unit_id":unit}),
            _ => json!({}),
        };
        let value = match command {
            Command::Directory => {
                let units:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',u.id,'name',u.name,'can_edit',$2 OR coalesce(m.can_edit,false),'can_share',$2 OR coalesce(m.can_share,false)) FROM routing_units u LEFT JOIN advertisement_unit_managers m ON m.unit_id=u.id AND m.account_id=$1 WHERE u.active ORDER BY u.name,u.id").bind(actor.id).bind(admin).fetch_all(&mut *tx).await?;
                let author = admin
                    || units
                        .iter()
                        .any(|u| u["can_edit"] == true || u["can_share"] == true);
                let own_routes = available_routes(&mut tx, actor.id, admin).await?;
                let mut scopes: Vec<Value> = vec![];
                let mut workspaces: Vec<Value> = vec![];
                if author {
                    scopes=sqlx::query_scalar("SELECT jsonb_build_object('kind','unit','id',id,'name',name) FROM routing_units WHERE active UNION ALL SELECT jsonb_build_object('kind','group','id',id,'name',name) FROM routing_groups WHERE active UNION ALL SELECT jsonb_build_object('kind','route','id',route_id,'name',route_name) FROM source_routes WHERE retired_at IS NULL").fetch_all(&mut *tx).await?;
                    workspaces=sqlx::query_scalar("SELECT jsonb_build_object('id',id,'channel','mobile','name',name) FROM mobile_workspaces WHERE active UNION ALL SELECT jsonb_build_object('id',id,'channel','wecom','name',name) FROM wecom_workspaces WHERE active").fetch_all(&mut *tx).await?;
                }
                json!({"units":units.into_iter().filter(|u|admin||u["can_edit"]==true||u["can_share"]==true).collect::<Vec<_>>(),"scopes":scopes,"workspaces":workspaces,"routes":own_routes.iter().map(|r|json!({"id":r.route_id,"name":r.route_id,"execution":r.execution})).collect::<Vec<_>>(),"system_admin":admin})
            }
            Command::List { page, filter } => {
                ensure(
                    matches!(filter.as_str(), "all" | "owned" | "shared" | "archived"),
                    "invalid_request",
                )?;
                let snapshots = available_routes(&mut tx, actor.id, admin).await?;
                let ids = visible_ids(&mut tx, actor.id, admin, &snapshots, &filter, page).await?;
                let mut items = Vec::new();
                for id in ids.iter().take(30) {
                    let r = ad_row(&mut tx, *id).await?;
                    let (edit, share, usable) = visible(&mut tx, actor.id, admin, &r).await?;
                    items.push(view(&r, edit, share, usable));
                }
                json!({"items":items,"has_more":ids.len()>30})
            }
            Command::Create(v) => {
                ensure(
                    capabilities(&mut tx, actor.id, admin, v.owner_unit_id)
                        .await?
                        .0,
                    "forbidden",
                )?;
                ensure(
                    sqlx::query_scalar::<_, bool>(
                        "SELECT EXISTS(SELECT 1 FROM routing_units WHERE id=$1 AND active)",
                    )
                    .bind(v.owner_unit_id)
                    .fetch_one(&mut *tx)
                    .await?,
                    "invalid_ad_scope",
                )?;
                let id = Uuid::now_v7();
                sqlx::query(
                    "INSERT INTO advertisements(id,owner_unit_id,created_by) VALUES($1,$2,$3)",
                )
                .bind(id)
                .bind(v.owner_unit_id)
                .bind(actor.id)
                .execute(&mut *tx)
                .await?;
                publish_content(&mut tx, actor.id, id, 1, &v.content, &v.reason).await?;
                for s in &v.workspaces {
                    ensure(
                        s.kind == "unit" && s.id == v.owner_unit_id.to_string(),
                        "invalid_ad_scope",
                    )?;
                }
                let p = Policy {
                    enabled: true,
                    valid_from: chrono::Utc::now(),
                    valid_until: None,
                    allows: v.workspaces,
                    denies: vec![],
                };
                publish_policy(&mut tx, actor.id, id, 2, &p, &v.reason).await?;
                json!({"id":id,"revision":2})
            }
            Command::Detail(id) => {
                let r = ad_row(&mut tx, id).await?;
                let (e, s, u) = visible(&mut tx, actor.id, admin, &r).await?;
                let mut v = view(&r, e, s, u);
                let refs:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('route_id',route_id,'revision',revision) FROM source_advertisement_configs c WHERE config->>'fixed_ad_id'=$1 OR EXISTS(SELECT 1 FROM advertisement_pools ap JOIN advertisement_pool_slots ps ON ps.pool_id=ap.id AND ps.revision=ap.current_revision WHERE ap.route_id=c.route_id AND ps.ad_id::text=$1)").bind(id.to_string()).fetch_all(&mut *tx).await?;
                let mut visible_refs = vec![];
                for r in refs {
                    match routes::access(
                        &mut tx,
                        actor.id,
                        r["route_id"].as_str().unwrap_or(""),
                        admin,
                    )
                    .await
                    {
                        Ok(()) => visible_refs.push(r),
                        Err(Fault("forbidden" | "not_found")) => {}
                        Err(e) => return Err(e),
                    }
                }
                v["references"] = json!(visible_refs);
                v
            }
            Command::Versions(id, page) => {
                let r = ad_row(&mut tx, id).await?;
                let (edit, share, _) = visible(&mut tx, actor.id, admin, &r).await?;
                ensure(edit || share, "forbidden")?;
                let rows:Vec<Value>=sqlx::query_scalar("SELECT value FROM (SELECT jsonb_build_object('kind','content','revision',revision,'value',to_jsonb(v)) AS value FROM advertisement_content_versions v WHERE ad_id=$1 UNION ALL SELECT jsonb_build_object('kind','policy','revision',revision,'value',to_jsonb(v)) FROM advertisement_policy_versions v WHERE ad_id=$1) versions ORDER BY (value->>'revision')::bigint DESC LIMIT 31 OFFSET $2").bind(id).bind((page-1)*30).fetch_all(&mut *tx).await?;
                json!({"items":rows.iter().take(30).collect::<Vec<_>>(),"has_more":rows.len()>30})
            }
            Command::Content(id, v) => {
                let r = ad_row(&mut tx, id).await?;
                ensure(
                    capabilities(&mut tx, actor.id, admin, r.get("owner_unit_id"))
                        .await?
                        .0,
                    "forbidden",
                )?;
                ensure(!r.get::<bool, _>("archived"), "ad_archived")?;
                let n = revision(&mut tx, id, v.expected_revision).await?;
                let version =
                    publish_content(&mut tx, actor.id, id, n, &v.content, &v.reason).await?;
                json!({"id":id,"revision":n,"version_id":version})
            }
            Command::Policy(id, v) => {
                let r = ad_row(&mut tx, id).await?;
                ensure(
                    capabilities(&mut tx, actor.id, admin, r.get("owner_unit_id"))
                        .await?
                        .1,
                    "forbidden",
                )?;
                ensure(!r.get::<bool, _>("archived"), "ad_archived")?;
                let n = revision(&mut tx, id, v.expected_revision).await?;
                let version =
                    publish_policy(&mut tx, actor.id, id, n, &v.policy, &v.reason).await?;
                json!({"id":id,"revision":n,"version_id":version})
            }
            Command::Archive(id, v) => {
                let r = ad_row(&mut tx, id).await?;
                ensure(
                    capabilities(&mut tx, actor.id, admin, r.get("owner_unit_id"))
                        .await?
                        .0,
                    "forbidden",
                )?;
                let n = revision(&mut tx, id, v.expected_revision).await?;
                sqlx::query("UPDATE advertisements SET archived=true,revision=$2 WHERE id=$1")
                    .bind(id)
                    .bind(n)
                    .execute(&mut *tx)
                    .await?;
                enqueue(&mut tx, Some(id), None, chrono::Utc::now()).await?;
                json!({"id":id,"revision":n})
            }
            Command::Managers(unit) => {
                ensure(admin, "forbidden")?;
                let items:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('account_id',m.account_id,'username',a.username,'revision',m.revision,'can_edit',m.can_edit,'can_share',m.can_share) FROM advertisement_unit_managers m JOIN accounts a ON a.id=m.account_id WHERE m.unit_id=$1 ORDER BY a.username").bind(unit).fetch_all(&mut *tx).await?;
                json!({"items":items})
            }
            Command::Manager(unit, v) => {
                ensure(admin, "forbidden")?;
                let account: Uuid =
                    sqlx::query_scalar("SELECT id FROM accounts WHERE username=$1 AND active")
                        .bind(&v.username)
                        .fetch_optional(&mut *tx)
                        .await?
                        .ok_or(Fault("not_found"))?;
                let old:Option<i64>=sqlx::query_scalar("SELECT revision FROM advertisement_unit_managers WHERE unit_id=$1 AND account_id=$2 FOR UPDATE").bind(unit).bind(account).fetch_optional(&mut *tx).await?;
                ensure(old.unwrap_or(0) == v.expected_revision, "revision_conflict")?;
                sqlx::query("INSERT INTO advertisement_unit_managers(unit_id,account_id,can_edit,can_share,revision,granted_by) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(unit_id,account_id) DO UPDATE SET can_edit=$3,can_share=$4,revision=$5,granted_by=$6").bind(unit).bind(account).bind(v.can_edit).bind(v.can_share).bind(v.expected_revision+1).bind(actor.id).execute(&mut *tx).await?;
                {
                    enqueue(&mut tx, None, None, chrono::Utc::now()).await?;
                    json!({"id":unit,"revision":v.expected_revision+1})
                }
            }
            Command::Candidates(route, page) => {
                routes::access(&mut tx, actor.id, &route, admin).await?;
                let snapshot = route_snapshot(&mut tx, &route).await?;
                let ids =
                    visible_ids(&mut tx, actor.id, false, &[snapshot], "candidates", page).await?;
                let mut items = vec![];
                for id in ids.iter().take(30) {
                    let r = ad_row(&mut tx, *id).await?;
                    items.push(view(&r, false, false, true));
                }
                json!({"items":items,"has_more":ids.len()>30})
            }
            Command::Pool(route) => {
                routes::access(&mut tx, actor.id, &route, admin).await?;
                pools::view(&mut tx, &route).await?
            }
            Command::PublishPool(route, input) => {
                routes::access(&mut tx, actor.id, &route, admin).await?;
                pools::publish(&mut tx, actor.id, &route, input, "publish").await?
            }
            Command::ResetPool(route, input) => {
                routes::access(&mut tx, actor.id, &route, admin).await?;
                pools::reset(&mut tx, actor.id, &route, input).await?
            }
            Command::CopyPool(route, input) => {
                ensure(route != input.target_route_id, "ad_pool_wrong_route")?;
                routes::access(&mut tx, actor.id, &route, admin).await?;
                routes::access(&mut tx, actor.id, &input.target_route_id, admin).await?;
                let original = pools::view(&mut tx, &route).await?;
                ensure(original["pool_id"].is_string(), "ad_pool_missing")?;
                let slots: Vec<pool::Slot> = serde_json::from_value(original["slots"].clone())?;
                // A copy always has new slot identities and begins at the first slot.
                ensure(
                    sqlx::query_scalar::<_, bool>(
                        "SELECT NOT EXISTS(SELECT 1 FROM advertisement_pools WHERE route_id=$1)",
                    )
                    .bind(&input.target_route_id)
                    .fetch_one(&mut *tx)
                    .await?,
                    "ad_pool_copy_target_exists",
                )?;
                pools::publish(
                    &mut tx,
                    actor.id,
                    &input.target_route_id,
                    pool::Publication {
                        expected_revision: input.expected_revision,
                        reason: input.reason,
                        slots: slots
                            .into_iter()
                            .map(|s| pool::SlotInput {
                                slot_id: None,
                                ad_id: s.ad_id,
                            })
                            .collect(),
                    },
                    "copy",
                )
                .await?
            }
            Command::Consumptions(route, page) => {
                routes::access(&mut tx, actor.id, &route, admin).await?;
                pools::consumptions(&mut tx, &route, page).await?
            }
            Command::Config(route) => {
                routes::access(&mut tx, actor.id, &route, admin).await?;
                let (revision, config) = pipeline::config(&mut tx, &route).await?;
                json!({"id":route,"revision":revision,"config":config})
            }
            Command::Configure(route, v) => {
                routes::access(&mut tx, actor.id, &route, admin).await?;
                sqlx::query("SELECT route_id FROM source_advertisement_configs WHERE route_id=$1 FOR UPDATE").bind(&route).fetch_one(&mut *tx).await?;
                let (n, _) = pipeline::config(&mut tx, &route).await?;
                if v.config.mode == "pool" {
                    ensure(
                        sqlx::query_scalar::<_, bool>(
                            "SELECT EXISTS(SELECT 1 FROM advertisement_pools WHERE route_id=$1)",
                        )
                        .bind(&route)
                        .fetch_one(&mut *tx)
                        .await?,
                        "ad_pool_missing",
                    )?;
                }
                ensure(n == v.expected_revision, "revision_conflict")?;
                if let Some(id) = v.config.fixed_ad_id {
                    let r = ad_row(&mut tx, id).await?;
                    ensure(
                        ad_exclusion(&r, &route_snapshot(&mut tx, &route).await?)?.is_none(),
                        "ad_not_allowed",
                    )?;
                }
                sqlx::query("UPDATE source_advertisement_configs SET revision=revision+1,config=$2 WHERE route_id=$1").bind(&route).bind(json!(v.config)).execute(&mut *tx).await?;
                sqlx::query("INSERT INTO source_advertisement_config_versions(route_id,revision,config,actor_id,reason) VALUES($1,$2,$3,$4,$5)").bind(&route).bind(n+1).bind(json!(v.config)).bind(actor.id).bind(v.reason).execute(&mut *tx).await?;
                enqueue(&mut tx, None, Some(&route), chrono::Utc::now()).await?;
                json!({"id":route,"revision":n+1})
            }
            Command::Preview(..) => unreachable!("preview handled before transaction"),
            Command::Compositions(route, receipt, page) => {
                routes::access(&mut tx, actor.id, &route, admin).await?;
                ensure(sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM source_receipts WHERE receipt_id=$1 AND route_id=$2)").bind(receipt).bind(&route).fetch_one(&mut *tx).await?,"receipt_not_found")?;
                let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(c) FROM source_message_compositions c WHERE receipt_id=$1 ORDER BY generation DESC LIMIT 31 OFFSET $2").bind(receipt).bind((page-1)*30).fetch_all(&mut *tx).await?;
                let generations:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(g) FROM source_execution_generations g WHERE receipt_id=$1 ORDER BY generation DESC LIMIT 31 OFFSET $2").bind(receipt).bind((page-1)*30).fetch_all(&mut *tx).await?;
                let decisions:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(d) FROM source_advertisement_decisions d WHERE receipt_id=$1 ORDER BY created_at DESC,id DESC LIMIT 31 OFFSET $2").bind(receipt).bind((page-1)*30).fetch_all(&mut *tx).await?;
                json!({"items":rows.iter().take(30).collect::<Vec<_>>(),"has_more":rows.len()>30||decisions.len()>30,"decisions":decisions.iter().take(30).collect::<Vec<_>>(),"generations":generations})
            }
        };
        audit_context["action"] = json!(action);
        audit_context["object_id"] = value.get("id").cloned().unwrap_or(Value::Null);
        audit_context["revision"] = value.get("revision").cloned().unwrap_or(Value::Null);
        audit_context["version_id"] = value.get("version_id").cloned().unwrap_or(Value::Null);
        crate::audit::record(
            &mut tx,
            actor,
            Uuid::nil(),
            "advertising.access",
            actor.id,
            audit_context,
        )
        .await
        .map_err(|_| Fault("temporarily_unavailable"))?;
        tx.commit().await?;
        Ok(value)
    }
}

/// Read-only preview uses the same original → selection → final ordering as the inbox.
/// Matching runs without an open transaction; the returned rule fingerprints identify this preview only.
async fn preview(pool: &PgPool, actor: &Actor, route: &str, input: Preview) -> Result<Value> {
    use crate::source_access::{
        domain::keyword_filter as k, south::adapter::postgres::keyword_filter as checks,
    };
    let mut tx = pool.begin().await?;
    let admin = routes::administrator(&mut tx, actor.id).await?;
    routes::access(&mut tx, actor.id, route, admin).await?;
    let snapshot = route_snapshot(&mut tx, route).await?;
    let (common_ctx, common_rules) = checks::context_for_scope(
        &mut tx,
        snapshot.source_id,
        route.into(),
        Some(snapshot.unit_id),
    )
    .await?;
    tx.commit().await?;
    let fields = [
        ("title", input.title.as_deref().unwrap_or("")),
        ("content", input.content.as_deref().unwrap_or("")),
    ];
    let common = k::evaluate(&common_rules, &fields)?;
    let mut selection = None;
    let mut content = None;
    if common.passed() {
        let source = k::compose(
            input.title.as_deref(),
            input.content.as_deref(),
            &input.detail_url,
        );
        let mut tx = pool.begin().await?;
        guard(&mut tx).await?;
        let admin = routes::administrator(&mut tx, actor.id).await?;
        routes::access(&mut tx, actor.id, route, admin).await?;
        ensure(
            route_snapshot(&mut tx, route).await?.revision == snapshot.revision,
            "ad_selection_stale",
        )?;
        let (revision, config) = pipeline::config(&mut tx, route).await?;
        let p = if config.mode == "pool" {
            tx.commit().await?;
            pools::select(pool, &snapshot, revision, config, &source, None).await?
        } else {
            let prepared =
                pipeline::select(&mut tx, &snapshot, revision, config, &source, None).await?;
            tx.commit().await?;
            prepared
        };
        content = Some(p.content);
        selection = Some(p.selection);
    }
    let mut tx = pool.begin().await?;
    crate::audit::record(&mut tx, actor, Uuid::nil(), "advertising.access", actor.id,
        json!({"action":"preview","route_id":route,"common_fingerprint":common_ctx.fingerprint(),"effective_fingerprint":common_ctx.fingerprint()})).await.map_err(|_|Fault("temporarily_unavailable"))?;
    tx.commit().await?;
    Ok(
        json!({"content":content,"selection":selection,"checks":{"common":common,"original":null,"final":null,"excluded_final":null},"rule_fingerprint":common_ctx.fingerprint(),"deliverable":common.passed() && selection.as_ref().is_some_and(|s|!s.unavailable()),"preview_only":true}),
    )
}

/// Apply disclosure predicates before pagination. Runtime selection still validates the
/// policy with the domain implementation under the submission transaction.
async fn visible_ids(
    tx: &mut Tx<'_>,
    actor: Uuid,
    admin: bool,
    snapshots: &[crate::source_access::domain::routing::RouteSnapshot],
    filter: &str,
    page: i64,
) -> Result<Vec<Uuid>> {
    let predicate = "scope->>'id'=CASE scope->>'kind' WHEN 'unit' THEN snapshot->>'unit_id' WHEN 'group' THEN snapshot->>'group_id' WHEN 'route' THEN snapshot->>'route_id' END AND (scope->>'channel' IS NULL OR scope->>'channel'=snapshot #>> '{execution,channel}') AND (scope->>'workspace_id' IS NULL OR scope->>'workspace_id'=snapshot #>> '{execution,workspace_id}')";
    let sql = format!(
        "WITH visibility AS (SELECT a.id,a.created_at,a.archived,($2 OR EXISTS(SELECT 1 FROM advertisement_unit_managers m WHERE m.unit_id=a.owner_unit_id AND m.account_id=$1 AND (m.can_edit OR m.can_share))) AS owned, (NOT a.archived AND (p.policy->>'enabled')::boolean AND (p.policy->>'valid_from')::timestamptz<=clock_timestamp() AND (p.policy->>'valid_until' IS NULL OR (p.policy->>'valid_until')::timestamptz>clock_timestamp()) AND EXISTS(SELECT 1 FROM jsonb_array_elements($3::jsonb) snapshot WHERE EXISTS(SELECT 1 FROM jsonb_array_elements(p.policy->'allows') scope WHERE {predicate}) AND NOT EXISTS(SELECT 1 FROM jsonb_array_elements(p.policy->'denies') scope WHERE {predicate}))) AS usable FROM advertisements a LEFT JOIN advertisement_policy_versions p ON p.id=a.policy_version) SELECT id FROM visibility WHERE ($4='archived' OR NOT archived) AND CASE $4 WHEN 'archived' THEN owned AND archived WHEN 'owned' THEN owned WHEN 'shared' THEN NOT owned AND usable WHEN 'candidates' THEN usable ELSE owned OR usable END ORDER BY created_at DESC,id DESC LIMIT 31 OFFSET $5"
    );
    Ok(sqlx::query_scalar(&sql)
        .bind(actor)
        .bind(admin)
        .bind(json!(snapshots))
        .bind(filter)
        .bind((page - 1) * 30)
        .fetch_all(&mut **tx)
        .await?)
}
