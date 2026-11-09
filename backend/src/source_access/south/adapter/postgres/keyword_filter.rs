use super::*;
use crate::source_access::{
    domain::{keyword_filter as k, routing::RouteSnapshot},
    south::port::KeywordStore,
};
type Tx<'a> = Transaction<'a, Postgres>;
#[derive(Clone)]
pub struct KeywordRepository(pub PgPool);
async fn guard(tx: &mut Tx<'_>) -> Result<()> {
    crate::distribution::south::adapter::postgres::routing::source_guard(tx)
        .await
        .map_err(|_| Fault("temporarily_unavailable"))
}
async fn source_lock(tx: &mut Tx<'_>, receipt: Uuid) -> Result<()> {
    sqlx::query("SELECT source_id FROM source_callers WHERE source_id=(SELECT source_id FROM source_receipts WHERE receipt_id=$1) FOR UPDATE").bind(receipt).fetch_one(&mut **tx).await?;
    Ok(())
}
/// Every applicable scope must exist. Missing storage is never an empty rule set.
pub(crate) async fn context(
    tx: &mut Tx<'_>,
    receipt: Uuid,
    workspace: Option<(&str, Uuid)>,
) -> Result<(k::Context, Vec<k::Rule>)> {
    let row=sqlx::query("SELECT r.source_id,r.route_id,CASE WHEN d.receipt_id IS NOT NULL THEN (d.snapshot->>'unit_id')::uuid ELSE s.unit_id END AS unit_id FROM source_receipts r LEFT JOIN source_routes s USING(route_id) LEFT JOIN source_routing_decisions d USING(receipt_id) WHERE r.receipt_id=$1").bind(receipt).fetch_one(&mut **tx).await?;
    let source: Uuid = row.get("source_id");
    let route: String = row.get("route_id");
    let unit: Option<Uuid> = row.get("unit_id");
    context_for_scope(tx, source, route, unit, workspace).await
}
pub(crate) async fn context_for_scope(
    tx: &mut Tx<'_>,
    source: Uuid,
    route: String,
    unit: Option<Uuid>,
    workspace: Option<(&str, Uuid)>,
) -> Result<(k::Context, Vec<k::Rule>)> {
    let mut ids = vec![
        "global".to_owned(),
        format!("source:{source}"),
        format!("route:{route}"),
    ];
    if let Some(unit) = unit {
        ids.push(format!("unit:{unit}"));
    }
    if let Some((channel, id)) = workspace {
        ids.push(format!("{channel}:{id}"));
    }
    ids.sort();
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
            channel: workspace.map(|w| w.0.into()),
            workspace_id: workspace.map(|w| w.1),
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
    workspace: Option<(&str, Uuid)>,
    fields: &[(&str, &str)],
) -> Result<(Uuid, bool, String, k::Context)> {
    let payload = crate::source_access::domain::projection::fingerprint(event);
    let mut tx = pool.begin().await?;
    let (ctx, rules) = context(&mut tx, receipt, workspace).await?;
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
    let (latest, _) = context(&mut tx, receipt, workspace).await?;
    ensure(latest.fingerprint() == fingerprint, "content_check_pending")?;
    let current:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM source_content_versions WHERE receipt_id=$1 AND NOT withdrawn AND fingerprint=$2)").bind(receipt).bind(&payload).fetch_one(&mut *tx).await?;
    ensure(current, "content_version_superseded")?;
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO keyword_check_results(id,receipt_id,stage,payload_fingerprint,context,effective_fingerprint,input_digest,normalized_digest,algorithm,unicode_version,outcome,total_hits,matched_rules,samples,truncated) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15) ON CONFLICT DO NOTHING").bind(id).bind(receipt).bind(stage).bind(&payload).bind(json!(ctx)).bind(&fingerprint).bind(&result.input_digest).bind(&result.normalized_digest).bind(k::ALGORITHM).bind(k::UNICODE_VERSION).bind(if result.passed(){"passed"}else{"blocked"}).bind(result.total_hits).bind(json!(result.matched_rules)).bind(json!(result.samples)).bind(result.truncated).execute(&mut *tx).await?;
    let id:Uuid=sqlx::query_scalar("SELECT id FROM keyword_check_results WHERE receipt_id=$1 AND stage=$2 AND payload_fingerprint=$3 AND effective_fingerprint=$4 AND input_digest=$5 AND algorithm=$6").bind(receipt).bind(stage).bind(payload).bind(&fingerprint).bind(input).bind(k::ALGORITHM).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok((id, result.passed(), fingerprint, ctx))
}
async fn block_targets(tx: &mut Tx<'_>, receipt: Uuid, check: Uuid) -> Result<()> {
    sqlx::query("INSERT INTO keyword_target_blocks SELECT $1,i.target_id,$2 FROM source_execution_tasks l JOIN mobile_v9_task_items i ON i.task_id=l.task_id WHERE l.receipt_id=$1 AND NOT EXISTS(SELECT 1 FROM mobile_v9_attempts a JOIN mobile_execution_grants g ON g.attempt_id=a.id WHERE a.item_id=i.id) UNION SELECT $1,t.group_id,$2 FROM source_execution_tasks l JOIN broadcast_targets t ON t.task_id=l.task_id JOIN broadcast_batches b ON b.id=t.batch_id WHERE l.receipt_id=$1 AND b.state='queued' ON CONFLICT(receipt_id,target_id) DO UPDATE SET check_id=$2").bind(receipt).bind(check).execute(&mut **tx).await?;
    sqlx::query("INSERT INTO source_summary_queue VALUES($1) ON CONFLICT DO NOTHING")
        .bind(receipt)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
pub(super) async fn common(pool: &PgPool, receipt: Uuid, event: &Event) -> Result<bool> {
    let (check, passed, fingerprint, _) = check(
        pool,
        receipt,
        event,
        "original_common",
        None,
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
        context(&mut tx, receipt, None).await?.0.fingerprint() == fingerprint,
        "content_check_pending",
    )?;
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
        block_targets(&mut tx, receipt, check).await?;
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
pub(crate) async fn frozen_text(tx: &mut Tx<'_>, receipt: Uuid) -> Result<Option<String>> {
    let composed:Option<String>=sqlx::query_scalar("SELECT c.content FROM source_execution_tasks l JOIN source_message_compositions c ON c.id=l.composition_id WHERE l.receipt_id=$1").bind(receipt).fetch_optional(&mut **tx).await?;
    if composed.is_some() {
        return Ok(composed);
    }
    Ok(sqlx::query_scalar("SELECT content FROM (SELECT s.content FROM source_execution_tasks l JOIN mobile_v9_task_items i ON i.task_id=l.task_id JOIN mobile_v9_plans p ON p.item_id=i.id JOIN distribution_message_snapshots s ON s.id=p.snapshot_id WHERE l.receipt_id=$1 UNION ALL SELECT t.content FROM source_execution_tasks l JOIN broadcast_tasks t ON t.id=l.task_id WHERE l.receipt_id=$1) q LIMIT 1").bind(receipt).fetch_optional(&mut **tx).await?)
}
pub(crate) async fn admit(
    pool: &PgPool,
    receipt: Uuid,
    event: &Event,
    snapshot: &RouteSnapshot,
    message: &str,
    composer: &str,
) -> Result<bool> {
    let config = snapshot
        .execution
        .as_ref()
        .ok_or(Fault("route_configuration_required"))?;
    let workspace = Some((config.channel.as_str(), config.workspace_id));
    let (common, common_passed, common_fp, _) = check(
        pool,
        receipt,
        event,
        "original_common",
        None,
        &[
            ("title", event.title.as_deref().unwrap_or("")),
            ("content", event.content.as_deref().unwrap_or("")),
        ],
    )
    .await?;
    if !common_passed {
        return Err(Fault("content_check_pending"));
    }
    let (original, original_passed, fingerprint, ctx) = check(
        pool,
        receipt,
        event,
        "original_workspace",
        workspace,
        &[
            ("title", event.title.as_deref().unwrap_or("")),
            ("content", event.content.as_deref().unwrap_or("")),
        ],
    )
    .await?;
    let final_result = if original_passed {
        Some(
            check(
                pool,
                receipt,
                event,
                "final",
                workspace,
                &[("final", message)],
            )
            .await?,
        )
    } else {
        None
    };
    let mut tx = pool.begin().await?;
    guard(&mut tx).await?;
    source_lock(&mut tx, receipt).await?;
    ensure(
        context(&mut tx, receipt, None).await?.0.fingerprint() == common_fp
            && context(&mut tx, receipt, workspace).await?.0.fingerprint() == fingerprint,
        "content_check_pending",
    )?;
    let current:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM source_content_versions WHERE receipt_id=$1 AND NOT withdrawn)").bind(receipt).fetch_one(&mut *tx).await?;
    ensure(current, "content_version_superseded")?;
    let passed = final_result.as_ref().is_some_and(|r| r.1);
    if !passed {
        let failed = final_result.map(|r| r.0).unwrap_or(original);
        block_targets(&mut tx, receipt, failed).await?;
        sqlx::query("UPDATE source_receipts SET execution_check_state=$2 WHERE receipt_id=$1")
            .bind(receipt)
            .bind(if original_passed {
                "final_blocked"
            } else {
                "original_workspace_blocked"
            })
            .execute(&mut *tx)
            .await?;
        super::projection::transition(
            &mut tx,
            receipt,
            "applied",
            "execution_blocked",
            Some(if original_passed {
                "final_keyword_blocked"
            } else {
                "workspace_keyword_blocked"
            }),
            "execution_blocked",
        )
        .await?;
    } else {
        let (final_id, _, final_fp, _) = final_result.ok_or(Fault("content_check_pending"))?;
        ensure(final_fp == fingerprint, "content_check_pending")?;
        sqlx::query("INSERT INTO keyword_admissions(receipt_id,common_check,workspace_check,final_check,effective_fingerprint,content_digest,composer,context) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(receipt_id) DO UPDATE SET common_check=$2,workspace_check=$3,final_check=$4,effective_fingerprint=$5,content_digest=$6,composer=$7,context=$8,updated_at=clock_timestamp()").bind(receipt).bind(common).bind(original).bind(final_id).bind(fingerprint).bind(digest(message.as_bytes())).bind(composer).bind(json!(ctx)).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM keyword_target_blocks WHERE receipt_id=$1")
            .bind(receipt)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO source_summary_queue VALUES($1) ON CONFLICT DO NOTHING")
            .bind(receipt)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "UPDATE source_receipts SET execution_check_state='passed' WHERE receipt_id=$1",
        )
        .bind(receipt)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(passed)
}
/// Called inside the existing signing/submission transaction. No matching or network work here.
pub(super) async fn guard_admission(
    tx: &mut Tx<'_>,
    receipt: Uuid,
    snapshot: &RouteSnapshot,
) -> Result<()> {
    let config = snapshot
        .execution
        .as_ref()
        .ok_or(Fault("route_configuration_required"))?;
    let common = context(tx, receipt, None).await?.0.fingerprint();
    let full = context(
        tx,
        receipt,
        Some((config.channel.as_str(), config.workspace_id)),
    )
    .await?
    .0
    .fingerprint();
    let row=sqlx::query("SELECT a.content_digest,a.effective_fingerprint,c.effective_fingerprint AS common_fp FROM keyword_admissions a JOIN keyword_check_results c ON c.id=a.common_check JOIN keyword_check_results w ON w.id=a.workspace_check JOIN keyword_check_results f ON f.id=a.final_check JOIN source_content_versions v ON v.receipt_id=a.receipt_id WHERE a.receipt_id=$1 AND NOT v.withdrawn AND v.check_state='passed' AND c.outcome='passed' AND w.outcome='passed' AND f.outcome='passed' AND c.payload_fingerprint=v.fingerprint AND w.payload_fingerprint=v.fingerprint AND f.payload_fingerprint=v.fingerprint AND w.effective_fingerprint=a.effective_fingerprint AND f.effective_fingerprint=a.effective_fingerprint").bind(receipt).fetch_optional(&mut **tx).await?;
    let valid = if let Some(row) = row {
        row.get::<String, _>("effective_fingerprint") == full
            && row.get::<String, _>("common_fp") == common
            && frozen_text(tx, receipt).await?.is_none_or(|text| {
                digest(text.as_bytes()) == row.get::<String, _>("content_digest")
            })
    } else {
        false
    };
    ensure(valid, "content_check_pending")
}
impl KeywordRepository {
    pub(super) async fn recover(&self) -> Result<()> {
        let mut tx = self.0.begin().await?;
        guard(&mut tx).await?;
        let batch=sqlx::query("SELECT b.*,h.kind,h.object_id,h.channel FROM keyword_recheck_batches b LEFT JOIN keyword_scope_heads h ON h.id=b.scope_id WHERE NOT b.complete ORDER BY b.created_at,b.id LIMIT 1 FOR UPDATE OF b SKIP LOCKED").fetch_optional(&mut *tx).await?;
        if let Some(b) = batch {
            let cursor: Option<Uuid> = b.get("after_receipt");
            let scope: Option<String> = b.get("kind");
            let object: Option<String> = b.get("object_id");
            let channel: Option<String> = b.get("channel");
            let ids:Vec<Uuid>=sqlx::query_scalar("SELECT v.receipt_id FROM source_content_versions v JOIN source_receipts r USING(receipt_id) LEFT JOIN source_routes s ON s.route_id=v.route_id LEFT JOIN source_routing_decisions d ON d.receipt_id=v.receipt_id WHERE ($1::uuid IS NULL OR v.receipt_id>$1) AND ($2::text IS NULL OR $2='global' OR ($2='source' AND v.source_id::text=$3) OR ($2='route' AND v.route_id=$3) OR ($2='unit' AND CASE WHEN d.receipt_id IS NOT NULL THEN d.snapshot->>'unit_id' ELSE s.unit_id::text END=$3) OR ($2='workspace' AND d.snapshot #>> '{execution,workspace_id}'=$3 AND d.snapshot #>> '{execution,channel}'=$4)) ORDER BY v.receipt_id LIMIT 50").bind(cursor).bind(scope).bind(object).bind(channel).fetch_all(&mut *tx).await?;
            sqlx::query("SELECT source_id FROM source_callers WHERE source_id IN (SELECT source_id FROM source_content_versions WHERE receipt_id=ANY($1)) ORDER BY source_id FOR UPDATE").bind(&ids).fetch_all(&mut *tx).await?;
            sqlx::query("UPDATE source_inbox i SET state='waiting',next_attempt_at=now() FROM source_content_versions v WHERE v.receipt_id=i.receipt_id AND v.receipt_id=ANY($1) AND NOT v.withdrawn AND NOT v.no_resend AND i.state NOT IN ('stopped','superseded')").bind(&ids).execute(&mut *tx).await?;
            sqlx::query(
                "UPDATE keyword_recheck_batches SET after_receipt=$2,complete=$3 WHERE id=$1",
            )
            .bind(b.get::<Uuid, _>("id"))
            .bind(ids.last().copied().or(cursor))
            .bind(ids.len() < 50)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}
async fn scope_access(tx: &mut Tx<'_>, actor: Uuid, admin: bool, scope: &str) -> Result<()> {
    let row = sqlx::query("SELECT * FROM keyword_scope_heads WHERE id=$1")
        .bind(scope)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(Fault("not_found"))?;
    if !admin {
        ensure(row.get::<String, _>("kind") == "workspace", "forbidden")?;
        let id: Uuid = row
            .get::<String, _>("object_id")
            .parse()
            .map_err(|_| Fault("forbidden"))?;
        crate::distribution::south::adapter::postgres::routing::authorize(
            tx,
            actor,
            &row.get::<String, _>("channel"),
            id,
        )
        .await
        .map_err(|_| Fault("forbidden"))?;
    }
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
                let rows=sqlx::query("SELECT h.*,COALESCE((SELECT bool_and(b.complete) FROM keyword_recheck_batches b WHERE b.scope_id=h.id),true) AS recheck_complete FROM keyword_scope_heads h ORDER BY h.kind,h.label,h.id").fetch_all(&mut *tx).await?;
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
                sqlx::query(
                    "INSERT INTO keyword_recheck_batches(id,scope_id,version_id) VALUES($1,$2,$3)",
                )
                .bind(batch)
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
