//! Existing inbox consumer owns the workflow; this adapter coordinates local atomic writes.
use super::*;
use crate::source_access::{
    domain::{Event, keyword_filter as k, routing::RouteSnapshot},
    south::adapter::postgres::keyword_filter as checks,
};
pub(crate) async fn config(tx: &mut Tx<'_>, route: &str) -> Result<(i64, Config)> {
    let (n, v): (i64, Value) = sqlx::query_as(
        "SELECT revision,config FROM source_advertisement_configs WHERE route_id=$1 FOR SHARE",
    )
    .bind(route)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(Fault("ad_configuration_unavailable"))?;
    Ok((n, serde_json::from_value(v)?))
}
pub(crate) async fn select(
    tx: &mut Tx<'_>,
    snapshot: &RouteSnapshot,
    revision: i64,
    config: Config,
    source: &str,
    base: Option<Uuid>,
) -> Result<Prepared> {
    let channel = snapshot
        .execution
        .as_ref()
        .ok_or(Fault("route_configuration_required"))?
        .channel
        .as_str();
    let mut selection = Selection {
        pool: None,
        config: config.clone(),
        candidates: vec![],
        config_revision: revision,
        ad_id: None,
        content_version: None,
        policy_version: None,
        composer: k::COMPOSER.into(),
        exclusions: vec![],
        measurement: measure(source, channel),
    };
    let mut content = source.to_owned();
    if let Some(id) = config.fixed_ad_id {
        let row = ad_row(tx, id).await?;
        if let Some(reason) = ad_exclusion(&row, snapshot)? {
            selection.exclusions.push(reason.into());
        } else {
            let c = Content {
                name: row.get("name"),
                body: row.get("body"),
                link: row.get("link"),
            };
            content = compose(source, Some(&c), &config.insertion_position);
            if measure(&content, channel).valid {
                selection.ad_id = Some(id);
                selection.content_version = row.get("content_version");
                selection.policy_version = row.get("policy_version");
                selection.composer = "source-ad-text/1".into();
            } else {
                selection.exclusions.push("ad_content_too_long".into());
                content = source.to_owned();
            }
        }
        selection.candidates.push(CandidateEvidence {
            ad_id: id,
            content_version: row.get("content_version"),
            policy_version: row.get("policy_version"),
            exclusion: selection.exclusions.last().cloned(),
        });
    }
    selection.measurement = measure(&content, channel);
    ensure(selection.measurement.valid, "channel_content_invalid")?;
    Ok(Prepared {
        selection,
        content,
        base,
        replacement: base.is_some(),
    })
}
pub(crate) async fn started(tx: &mut Tx<'_>, receipt: Uuid) -> Result<bool> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM source_execution_generations l JOIN mobile_v9_task_items i ON i.task_id=l.task_id JOIN mobile_v9_attempts a ON a.item_id=i.id LEFT JOIN mobile_execution_grants g ON g.attempt_id=a.id WHERE l.receipt_id=$1 AND (g.id IS NOT NULL OR a.state IN ('unknown','observed_sent','manually_confirmed_sent'))) OR EXISTS(SELECT 1 FROM source_execution_generations l JOIN broadcast_batches b ON b.task_id=l.task_id WHERE l.receipt_id=$1 AND (b.state NOT IN ('queued','cancelled') OR b.msgid IS NOT NULL OR b.stop_state='unknown'))").bind(receipt).fetch_one(&mut **tx).await?)
}
async fn can_replace(tx: &mut Tx<'_>, receipt: Uuid) -> Result<bool> {
    let stopped:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM source_execution_tasks l JOIN mobile_v9_tasks t ON t.id=l.task_id WHERE l.receipt_id=$1 AND (t.stopped OR t.confirmed_until<=clock_timestamp())) OR EXISTS(SELECT 1 FROM source_execution_tasks l JOIN broadcast_tasks t ON t.id=l.task_id WHERE l.receipt_id=$1 AND t.stopped)").bind(receipt).fetch_one(&mut **tx).await?;
    Ok(!stopped && !started(tx, receipt).await?)
}
pub(crate) async fn prepare(
    pool: &PgPool,
    receipt: Uuid,
    event: &Event,
    snapshot: &RouteSnapshot,
) -> Result<Option<Prepared>> {
    let source = k::compose(
        event.title.as_deref(),
        event.content.as_deref(),
        &event.detail_url,
    );
    let mut tx = pool.begin().await?;
    let old=sqlx::query("SELECT c.id,c.selection,c.content FROM source_execution_tasks l JOIN source_message_compositions c ON c.id=l.composition_id WHERE l.receipt_id=$1").bind(receipt).fetch_optional(&mut *tx).await?;
    let prepared = if let Some(old) = old {
        let id: Uuid = old.get("id");
        let selection: Selection = serde_json::from_value(old.get("selection"))?;
        let content: String = old.get("content");
        let mut valid = true;
        if let Some(ad) = selection.ad_id {
            valid = ad_exclusion(&ad_row(&mut tx, ad).await?, snapshot)?.is_none();
        }
        let replace = selection.config.mode != "pool"
            && selection.config.auto_recover
            && can_replace(&mut tx, receipt).await?;
        if valid {
            Prepared {
                selection,
                content,
                base: Some(id),
                replacement: false,
            }
        } else if replace {
            select(
                &mut tx,
                snapshot,
                selection.config_revision,
                selection.config.clone(),
                &source,
                Some(id),
            )
            .await?
        } else {
            tx.commit().await?;
            hold(pool, receipt, "ad_frozen_unavailable", Some(&selection)).await?;
            return Ok(None);
        }
    } else {
        let legacy = checks::frozen_text(&mut tx, receipt).await?;
        let (revision, config) = if legacy.is_some() {
            (0, Config::default())
        } else {
            config(&mut tx, &snapshot.route_id).await?
        };
        if config.mode == "pool" {
            tx.commit().await?;
            let prepared =
                pools::select(pool, snapshot, revision, config, &source, Some(receipt)).await?;
            tx = pool.begin().await?;
            prepared
        } else {
            select(
                &mut tx,
                snapshot,
                revision,
                config,
                legacy.as_deref().unwrap_or(&source),
                None,
            )
            .await?
        }
    };
    tx.commit().await?;
    if prepared.selection.unavailable() {
        hold(pool, receipt, "ad_unavailable", Some(&prepared.selection)).await?;
        return Ok(None);
    }
    Ok(Some(prepared))
}
async fn hold(
    pool: &PgPool,
    receipt: Uuid,
    reason: &str,
    selection: Option<&Selection>,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    guard(&mut tx).await?;
    if let Some(selection) = selection {
        decision(&mut tx, receipt, selection, reason, None).await?;
    }
    sqlx::query("UPDATE source_receipts SET processing_stage='execution_blocked',reason_code=$2 WHERE receipt_id=$1").bind(receipt).bind(reason).execute(&mut *tx).await?;
    sqlx::query("UPDATE source_inbox SET state='waiting',next_attempt_at=now()+interval '30 seconds',last_error=$2 WHERE receipt_id=$1 AND state NOT IN ('stopped','superseded')").bind(receipt).bind(reason).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO advertisement_target_blocks SELECT $1,i.target_id,$2 FROM source_execution_tasks l JOIN mobile_v9_task_items i ON i.task_id=l.task_id WHERE l.receipt_id=$1 AND NOT EXISTS(SELECT 1 FROM mobile_v9_attempts a JOIN mobile_execution_grants g ON g.attempt_id=a.id WHERE a.item_id=i.id) UNION SELECT $1,t.group_id,$2 FROM source_execution_tasks l JOIN broadcast_targets t ON t.task_id=l.task_id JOIN broadcast_batches b ON b.id=t.batch_id WHERE l.receipt_id=$1 AND b.state='queued' ON CONFLICT(receipt_id,target_id) DO UPDATE SET reason=$2").bind(receipt).bind(reason).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO source_summary_queue VALUES($1) ON CONFLICT DO NOTHING")
        .bind(receipt)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
pub(crate) async fn validate(
    tx: &mut Tx<'_>,
    receipt: Uuid,
    snapshot: &RouteSnapshot,
    p: &Prepared,
) -> Result<()> {
    ensure(!p.selection.unavailable(), "ad_unavailable")?;
    let current: Option<Uuid> =
        sqlx::query_scalar("SELECT composition_id FROM source_execution_tasks WHERE receipt_id=$1")
            .bind(receipt)
            .fetch_optional(&mut **tx)
            .await?
            .flatten();
    ensure(current == p.base, "ad_selection_stale")?;
    if p.base.is_none() {
        let legacy: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM source_execution_tasks WHERE receipt_id=$1)",
        )
        .bind(receipt)
        .fetch_one(&mut **tx)
        .await?;
        if !legacy {
            ensure(
                config(tx, &snapshot.route_id).await?.0 == p.selection.config_revision,
                "ad_selection_stale",
            )?;
        }
    }
    pools::validate(tx, receipt, p).await?;
    if p.replacement {
        ensure(can_replace(tx, receipt).await?, "ad_selection_stale")?;
    }
    if let Some(id) = p.selection.ad_id {
        let r = ad_row(tx, id).await?;
        ensure(ad_exclusion(&r, snapshot)?.is_none(), "ad_selection_stale")?;
        if p.replacement || p.base.is_none() {
            ensure(
                r.get::<Option<Uuid>, _>("policy_version") == p.selection.policy_version
                    && r.get::<Option<Uuid>, _>("content_version") == p.selection.content_version,
                "ad_selection_stale",
            )?;
        }
    }
    Ok(())
}
pub(crate) async fn current_guard(
    tx: &mut Tx<'_>,
    receipt: Uuid,
    snapshot: &RouteSnapshot,
) -> Result<()> {
    let row=sqlx::query("SELECT c.selection FROM source_execution_tasks l JOIN source_message_compositions c ON c.id=l.composition_id WHERE l.receipt_id=$1").bind(receipt).fetch_optional(&mut **tx).await?;
    if let Some(r) = row {
        let s: Selection = serde_json::from_value(r.get("selection"))?;
        if let Some(id) = s.ad_id {
            ensure(
                ad_exclusion(&ad_row(tx, id).await?, snapshot)?.is_none(),
                "ad_frozen_unavailable",
            )?;
        }
    }
    Ok(())
}
pub(crate) async fn save(
    tx: &mut Tx<'_>,
    receipt: Uuid,
    snapshot: &RouteSnapshot,
    p: &Prepared,
) -> Result<(Uuid, i64, Uuid)> {
    let e = snapshot
        .execution
        .as_ref()
        .ok_or(Fault("route_configuration_required"))?;
    let generation: Option<i64> =
        sqlx::query_scalar("SELECT generation FROM source_execution_tasks WHERE receipt_id=$1")
            .bind(receipt)
            .fetch_optional(&mut **tx)
            .await?;
    let generation = generation.map(|v| v + 1).unwrap_or(0);
    let id = Uuid::now_v7();
    let snap = Uuid::now_v7();
    sqlx::query("INSERT INTO distribution_message_snapshots(id,school_id,channel,content,content_hash) VALUES($1,$2,$3,$4,$5)").bind(snap).bind(e.workspace_id).bind(e.channel.as_str()).bind(&p.content).bind(crate::business::hash(&p.content)).execute(&mut **tx).await?;
    let written=sqlx::query("INSERT INTO source_message_compositions(id,receipt_id,generation,supersedes,selection,content,content_digest,composer,message_snapshot_id,common_check,reason) SELECT $1,$2,$3,$4,$5,$6,$7,$8,$9,c.id,$10 FROM source_content_versions v JOIN LATERAL (SELECT id FROM keyword_check_results WHERE receipt_id=v.receipt_id AND stage='original_common' AND outcome='passed' AND payload_fingerprint=v.fingerprint ORDER BY created_at DESC LIMIT 1) c ON true JOIN source_receipts r ON r.receipt_id=v.receipt_id WHERE v.receipt_id=$2 AND NOT v.withdrawn AND v.check_state='passed' AND NOT r.keyword_terminal_rejected").bind(id).bind(receipt).bind(generation).bind(p.base).bind(json!(p.selection)).bind(&p.content).bind(p.digest()).bind(&p.selection.composer).bind(snap).bind(if p.replacement{"pre_execution_replacement"}else{"initial_selection"}).execute(&mut **tx).await?;
    ensure(written.rows_affected() == 1, "content_check_pending")?;
    decision(tx, receipt, &p.selection, "selected", Some(id)).await?;
    pools::consume(tx, receipt, id, p).await?;
    Ok((id, generation, snap))
}
pub(crate) async fn recover(pool: &PgPool) -> Result<()> {
    let id:Option<Uuid>=sqlx::query_scalar("SELECT id FROM advertisement_recheck_jobs WHERE NOT complete AND due<=clock_timestamp() ORDER BY due,id LIMIT 1").fetch_optional(pool).await?;
    let Some(id) = id else {
        return Ok(());
    };
    if let Err(error) = recover_batch(pool, id).await {
        sqlx::query("UPDATE advertisement_recheck_jobs SET attempts=attempts+1,last_error=$2,due=clock_timestamp()+make_interval(secs=>least(60,5*power(2,least(attempts,4)))::double precision) WHERE id=$1 AND NOT complete").bind(id).bind(error.0).execute(pool).await?;
        return Err(error);
    }
    Ok(())
}
async fn recover_batch(pool: &PgPool, id: Uuid) -> Result<()> {
    let mut tx = pool.begin().await?;
    guard(&mut tx).await?;
    let job=sqlx::query("SELECT * FROM advertisement_recheck_jobs WHERE id=$1 AND NOT complete AND due<=clock_timestamp() FOR UPDATE SKIP LOCKED").bind(id).fetch_optional(&mut *tx).await?;
    if let Some(j) = job {
        let cursor: Option<Uuid> = j.get("after_receipt");
        let ad: Option<Uuid> = j.get("ad_id");
        let route: Option<String> = j.get("route_id");
        let ids:Vec<Uuid>=sqlx::query_scalar("SELECT v.receipt_id FROM source_content_versions v LEFT JOIN source_execution_tasks l ON l.receipt_id=v.receipt_id LEFT JOIN source_message_compositions c ON c.id=l.composition_id LEFT JOIN source_advertisement_configs a ON a.route_id=v.route_id WHERE ($1::uuid IS NULL OR v.receipt_id>$1) AND ($2::text IS NULL OR v.route_id=$2) AND ($3::uuid IS NULL OR c.selection->>'ad_id'=$3::text OR c.selection #>> '{config,fixed_ad_id}'=$3::text OR a.config->>'fixed_ad_id'=$3::text OR EXISTS(SELECT 1 FROM advertisement_pools ap JOIN advertisement_pool_slots ps ON ps.pool_id=ap.id AND ps.revision=ap.current_revision WHERE ap.route_id=v.route_id AND ps.ad_id=$3)) AND NOT v.withdrawn AND NOT v.no_resend ORDER BY v.receipt_id LIMIT 50").bind(cursor).bind(route).bind(ad).fetch_all(&mut *tx).await?;
        sqlx::query("UPDATE source_inbox SET state='waiting',next_attempt_at=now() WHERE receipt_id=ANY($1) AND state NOT IN ('stopped','superseded')").bind(&ids).execute(&mut *tx).await?;
        sqlx::query("UPDATE advertisement_recheck_jobs SET after_receipt=$2,complete=$3,last_error=NULL WHERE id=$1").bind(j.get::<Uuid,_>("id")).bind(ids.last().copied().or(cursor)).bind(ids.len()<50).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn decision(
    tx: &mut Tx<'_>,
    receipt: Uuid,
    selection: &Selection,
    reason: &str,
    composition: Option<Uuid>,
) -> Result<()> {
    let fingerprint = crate::source_access::domain::digest(&serde_json::to_vec(&(
        selection,
        reason,
        composition,
    ))?);
    sqlx::query("INSERT INTO source_advertisement_decisions(id,receipt_id,selection,reason,decision_digest,composition_id) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(receipt_id,decision_digest) DO NOTHING").bind(Uuid::now_v7()).bind(receipt).bind(json!(selection)).bind(reason).bind(fingerprint).bind(composition).execute(&mut **tx).await?;
    Ok(())
}
