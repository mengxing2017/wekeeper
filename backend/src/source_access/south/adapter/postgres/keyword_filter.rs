use super::*;
use crate::source_access::{domain::keyword_filter as k, south::port::KeywordStore};
type Tx<'a> = Transaction<'a, Postgres>;
#[derive(Clone)]
pub struct KeywordRepository(pub PgPool);
async fn guard(tx: &mut Tx<'_>) -> Result<()> {
    crate::distribution::south::adapter::postgres::routing::source_guard(tx)
        .await
        .map_err(|_| Fault("temporarily_unavailable"))
}
async fn source_lock(tx: &mut Tx<'_>, receipt: Uuid) -> Result<()> {
    sqlx::query("SELECT source_id FROM source_callers WHERE source_id=(SELECT source_id FROM source_receipts WHERE receipt_id=$1) FOR SHARE").bind(receipt).fetch_one(&mut **tx).await?;
    sqlx::query("SELECT route_id FROM source_routes WHERE route_id=(SELECT route_id FROM source_receipts WHERE receipt_id=$1) FOR UPDATE").bind(receipt).fetch_optional(&mut **tx).await?;
    Ok(())
}
/// Every applicable scope must exist. Missing storage is never an empty rule set.
pub(crate) async fn context(tx: &mut Tx<'_>, receipt: Uuid) -> Result<(k::Context, Vec<k::Rule>)> {
    let row=sqlx::query("SELECT r.source_id,r.route_id,CASE WHEN d.receipt_id IS NOT NULL THEN (d.snapshot->>'unit_id')::uuid ELSE s.unit_id END AS unit_id FROM source_receipts r LEFT JOIN source_routes s USING(route_id) LEFT JOIN source_current_routing_decisions d USING(receipt_id) WHERE r.receipt_id=$1").bind(receipt).fetch_one(&mut **tx).await?;
    let source: Uuid = row.get("source_id");
    let route: String = row.get("route_id");
    let unit: Option<Uuid> = row.get("unit_id");
    context_for_scope(tx, source, route, unit).await
}
pub(crate) async fn context_for_scope(
    tx: &mut Tx<'_>,
    source: Uuid,
    route: String,
    unit: Option<Uuid>,
) -> Result<(k::Context, Vec<k::Rule>)> {
    let ids = vec!["global".to_owned()];
    let rows=sqlx::query("SELECT h.id,h.revision,h.version_id,v.entries FROM keyword_scope_heads h JOIN keyword_rule_versions v ON v.id=h.version_id WHERE h.id=ANY($1) ORDER BY h.id FOR SHARE OF h").bind(&ids).fetch_all(&mut **tx).await?;
    ensure(rows.len() == ids.len(), "keyword_rules_unavailable")?;
    let mut versions = vec![];
    let mut rules = vec![];
    for row in rows {
        let scope: String = row.get("id");
        let version: Uuid = row.get("version_id");
        versions.push(k::ScopeVersion {
            scope_id: scope.clone(),
            revision: row.get("revision"),
            version_id: version,
        });
        let entries: Vec<k::Entry> = serde_json::from_value(row.get("entries"))?;
        // Legacy imports retain every nonempty restriction, with deterministic normalized deduplication.
        let mut seen = std::collections::BTreeSet::new();
        for entry in entries {
            if entry.enabled {
                let normalized = k::normalize(&entry.word);
                ensure(!normalized.trim().is_empty(), "invalid_keyword")?;
                if seen.insert(normalized.clone()) {
                    rules.push(k::Rule {
                        id: digest(format!("{scope}\n{normalized}").as_bytes()),
                        scope_id: scope.clone(),
                        version_id: version,
                        word: entry.word,
                    });
                }
            }
        }
    }
    ensure(rules.len() <= 10000, "keyword_limit_exceeded")?;
    Ok((
        k::Context {
            source_id: source,
            route_id: route,
            unit_id: unit,
            channel: None,
            workspace_id: None,
            versions,
        },
        rules,
    ))
}
pub(crate) async fn check(
    pool: &PgPool,
    receipt: Uuid,
    event: &Event,
    stage: &str,
    fields: &[(&str, &str)],
) -> Result<(Uuid, bool, String, k::Context)> {
    let payload = crate::source_access::domain::projection::fingerprint(event);
    let mut tx = pool.begin().await?;
    let (ctx, rules) = context(&mut tx, receipt).await?;
    let fingerprint = ctx.fingerprint();
    let input = digest(&serde_json::to_vec(fields)?);
    let old:Option<(Uuid,String)>=sqlx::query_as("SELECT id,outcome FROM keyword_check_results WHERE receipt_id=$1 AND stage=$2 AND payload_fingerprint=$3 AND effective_fingerprint=$4 AND input_digest=$5 AND algorithm=$6").bind(receipt).bind(stage).bind(&payload).bind(&fingerprint).bind(&input).bind(k::ALGORITHM).fetch_optional(&mut *tx).await?;
    tx.commit().await?;
    if let Some((id, outcome)) = old {
        return Ok((id, outcome == "passed", fingerprint, ctx));
    }
    let result = k::evaluate(&rules, fields)?;
    let mut tx = pool.begin().await?;
    guard(&mut tx).await?;
    source_lock(&mut tx, receipt).await?;
    let (latest, _) = context(&mut tx, receipt).await?;
    ensure(latest.fingerprint() == fingerprint, "content_check_pending")?;
    let current:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM source_content_versions WHERE receipt_id=$1 AND NOT withdrawn AND fingerprint=$2)").bind(receipt).bind(&payload).fetch_one(&mut *tx).await?;
    ensure(current, "content_version_superseded")?;
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO keyword_check_results(id,receipt_id,stage,payload_fingerprint,context,effective_fingerprint,input_digest,normalized_digest,algorithm,unicode_version,outcome,total_hits,matched_rules,samples,truncated) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15) ON CONFLICT DO NOTHING").bind(id).bind(receipt).bind(stage).bind(&payload).bind(json!(ctx)).bind(&fingerprint).bind(&result.input_digest).bind(&result.normalized_digest).bind(k::ALGORITHM).bind(k::UNICODE_VERSION).bind(if result.passed(){"passed"}else{"blocked"}).bind(result.total_hits).bind(json!(result.matched_rules)).bind(json!(result.samples)).bind(result.truncated).execute(&mut *tx).await?;
    let id:Uuid=sqlx::query_scalar("SELECT id FROM keyword_check_results WHERE receipt_id=$1 AND stage=$2 AND payload_fingerprint=$3 AND effective_fingerprint=$4 AND input_digest=$5 AND algorithm=$6").bind(receipt).bind(stage).bind(payload).bind(&fingerprint).bind(input).bind(k::ALGORITHM).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok((id, result.passed(), fingerprint, ctx))
}
pub(super) async fn common(pool: &PgPool, receipt: Uuid, event: &Event) -> Result<bool> {
    let existing: Option<(String, bool, bool)> = sqlx::query_as("SELECT v.check_state,v.no_resend,r.keyword_terminal_rejected FROM source_content_versions v JOIN source_receipts r USING(receipt_id) WHERE v.receipt_id=$1 AND NOT v.withdrawn")
        .bind(receipt).fetch_optional(pool).await?;
    if let Some((state, no_resend, terminal)) = existing {
        if terminal {
            return Ok(false);
        }
        if state == "passed" {
            return Ok(!no_resend);
        }
    }
    let (_, passed, fingerprint, _) = check(
        pool,
        receipt,
        event,
        "original_common",
        &[
            ("title", event.title.as_deref().unwrap_or("")),
            ("content", event.content.as_deref().unwrap_or("")),
        ],
    )
    .await?;
    let mut tx = pool.begin().await?;
    guard(&mut tx).await?;
    source_lock(&mut tx, receipt).await?;
    ensure(
        context(&mut tx, receipt).await?.0.fingerprint() == fingerprint,
        "content_check_pending",
    )?;
    let terminal: bool = sqlx::query_scalar(
        "SELECT keyword_terminal_rejected FROM source_receipts WHERE receipt_id=$1 FOR UPDATE",
    )
    .bind(receipt)
    .fetch_one(&mut *tx)
    .await?;
    if terminal {
        tx.commit().await?;
        return Ok(false);
    }
    let no_resend:Option<bool>=sqlx::query_scalar("SELECT no_resend FROM source_content_versions WHERE receipt_id=$1 AND NOT withdrawn FOR UPDATE").bind(receipt).fetch_optional(&mut *tx).await?;
    let no_resend = no_resend.ok_or(Fault("content_version_superseded"))?;
    sqlx::query(
        "UPDATE source_content_versions SET check_state=$2,check_revision=$3 WHERE receipt_id=$1",
    )
    .bind(receipt)
    .bind(if passed { "passed" } else { "blocked" })
    .bind(&fingerprint)
    .execute(&mut *tx)
    .await?;
    if !passed {
        sqlx::query(
            "UPDATE source_receipts SET keyword_terminal_rejected=true WHERE receipt_id=$1",
        )
        .bind(receipt)
        .execute(&mut *tx)
        .await?;
    }
    super::projection::transition(
        &mut tx,
        receipt,
        if passed { "applied" } else { "rejected" },
        if !passed {
            "rejected"
        } else if no_resend {
            "no_resend"
        } else {
            "projection_applied"
        },
        if passed {
            None
        } else {
            Some("keyword_blocked")
        },
        if !passed {
            "rejected"
        } else if no_resend {
            "no_resend"
        } else {
            "waiting"
        },
    )
    .await?;
    sqlx::query("DELETE FROM source_recheck_requests WHERE receipt_id=$1")
        .bind(receipt)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(passed && !no_resend)
}
async fn scope_access(tx: &mut Tx<'_>, actor: Uuid, admin: bool, scope: &str) -> Result<()> {
    let row = sqlx::query("SELECT * FROM keyword_scope_heads WHERE id=$1")
        .bind(scope)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(Fault("not_found"))?;
    let _ = (tx, actor);
    ensure(admin && row.get::<String, _>("id") == "global", "forbidden")?;
    Ok(())
}
async fn receipt_access(
    tx: &mut Tx<'_>,
    actor: Uuid,
    admin: bool,
    route: &str,
    receipt: Uuid,
) -> Result<()> {
    super::routing::access(tx, actor, route, admin).await?;
    ensure(
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM source_receipts WHERE receipt_id=$1 AND route_id=$2)",
        )
        .bind(receipt)
        .bind(route)
        .fetch_one(&mut **tx)
        .await?,
        "receipt_not_found",
    )
}
impl KeywordStore for KeywordRepository {
    async fn execute(&self, actor: &crate::business::Actor, command: k::Command) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        if matches!(&command, k::Command::Publish { .. }) {
            guard(&mut tx).await?;
        }
        let admin = super::routing::administrator(&mut tx, actor.id).await?;
        let audit_context = match &command {
            k::Command::Scopes => json!({"action":"list_scopes"}),
            k::Command::Versions { scope, .. } => json!({"action":"versions","scope_id":scope}),
            k::Command::Publish { scope, .. } => json!({"action":"publish","scope_id":scope}),
            k::Command::Checks { route, receipt, .. } => {
                json!({"action":"checks","route_id":route,"receipt_id":receipt})
            }
            k::Command::Hits {
                route,
                receipt,
                check,
                ..
            } => json!({"action":"hits","route_id":route,"receipt_id":receipt,"check_id":check}),
        };
        let value = match command {
            k::Command::Scopes => {
                let rows=sqlx::query("SELECT h.*,COALESCE((SELECT bool_and(b.complete) FROM keyword_recheck_batches b WHERE b.scope_id=h.id),true) AS recheck_complete FROM keyword_scope_heads h WHERE h.id = 'global' ORDER BY h.id").fetch_all(&mut *tx).await?;
                let mut items = vec![];
                for r in rows {
                    let id: String = r.get("id");
                    match scope_access(&mut tx,actor.id,admin,&id).await{Ok(())=>items.push(json!({"id":id,"kind":r.get::<String,_>("kind"),"object_id":r.get::<String,_>("object_id"),"channel":r.get::<Option<String>,_>("channel"),"label":r.get::<String,_>("label"),"revision":r.get::<i64,_>("revision"),"version_id":r.get::<Uuid,_>("version_id"),"can_edit":true,"recheck_complete":r.get::<bool,_>("recheck_complete")})),Err(Fault("forbidden"))=>{},Err(e)=>return Err(e)}
                }
                json!({"items":items})
            }
            k::Command::Versions { scope, page } => {
                scope_access(&mut tx, actor.id, admin, &scope).await?;
                let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(v) FROM keyword_rule_versions v WHERE scope_id=$1 ORDER BY revision DESC LIMIT 31 OFFSET $2").bind(&scope).bind((page-1)*30).fetch_all(&mut *tx).await?;
                let items: Vec<Value> = rows
                    .iter()
                    .take(30)
                    .map(|v| {
                        let mut v = v.clone();
                        if let Some(entries) = v["entries"].as_array_mut() {
                            for entry in entries {
                                entry["normalized"] =
                                    json!(k::normalize(entry["word"].as_str().unwrap_or("")));
                            }
                        }
                        v
                    })
                    .collect();
                json!({"items":items,"has_more":rows.len()>30,"algorithm":k::ALGORITHM,"unicode_version":k::UNICODE_VERSION})
            }
            k::Command::Publish { scope, input } => {
                scope_access(&mut tx, actor.id, admin, &scope).await?;
                k::validate(&input.entries)?;
                let revision: i64 = sqlx::query_scalar(
                    "SELECT revision FROM keyword_scope_heads WHERE id=$1 FOR UPDATE",
                )
                .bind(&scope)
                .fetch_one(&mut *tx)
                .await?;
                ensure(revision == input.expected_revision, "revision_conflict")?;
                let version = Uuid::now_v7();
                let batch = Uuid::now_v7();
                sqlx::query("INSERT INTO keyword_rule_versions(id,scope_id,revision,entries,actor_id,reason) VALUES($1,$2,$3,$4,$5,$6)").bind(version).bind(&scope).bind(revision+1).bind(json!(input.entries)).bind(actor.id).bind(input.reason.trim()).execute(&mut *tx).await?;
                sqlx::query(
                    "UPDATE keyword_scope_heads SET revision=revision+1,version_id=$2 WHERE id=$1",
                )
                .bind(&scope)
                .bind(version)
                .execute(&mut *tx)
                .await?;

                json!({"id":scope,"revision":revision+1,"version_id":version,"batch_id":batch})
            }
            k::Command::Checks {
                route,
                receipt,
                page,
            } => {
                receipt_access(&mut tx, actor.id, admin, &route, receipt).await?;
                let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(c)-'matched_rules'-'samples' FROM keyword_check_results c WHERE receipt_id=$1 ORDER BY created_at DESC,id DESC LIMIT 31 OFFSET $2").bind(receipt).bind((page-1)*30).fetch_all(&mut *tx).await?;
                let items = rows
                    .iter()
                    .take(30)
                    .map(|v| serde_json::from_value::<k::CheckRecord>(v.clone()))
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                json!({"items":items,"has_more":rows.len()>30})
            }
            k::Command::Hits {
                route,
                receipt,
                check,
                page,
            } => {
                receipt_access(&mut tx, actor.id, admin, &route, receipt).await?;
                let row=sqlx::query("SELECT samples,matched_rules,total_hits,truncated FROM keyword_check_results WHERE id=$1 AND receipt_id=$2").bind(check).bind(receipt).fetch_optional(&mut *tx).await?.ok_or(Fault("receipt_not_found"))?;
                let rules: Value = row.get("matched_rules");
                let rules = rules.as_array().ok_or(Fault("temporarily_unavailable"))?;
                let start = ((page - 1) * 30) as usize;
                json!({"items":rules.iter().skip(start).take(30).collect::<Vec<_>>(),"has_more":rules.len()>start+30,"samples":row.get::<Value,_>("samples"),"total_hits":row.get::<i64,_>("total_hits"),"truncated":row.get::<bool,_>("truncated")})
            }
        };
        crate::audit::record(
            &mut tx,
            actor,
            Uuid::nil(),
            "source.keyword.access",
            actor.id,
            audit_context,
        )
        .await
        .map_err(|_| Fault("temporarily_unavailable"))?;
        tx.commit().await?;
        Ok(value)
    }
}
