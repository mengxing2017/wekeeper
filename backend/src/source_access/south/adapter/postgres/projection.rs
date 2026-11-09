use super::routing::RoutingRepository;
use super::*;
use crate::source_access::domain::projection::{self, Decision};
type Tx<'a> = Transaction<'a, Postgres>;

pub(super) async fn transition(
    tx: &mut Tx<'_>,
    receipt: Uuid,
    status: &str,
    stage: &str,
    reason: Option<&str>,
    state: &str,
) -> Result<()> {
    sqlx::query("UPDATE source_receipts SET status=$2,processing_stage=$3,reason_code=$4,updated_at=clock_timestamp() WHERE receipt_id=$1").bind(receipt).bind(status).bind(stage).bind(reason).execute(&mut **tx).await?;
    sqlx::query("UPDATE source_inbox SET state=$2,last_error=$3 WHERE receipt_id=$1")
        .bind(receipt)
        .bind(state)
        .bind(reason)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
async fn evidence(
    tx: &mut Tx<'_>,
    source: Uuid,
    receipt: Uuid,
    event: &Event,
    fp: &str,
) -> Result<()> {
    sqlx::query("INSERT INTO source_content_history(source_id,route_id,external_content_id,sequence,receipt_id,fingerprint) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING").bind(source).bind(&event.route_id).bind(&event.external_content_id).bind(event.sequence).bind(receipt).bind(fp).execute(&mut **tx).await?;
    Ok(())
}
impl RoutingRepository {
    pub(super) async fn prepare(&self, receipt: Uuid, source: Uuid, event: &Event) -> Result<bool> {
        let fp = projection::fingerprint(event);
        let mut tx = self.0.begin().await?;
        crate::distribution::south::adapter::postgres::routing::source_guard(&mut tx)
            .await
            .map_err(|_| Fault("temporarily_unavailable"))?;
        sqlx::query("SELECT source_id FROM source_callers WHERE source_id=$1 FOR UPDATE")
            .bind(source)
            .fetch_one(&mut *tx)
            .await?;
        let row =
            sqlx::query("SELECT state,projected FROM source_inbox WHERE receipt_id=$1 FOR UPDATE")
                .bind(receipt)
                .fetch_one(&mut *tx)
                .await?;
        if !matches!(
            row.get::<String, _>("state").as_str(),
            "received" | "waiting"
        ) {
            return Ok(false);
        }
        if !row.get::<bool, _>("projected") {
            let prior=sqlx::query("SELECT * FROM source_content_versions WHERE source_id=$1 AND route_id=$2 AND external_content_id=$3 FOR UPDATE").bind(source).bind(&event.route_id).bind(&event.external_content_id).fetch_optional(&mut *tx).await?;
            let mut previous = None;
            let mut no_resend = false;
            if let Some(old) = prior.as_ref() {
                let old_receipt: Uuid = old.get("receipt_id");
                let old_fp = if let Some(value) = old.get::<Option<String>, _>("fingerprint") {
                    value
                } else {
                    let bytes: Vec<u8> = sqlx::query_scalar(
                        "SELECT request_body FROM source_inbox WHERE receipt_id=$1",
                    )
                    .bind(old_receipt)
                    .fetch_one(&mut *tx)
                    .await?;
                    let old_event: Document<Event> = serde_json::from_slice(&bytes)?;
                    let value = projection::fingerprint(&old_event.data.attributes);
                    evidence(
                        &mut tx,
                        source,
                        old_receipt,
                        &old_event.data.attributes,
                        &value,
                    )
                    .await?;
                    value
                };
                previous = Some((old.get::<i64, _>("sequence"), old_fp, old_receipt));
                no_resend = old.get("no_resend");
            }
            let decision = projection::decide(
                event.sequence,
                &fp,
                previous.as_ref().map(|(s, f, _)| (*s, f.as_str())),
            );
            let observed = previous
                .as_ref()
                .map(|p| p.0)
                .unwrap_or(event.sequence)
                .max(event.sequence);
            sqlx::query("UPDATE source_receipts SET observed_sequence=$2 WHERE receipt_id=$1")
                .bind(receipt)
                .bind(observed)
                .execute(&mut *tx)
                .await?;
            match decision {
                Decision::Stale => {
                    transition(
                        &mut tx,
                        receipt,
                        "ignored_stale",
                        "ignored_stale",
                        Some("content_version_superseded"),
                        "ignored_stale",
                    )
                    .await?;
                    tx.commit().await?;
                    return Ok(false);
                }
                Decision::Conflict => {
                    let previous = previous.as_ref().ok_or(Fault("temporarily_unavailable"))?;
                    audit(&mut tx,source,None,None,"source.version.conflict","source-projection",json!({"receipt_id":receipt,"canonical_receipt_id":previous.2,"sequence":event.sequence,"first_digest":previous.1,"conflicting_digest":fp})).await?;
                    transition(
                        &mut tx,
                        receipt,
                        "rejected",
                        "rejected",
                        Some("content_version_conflict"),
                        "rejected",
                    )
                    .await?;
                    tx.commit().await?;
                    return Ok(false);
                }
                Decision::Duplicate => {
                    let canonical = previous.as_ref().ok_or(Fault("temporarily_unavailable"))?.2;
                    sqlx::query("UPDATE source_receipts r SET canonical_receipt_id=$2,status=c.status,reason_code=c.reason_code,processing_stage='projection_duplicate' FROM source_receipts c WHERE r.receipt_id=$1 AND c.receipt_id=$2").bind(receipt).bind(canonical).execute(&mut *tx).await?;
                    sqlx::query("INSERT INTO source_receipt_relations VALUES($1,$2,'duplicate') ON CONFLICT DO NOTHING").bind(receipt).bind(canonical).execute(&mut *tx).await?;
                    sqlx::query("UPDATE source_inbox SET state='duplicate',projected=true WHERE receipt_id=$1").bind(receipt).execute(&mut *tx).await?;
                    tx.commit().await?;
                    return Ok(false);
                }
                Decision::Apply => {}
            }
            if let Some((old_sequence, _, old_receipt)) = previous
                && old_receipt != receipt
            {
                no_resend |= super::routing_worker::halt(
                    &mut tx,
                    old_receipt,
                    if event.event_type == "content.withdrawn" {
                        "source_content_withdrawn"
                    } else {
                        "content_version_superseded"
                    },
                )
                .await?;
                sqlx::query("DELETE FROM source_recheck_requests WHERE receipt_id=$1")
                    .bind(old_receipt)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query("UPDATE source_inbox SET state='superseded' WHERE receipt_id=$1 AND state IN ('received','waiting')").bind(old_receipt).execute(&mut *tx).await?;
                sqlx::query("UPDATE source_routing_waits SET stopped=true WHERE receipt_id=$1")
                    .bind(old_receipt)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query(
                    "INSERT INTO source_receipt_relations VALUES($1,$2,$3) ON CONFLICT DO NOTHING",
                )
                .bind(receipt)
                .bind(old_receipt)
                .bind(if event.event_type == "content.withdrawn" {
                    "withdrawal"
                } else {
                    "update"
                })
                .execute(&mut *tx)
                .await?;
                audit(&mut tx,source,None,None,"source.version.advance","source-projection",json!({"receipt_id":receipt,"previous_sequence":old_sequence,"sequence":event.sequence,"gap":event.sequence-old_sequence-1})).await?;
            }
            evidence(&mut tx, source, receipt, event, &fp).await?;
            sqlx::query("INSERT INTO source_content_versions(source_id,route_id,external_content_id,sequence,version,receipt_id,withdrawn,fingerprint,check_state,no_resend) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT(source_id,route_id,external_content_id) DO UPDATE SET sequence=$4,version=$5,receipt_id=$6,withdrawn=$7,fingerprint=$8,check_state=$9,check_revision=NULL,no_resend=$10,updated_at=clock_timestamp()").bind(source).bind(&event.route_id).bind(&event.external_content_id).bind(event.sequence).bind(&event.version).bind(receipt).bind(event.event_type=="content.withdrawn").bind(&fp).bind(if event.event_type=="content.withdrawn"{"withdrawn"}else{"pending"}).bind(no_resend).execute(&mut *tx).await?;
            sqlx::query("UPDATE source_inbox SET projected=true WHERE receipt_id=$1")
                .bind(receipt)
                .execute(&mut *tx)
                .await?;
            if event.event_type == "content.withdrawn" {
                sqlx::query("INSERT INTO source_receipt_relations SELECT $1,h.receipt_id,'withdrawal' FROM source_content_history h JOIN source_execution_tasks t ON t.receipt_id=h.receipt_id WHERE h.source_id=$2 AND h.route_id=$3 AND h.external_content_id=$4 AND h.receipt_id<>$1 ORDER BY h.sequence DESC LIMIT 1 ON CONFLICT DO NOTHING").bind(receipt).bind(source).bind(&event.route_id).bind(&event.external_content_id).execute(&mut *tx).await?;
            }
            if event.event_type == "content.withdrawn" {
                transition(&mut tx, receipt, "applied", "withdrawn", None, "withdrawn").await?;
                tx.commit().await?;
                return Ok(false);
            }
            transition(
                &mut tx,
                receipt,
                "received",
                "filter_pending",
                None,
                "waiting",
            )
            .await?;
        }
        tx.commit().await?;
        self.evaluate(receipt, source, event, &fp).await
    }
    async fn evaluate(&self, receipt: Uuid, source: Uuid, event: &Event, fp: &str) -> Result<bool> {
        let mut tx = self.0.begin().await?;
        crate::distribution::south::adapter::postgres::routing::source_guard(&mut tx)
            .await
            .map_err(|_| Fault("temporarily_unavailable"))?;
        sqlx::query("SELECT source_id FROM source_callers WHERE source_id=$1 FOR UPDATE")
            .bind(source)
            .fetch_one(&mut *tx)
            .await?;
        let current=sqlx::query("SELECT receipt_id,withdrawn,no_resend,check_state,check_revision FROM source_content_versions WHERE source_id=$1 AND route_id=$2 AND external_content_id=$3 FOR UPDATE").bind(source).bind(&event.route_id).bind(&event.external_content_id).fetch_one(&mut *tx).await?;
        if current.get::<Uuid, _>("receipt_id") != receipt || current.get::<bool, _>("withdrawn") {
            transition(
                &mut tx,
                receipt,
                "ignored_stale",
                "ignored_stale",
                Some("content_version_superseded"),
                "ignored_stale",
            )
            .await?;
            tx.commit().await?;
            return Ok(false);
        }
        let policy =
            sqlx::query("SELECT revision,terms FROM source_check_policy WHERE singleton FOR SHARE")
                .fetch_one(&mut *tx)
                .await?;
        let revision: i64 = policy.get("revision");
        let terms: Vec<String> = policy.get("terms");
        if current.get::<String, _>("check_state") == "passed"
            && current.get::<Option<String>, _>("check_revision") == Some(revision.to_string())
        {
            tx.commit().await?;
            return Ok(!current.get::<bool, _>("no_resend"));
        }

        let blocked = projection::blocked(event, &terms);
        let outcome = if blocked { "blocked" } else { "passed" };
        sqlx::query("INSERT INTO source_content_checks(receipt_id,rule_revision,fingerprint,outcome) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING").bind(receipt).bind(revision).bind(fp).bind(outcome).execute(&mut *tx).await?;
        sqlx::query("UPDATE source_content_versions SET check_state=$2,check_revision=$3 WHERE receipt_id=$1").bind(receipt).bind(outcome).bind(revision.to_string()).execute(&mut *tx).await?;
        let no_resend: bool = current.get("no_resend");
        transition(
            &mut tx,
            receipt,
            if blocked { "rejected" } else { "applied" },
            if blocked {
                "rejected"
            } else if no_resend {
                "no_resend"
            } else {
                "projection_applied"
            },
            if blocked {
                Some("keyword_blocked")
            } else {
                None
            },
            if blocked {
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
        Ok(!blocked && !no_resend)
    }
    pub(super) async fn recover_checks(&self) -> Result<()> {
        // Bounded backfill preserves existing tasks, envelopes and original receipt identities.
        let rows=sqlx::query("SELECT v.source_id,v.receipt_id,i.request_body FROM source_content_versions v JOIN source_inbox i USING(receipt_id) WHERE v.fingerprint IS NULL ORDER BY v.updated_at LIMIT 50").fetch_all(&self.0).await?;
        for row in rows {
            let receipt: Uuid = row.get("receipt_id");
            let body: Vec<u8> = row.get("request_body");
            let event: Document<Event> = serde_json::from_slice(&body)?;
            let mut tx = self.0.begin().await?;
            sqlx::query("SELECT source_id FROM source_callers WHERE source_id=$1 FOR UPDATE")
                .bind(row.get::<Uuid, _>("source_id"))
                .fetch_one(&mut *tx)
                .await?;
            evidence(
                &mut tx,
                row.get("source_id"),
                receipt,
                &event.data.attributes,
                &projection::fingerprint(&event.data.attributes),
            )
            .await?;
            sqlx::query("UPDATE source_content_versions SET fingerprint=$2,check_state=CASE WHEN withdrawn THEN 'withdrawn' ELSE 'pending' END,no_resend=no_resend OR EXISTS(SELECT 1 FROM source_receipts r WHERE r.receipt_id=$1 AND r.reason_code='content_updated_no_resend') WHERE receipt_id=$1 AND fingerprint IS NULL").bind(receipt).bind(projection::fingerprint(&event.data.attributes)).execute(&mut *tx).await?;
            sqlx::query("UPDATE source_inbox SET projected=true,state='waiting',next_attempt_at=now() WHERE receipt_id=$1 AND NOT EXISTS(SELECT 1 FROM source_content_versions WHERE receipt_id=$1 AND withdrawn)").bind(receipt).execute(&mut *tx).await?;
            tx.commit().await?;
        }
        let mut tx = self.0.begin().await?;
        crate::distribution::south::adapter::postgres::routing::source_guard(&mut tx)
            .await
            .map_err(|_| Fault("temporarily_unavailable"))?;
        let policy: Option<i64> = sqlx::query_scalar(
            "SELECT revision FROM source_check_policy WHERE singleton FOR SHARE",
        )
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(revision) = policy {
            let scan = sqlx::query("SELECT * FROM source_check_scan WHERE singleton FOR UPDATE")
                .fetch_one(&mut *tx)
                .await?;
            let changed = scan.get::<i64, _>("rule_revision") != revision;
            if changed || !scan.get::<bool, _>("complete") {
                let cursor: Option<Uuid> = if changed {
                    None
                } else {
                    scan.get("after_receipt")
                };
                let page:Vec<Uuid>=sqlx::query_scalar("SELECT receipt_id FROM source_content_versions WHERE $1::uuid IS NULL OR receipt_id>$1 ORDER BY receipt_id LIMIT 50").bind(cursor).fetch_all(&mut *tx).await?;
                sqlx::query("SELECT source_id FROM source_callers WHERE source_id IN (SELECT source_id FROM source_content_versions WHERE receipt_id=ANY($1)) ORDER BY source_id FOR UPDATE").bind(&page).fetch_all(&mut *tx).await?;
                let ids:Vec<Uuid>=sqlx::query_scalar("UPDATE source_content_versions SET check_state='pending' WHERE receipt_id=ANY($1) AND NOT withdrawn AND fingerprint IS NOT NULL AND check_revision IS DISTINCT FROM $2 RETURNING receipt_id").bind(&page).bind(revision.to_string()).fetch_all(&mut *tx).await?;
                sqlx::query("UPDATE source_receipts SET status='received',processing_stage='filter_pending',reason_code=NULL WHERE receipt_id=ANY($1) AND status<>'received'").bind(&ids).execute(&mut *tx).await?;
                sqlx::query("UPDATE source_inbox SET state='waiting',next_attempt_at=now() WHERE receipt_id=ANY($1)").bind(&ids).execute(&mut *tx).await?;
                sqlx::query("UPDATE source_check_scan SET rule_revision=$1,after_receipt=$2,complete=$3 WHERE singleton").bind(revision).bind(page.last().copied().or(cursor)).bind(page.len()<50).execute(&mut *tx).await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct ReceiptsRepository(pub PgPool);
impl crate::source_access::south::port::ProjectionStore for ReceiptsRepository {
    async fn execute(
        &self,
        actor: &crate::business::Actor,
        command: projection::Command,
    ) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        if matches!(&command, projection::Command::Recheck { .. }) {
            crate::distribution::south::adapter::postgres::routing::source_guard(&mut tx)
                .await
                .map_err(|_| Fault("temporarily_unavailable"))?;
        }
        let admin = super::routing::administrator(&mut tx, actor.id).await?;
        let route = match &command {
            projection::Command::List { route, .. }
            | projection::Command::Detail { route, .. }
            | projection::Command::Recheck { route, .. } => route,
        };
        super::routing::access(&mut tx, actor.id, route, admin).await?;
        let can_recheck=admin||sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM source_route_managers WHERE route_id=$1 AND account_id=$2 AND can_recheck)").bind(route).bind(actor.id).fetch_one(&mut *tx).await?;
        let fields = "jsonb_build_object('id',r.receipt_id,'event_id',r.event_id,'status',COALESCE(c.status,r.status),'reason_code',CASE WHEN c.receipt_id IS NOT NULL THEN c.reason_code ELSE r.reason_code END,'processing_stage',r.processing_stage,'received_at',r.received_at,'updated_at',greatest(r.updated_at,c.updated_at,w.updated_at),'revision',r.processing_revision,'observed_sequence',r.observed_sequence,'sending',COALESCE(c.sending,r.sending,w.sending),'summary_version',COALESCE(c.summary_version,r.summary_version,w.summary_version),'summary_revision',CASE WHEN c.summary_version IS NOT NULL THEN c.summary_revision WHEN w.summary_version IS NOT NULL THEN w.summary_revision ELSE r.summary_revision END,'summary_total',COALESCE(c.summary_total,r.summary_total,w.summary_total),'external_content_id',convert_from(i.request_body,'UTF8')::jsonb #>> '{data,attributes,external_content_id}','sequence',(convert_from(i.request_body,'UTF8')::jsonb #>> '{data,attributes,sequence}')::bigint)";
        let value = match &command {
            projection::Command::List { page, .. } => {
                let query = format!(
                    "SELECT {fields} FROM source_receipts r JOIN source_inbox i USING(receipt_id) LEFT JOIN source_receipts c ON c.receipt_id=r.canonical_receipt_id LEFT JOIN LATERAL (SELECT linked.sending,linked.updated_at,linked.summary_version,linked.summary_revision,linked.summary_total FROM source_receipt_relations l JOIN source_receipts linked ON linked.receipt_id=l.related_receipt_id JOIN source_execution_tasks t ON t.receipt_id=linked.receipt_id WHERE l.receipt_id=COALESCE(r.canonical_receipt_id,r.receipt_id) AND l.kind='withdrawal' ORDER BY linked.received_at DESC LIMIT 1) w ON true WHERE r.route_id=$1 ORDER BY r.received_at DESC,r.receipt_id DESC LIMIT 31 OFFSET $2"
                );
                let rows: Vec<Value> = sqlx::query_scalar(&query)
                    .bind(route)
                    .bind((page - 1) * 30)
                    .fetch_all(&mut *tx)
                    .await?;
                json!({"items":rows.iter().take(30).collect::<Vec<_>>(),"has_more":rows.len()>30,"can_recheck":can_recheck})
            }
            projection::Command::Detail { receipt, .. } => {
                let query = format!(
                    "SELECT {fields} FROM source_receipts r JOIN source_inbox i USING(receipt_id) LEFT JOIN source_receipts c ON c.receipt_id=r.canonical_receipt_id LEFT JOIN LATERAL (SELECT linked.sending,linked.updated_at,linked.summary_version,linked.summary_revision,linked.summary_total FROM source_receipt_relations l JOIN source_receipts linked ON linked.receipt_id=l.related_receipt_id JOIN source_execution_tasks t ON t.receipt_id=linked.receipt_id WHERE l.receipt_id=COALESCE(r.canonical_receipt_id,r.receipt_id) AND l.kind='withdrawal' ORDER BY linked.received_at DESC LIMIT 1) w ON true WHERE r.route_id=$1 AND r.receipt_id=$2"
                );
                let mut detail: Value = sqlx::query_scalar(&query)
                    .bind(route)
                    .bind(receipt)
                    .fetch_optional(&mut *tx)
                    .await?
                    .ok_or(Fault("receipt_not_found"))?;
                detail["history"]=json!(sqlx::query_scalar::<_,Value>("SELECT to_jsonb(h)-'receipt_id' FROM source_receipt_history h WHERE receipt_id=$1 ORDER BY revision DESC LIMIT 100").bind(receipt).fetch_all(&mut *tx).await?);
                detail["checks"]=json!(sqlx::query_scalar::<_,Value>("SELECT to_jsonb(c)-'receipt_id' FROM source_content_checks c WHERE receipt_id=$1 ORDER BY created_at DESC LIMIT 100").bind(receipt).fetch_all(&mut *tx).await?);
                detail["relations"]=json!(sqlx::query_scalar::<_,Value>("SELECT to_jsonb(l)-'receipt_id' FROM source_receipt_relations l WHERE receipt_id=$1").bind(receipt).fetch_all(&mut *tx).await?);
                let current:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM source_content_versions WHERE receipt_id=$1 AND NOT withdrawn)").bind(receipt).fetch_one(&mut *tx).await?;
                detail["can_recheck"] = json!(can_recheck && current);
                detail
            }
            projection::Command::Recheck { receipt, input, .. } => {
                ensure(can_recheck, "forbidden")?;
                let source: Uuid = sqlx::query_scalar(
                    "SELECT source_id FROM source_receipts WHERE receipt_id=$1 AND route_id=$2",
                )
                .bind(receipt)
                .bind(route)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(Fault("receipt_not_found"))?;
                sqlx::query("SELECT source_id FROM source_callers WHERE source_id=$1 FOR UPDATE")
                    .bind(source)
                    .fetch_one(&mut *tx)
                    .await?;
                let row=sqlx::query("SELECT r.processing_revision,r.reason_code,v.withdrawn FROM source_receipts r JOIN source_content_versions v ON v.receipt_id=r.receipt_id WHERE r.receipt_id=$1 FOR UPDATE OF r,v").bind(receipt).fetch_optional(&mut *tx).await?.ok_or(Fault("content_version_superseded"))?;
                ensure(
                    row.get::<i64, _>("processing_revision") == input.revision,
                    "revision_conflict",
                )?;
                ensure(
                    !row.get::<bool, _>("withdrawn")
                        && row.get::<Option<String>, _>("reason_code").as_deref()
                            == Some("keyword_blocked"),
                    "recheck_not_allowed",
                )?;
                sqlx::query("INSERT INTO source_recheck_requests(receipt_id,actor_id,reason,expected_revision) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING").bind(receipt).bind(actor.id).bind(input.reason.trim()).bind(input.revision).execute(&mut *tx).await?;
                sqlx::query(
                    "UPDATE source_content_versions SET check_state='pending' WHERE receipt_id=$1",
                )
                .bind(receipt)
                .execute(&mut *tx)
                .await?;
                transition(
                    &mut tx,
                    *receipt,
                    "received",
                    "recheck_pending",
                    None,
                    "waiting",
                )
                .await?;
                sqlx::query("UPDATE source_inbox SET next_attempt_at=now() WHERE receipt_id=$1")
                    .bind(receipt)
                    .execute(&mut *tx)
                    .await?;
                audit(
                    &mut tx,
                    source,
                    None,
                    Some(actor.id),
                    "source.receipt.recheck",
                    &actor.request_id,
                    json!({"receipt_id":receipt,"reason":input.reason,"revision":input.revision}),
                )
                .await?;
                json!({"id":receipt,"queued":true})
            }
        };
        if admin {
            crate::audit::record(
                &mut tx,
                actor,
                Uuid::nil(),
                "source.receipts.read",
                actor.id,
                json!({"route_id":route}),
            )
            .await
            .map_err(|_| Fault("temporarily_unavailable"))?;
        }
        tx.commit().await?;
        Ok(value)
    }
}
