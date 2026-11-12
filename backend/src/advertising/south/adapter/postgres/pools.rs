//! Pool administration and atomic consumption; all SQL remains in this adapter.
use super::*;
use crate::advertising::domain::pool as p;

pub(super) async fn cursor(tx: &mut Tx<'_>, id: Uuid, lock: bool) -> Result<p::Cursor> {
    let query = if lock {
        "SELECT to_jsonb(c)-'pool_id' FROM advertisement_pool_cursors c WHERE pool_id=$1 FOR UPDATE"
    } else {
        "SELECT to_jsonb(c)-'pool_id' FROM advertisement_pool_cursors c WHERE pool_id=$1 FOR SHARE"
    };
    let v: Value = sqlx::query_scalar(query)
        .bind(id)
        .fetch_one(&mut **tx)
        .await?;
    Ok(serde_json::from_value(v)?)
}
pub(super) async fn slots(tx: &mut Tx<'_>, id: Uuid, revision: i64) -> Result<Vec<p::Slot>> {
    let values:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('slot_id',slot_id,'ad_id',ad_id) FROM advertisement_pool_slots WHERE pool_id=$1 AND revision=$2 ORDER BY position").bind(id).bind(revision).fetch_all(&mut **tx).await?;
    values
        .into_iter()
        .map(|v| serde_json::from_value(v).map_err(Into::into))
        .collect()
}
async fn id(tx: &mut Tx<'_>, route: &str) -> Result<Uuid> {
    sqlx::query_scalar("SELECT id FROM advertisement_pools WHERE route_id=$1")
        .bind(route)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(Fault("ad_pool_missing"))
}
pub(super) async fn write_cursor(tx: &mut Tx<'_>, id: Uuid, c: &p::Cursor) -> Result<()> {
    sqlx::query("UPDATE advertisement_pool_cursors SET pool_revision=$2,cursor_version=$3,reset_epoch=$4,next_slot_id=$5,last_consumed_slot_id=$6,cycle=$7,selection_order=$8 WHERE pool_id=$1").bind(id).bind(c.pool_revision).bind(c.cursor_version).bind(c.reset_epoch).bind(c.next_slot_id).bind(c.last_consumed_slot_id).bind(c.cycle).bind(c.selection_order).execute(&mut **tx).await?;
    Ok(())
}
async fn transition(
    tx: &mut Tx<'_>,
    id: Uuid,
    actor: Uuid,
    kind: &str,
    reason: &str,
    before: &p::Cursor,
    after: &p::Cursor,
) -> Result<()> {
    sqlx::query("INSERT INTO advertisement_pool_transitions(id,pool_id,kind,actor_id,reason,before_cursor,after_cursor) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(Uuid::now_v7()).bind(id).bind(kind).bind(actor).bind(reason).bind(json!(before)).bind(json!(after)).execute(&mut **tx).await?;
    Ok(())
}
pub(super) async fn view(tx: &mut Tx<'_>, route: &str) -> Result<Value> {
    let pool: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM advertisement_pools WHERE route_id=$1")
            .bind(route)
            .fetch_optional(&mut **tx)
            .await?;
    let Some(pool) = pool else {
        return Ok(
            json!({"id":route,"pool_id":null,"revision":0,"slots":[],"cursor":null,"transitions":[]}),
        );
    };
    let c = cursor(tx, pool, false).await?;
    let slots = slots(tx, pool, c.pool_revision).await?;
    let transitions:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(t) FROM advertisement_pool_transitions t WHERE pool_id=$1 ORDER BY created_at DESC,id DESC LIMIT 30").bind(pool).fetch_all(&mut **tx).await?;
    Ok(
        json!({"id":route,"pool_id":pool,"revision":c.pool_revision,"slots":slots,"cursor":c,"transitions":transitions}),
    )
}
pub(super) async fn publish(
    tx: &mut Tx<'_>,
    actor: Uuid,
    route: &str,
    input: p::Publication,
    kind: &str,
) -> Result<Value> {
    input.validate().map_err(|e| Fault(e.0))?;
    // Lock the route config in write mode from the start; never upgrade a shared config lock.
    sqlx::query("SELECT route_id FROM source_advertisement_configs WHERE route_id=$1 FOR UPDATE")
        .bind(route)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(Fault("not_found"))?;
    let candidate = Uuid::now_v7();
    sqlx::query("INSERT INTO advertisement_pools(id,route_id,created_by) VALUES($1,$2,$3) ON CONFLICT(route_id) DO NOTHING").bind(candidate).bind(route).bind(actor).execute(&mut **tx).await?;
    let pool = id(tx, route).await?;
    sqlx::query(
        "INSERT INTO advertisement_pool_cursors(pool_id) VALUES($1) ON CONFLICT DO NOTHING",
    )
    .bind(pool)
    .execute(&mut **tx)
    .await?;
    let before = cursor(tx, pool, true).await?;
    ensure(
        before.pool_revision == input.expected_revision,
        "revision_conflict",
    )?;
    let old = slots(tx, pool, before.pool_revision).await?;
    let snapshot = route_snapshot(tx, route).await?;
    let mut new = vec![];
    for slot in input.slots {
        if let Some(slot_id) = slot.slot_id {
            ensure(
                old.iter()
                    .any(|s| s.slot_id == slot_id && s.ad_id == slot.ad_id),
                "ad_pool_invalid_slot",
            )?;
        }
        let r = ad_row(tx, slot.ad_id).await?;
        // Existing unavailable slots can remain visible for repair; new selections must be authorized.
        if slot.slot_id.is_none() {
            ensure(ad_exclusion(&r, &snapshot)?.is_none(), "ad_not_allowed")?;
        }
        new.push(p::Slot {
            slot_id: slot.slot_id.unwrap_or_else(Uuid::now_v7),
            ad_id: slot.ad_id,
        });
    }
    let after = p::published_cursor(&new, &before);
    sqlx::query("INSERT INTO advertisement_pool_revisions(pool_id,revision,actor_id,reason) VALUES($1,$2,$3,$4)").bind(pool).bind(after.pool_revision).bind(actor).bind(&input.reason).execute(&mut **tx).await?;
    for (i, slot) in new.iter().enumerate() {
        sqlx::query("INSERT INTO advertisement_pool_slots(pool_id,revision,slot_id,position,ad_id) VALUES($1,$2,$3,$4,$5)").bind(pool).bind(after.pool_revision).bind(slot.slot_id).bind((i+1) as i32).bind(slot.ad_id).execute(&mut **tx).await?;
    }
    sqlx::query("UPDATE advertisement_pools SET current_revision=$2 WHERE id=$1")
        .bind(pool)
        .bind(after.pool_revision)
        .execute(&mut **tx)
        .await?;
    write_cursor(tx, pool, &after).await?;
    transition(tx, pool, actor, kind, &input.reason, &before, &after).await?;
    enqueue(tx, None, Some(route), chrono::Utc::now()).await?;
    view(tx, route).await
}
pub(super) async fn reset(
    tx: &mut Tx<'_>,
    actor: Uuid,
    route: &str,
    input: p::Reset,
) -> Result<Value> {
    let pool = id(tx, route).await?;
    let before = cursor(tx, pool, true).await?;
    ensure(
        before.pool_revision == input.expected_revision
            && before.cursor_version == input.expected_cursor_version,
        "revision_conflict",
    )?;
    let slots = slots(tx, pool, before.pool_revision).await?;
    let mut after = before.clone();
    after.cursor_version += 1;
    after.reset_epoch += 1;
    after.cycle = 0;
    after.last_consumed_slot_id = None;
    after.next_slot_id = slots.first().map(|s| s.slot_id);
    write_cursor(tx, pool, &after).await?;
    transition(tx, pool, actor, "reset", &input.reason, &before, &after).await?;
    view(tx, route).await
}
pub(super) async fn consumptions(tx: &mut Tx<'_>, route: &str, page: i64) -> Result<Value> {
    let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(c) FROM advertisement_pool_consumptions c JOIN advertisement_pools p ON p.id=c.pool_id WHERE p.route_id=$1 ORDER BY c.selection_order DESC LIMIT 31 OFFSET $2").bind(route).bind((page-1)*30).fetch_all(&mut **tx).await?;
    let slot_counts:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('slot_id',c.slot_id,'ad_id',c.selection->>'ad_id','selections',count(*)) FROM advertisement_pool_consumptions c JOIN advertisement_pools p ON p.id=c.pool_id WHERE p.route_id=$1 GROUP BY c.slot_id,c.selection->>'ad_id' ORDER BY c.slot_id").bind(route).fetch_all(&mut **tx).await?;
    let ad_counts:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('ad_id',c.selection->>'ad_id','selections',count(*)) FROM advertisement_pool_consumptions c JOIN advertisement_pools p ON p.id=c.pool_id WHERE p.route_id=$1 GROUP BY c.selection->>'ad_id' ORDER BY c.selection->>'ad_id'").bind(route).fetch_all(&mut **tx).await?;
    let statistics:Value=sqlx::query_scalar("SELECT jsonb_build_object('selections',count(*),'valid_targets',coalesce(sum(r.summary_total),0),'device_attempts',coalesce(sum((SELECT count(*) FROM source_execution_generations g JOIN mobile_v9_task_items i ON i.task_id=g.task_id JOIN mobile_v9_attempts a ON a.item_id=i.id WHERE g.receipt_id=c.receipt_id)),0),'sent_targets',coalesce(sum((r.sending->>'sent')::bigint),0),'failed_targets',coalesce(sum((r.sending->>'failed')::bigint),0),'unknown_targets',coalesce(sum((r.sending->>'unknown')::bigint),0),'pending_targets',coalesce(sum((r.sending->>'pending')::bigint),0)) FROM advertisement_pool_consumptions c JOIN advertisement_pools p ON p.id=c.pool_id JOIN source_receipts r ON r.receipt_id=c.receipt_id WHERE p.route_id=$1").bind(route).fetch_one(&mut **tx).await?;
    Ok(
        json!({"items":rows.iter().take(30).collect::<Vec<_>>(),"has_more":rows.len()>30,"statistics":statistics,"slot_statistics":slot_counts,"ad_statistics":ad_counts}),
    )
}
pub(crate) async fn validate(tx: &mut Tx<'_>, receipt: Uuid, p: &Prepared) -> Result<()> {
    let Some(pool) = &p.selection.pool else {
        return Ok(());
    };
    if p.base.is_some() {
        return Ok(());
    }
    let snapshot: crate::source_access::domain::routing::RouteSnapshot = serde_json::from_value(
        sqlx::query_scalar::<_, Value>(
            "SELECT snapshot FROM source_current_routing_decisions WHERE receipt_id=$1",
        )
        .bind(receipt)
        .fetch_one(&mut **tx)
        .await?,
    )?;
    let current = cursor(tx, pool.pool_id, true).await?;
    ensure(current == pool.before, "ad_selection_stale")?;
    let route: String =
        sqlx::query_scalar("SELECT route_id FROM source_receipts WHERE receipt_id=$1")
            .bind(receipt)
            .fetch_one(&mut **tx)
            .await?;
    ensure(id(tx, &route).await? == pool.pool_id, "ad_pool_wrong_route")?;
    // Every scanned version matters, including excluded slots whose eligibility may have changed.
    let mut candidates: Vec<_> = p.selection.candidates.iter().collect();
    candidates.sort_by_key(|c| c.ad_id);
    for c in candidates {
        let r = ad_row(tx, c.ad_id).await?;
        ensure(
            r.get::<Option<Uuid>, _>("content_version") == c.content_version
                && r.get::<Option<Uuid>, _>("policy_version") == c.policy_version,
            "ad_selection_stale",
        )?;
        if c.exclusion.as_deref().is_some_and(|e| {
            matches!(
                e,
                "ad_not_started"
                    | "ad_expired"
                    | "ad_disabled"
                    | "ad_denied"
                    | "ad_not_allowed"
                    | "ad_archived"
                    | "ad_unpublished"
            )
        }) {
            ensure(
                ad_exclusion(&r, &snapshot)? == c.exclusion.as_deref(),
                "ad_selection_stale",
            )?;
        }
    }
    Ok(())
}
pub(crate) async fn consume(
    tx: &mut Tx<'_>,
    receipt: Uuid,
    composition: Uuid,
    p: &Prepared,
) -> Result<()> {
    let Some(pool) = &p.selection.pool else {
        return Ok(());
    };
    if p.base.is_some() {
        return Ok(());
    }
    sqlx::query("UPDATE advertisement_pool_queue SET pool_id=$2 WHERE receipt_id=$1")
        .bind(receipt)
        .bind(pool.pool_id)
        .execute(&mut **tx)
        .await?;
    if let Some(slot) = pool.selected_slot_id {
        sqlx::query("INSERT INTO advertisement_pool_consumptions(id,pool_id,pool_revision,receipt_id,chain_decision_id,selection_order,slot_id,composition_id,selection) VALUES($1,$2,$3,$4,$4,$5,$6,$7,$8)").bind(Uuid::now_v7()).bind(pool.pool_id).bind(pool.before.pool_revision).bind(receipt).bind(pool.after.selection_order).bind(slot).bind(composition).bind(json!(p.selection)).execute(&mut **tx).await?;
        write_cursor(tx, pool.pool_id, &pool.after).await?;
        tracing::info!(pool_id=%pool.pool_id,receipt_id=%receipt,slot_id=%slot,selection_order=pool.after.selection_order,"advertisement pool consumption staged in task transaction");
    }
    Ok(())
}

/// Read versions in a short transaction, then perform expensive matching without locks.
pub(crate) async fn select(
    db: &PgPool,
    snapshot: &crate::source_access::domain::routing::RouteSnapshot,
    revision: i64,
    config: Config,
    source: &str,
    receipt: Option<Uuid>,
) -> Result<Prepared> {
    use crate::source_access::domain::keyword_filter as k;
    let e = snapshot
        .execution
        .as_ref()
        .ok_or(Fault("route_configuration_required"))?;
    let mut tx = db.begin().await?;
    let pool = id(&mut tx, &snapshot.route_id).await?;
    let before = cursor(&mut tx, pool, false).await?;
    let slots = slots(&mut tx, pool, before.pool_revision).await?;
    let receive_order = if let Some(receipt) = receipt {
        Some(
            sqlx::query_scalar::<_, i64>(
                "SELECT receive_order FROM source_receipts WHERE receipt_id=$1",
            )
            .bind(receipt)
            .fetch_one(&mut *tx)
            .await?,
        )
    } else {
        None
    };
    let mut assets = std::collections::HashMap::new();
    // Stable lock order even when two pools contain the same advertisements in different orders.
    let mut ids: Vec<_> = slots.iter().map(|s| s.ad_id).collect();
    ids.sort();
    ids.dedup();
    for id in ids {
        let row = ad_row(&mut tx, id).await?;
        let excluded = ad_exclusion(&row, snapshot)?.map(str::to_owned);
        let content = if excluded.is_none() {
            Some(Content {
                name: row.get("name"),
                body: row.get("body"),
                link: row.get("link"),
            })
        } else {
            None
        };
        assets.insert(
            id,
            (
                row.get::<Option<Uuid>, _>("content_version"),
                row.get::<Option<Uuid>, _>("policy_version"),
                excluded,
                content,
            ),
        );
    }
    tx.commit().await?;
    let mut selection = Selection {
        pool: None,
        candidates: vec![],
        config: config.clone(),
        config_revision: revision,
        ad_id: None,
        content_version: None,
        policy_version: None,
        composer: k::COMPOSER.into(),
        exclusions: vec![],
        measurement: measure(source, e.channel.as_str()),
    };
    let mut evidence = p::Selection {
        pool_id: pool,
        before: before.clone(),
        after: before.clone(),
        selected_slot_id: None,
        selected_position: None,
        receive_order,
        rule_fingerprint: String::new(),
        scans: vec![],
    };
    let mut results = std::collections::HashMap::<Uuid, (Option<String>, String)>::new();
    let mut content = source.to_owned();
    for index in p::scan_indices(&slots, &before) {
        let slot = &slots[index];
        let (cv, pv, excluded, ad) = &assets[&slot.ad_id];
        let (exclusion, text) = if let Some(result) = results.get(&slot.ad_id) {
            result.clone()
        } else {
            let mut exclusion = excluded.clone();
            let text = ad
                .as_ref()
                .map(|ad| compose(source, Some(ad), &config.insertion_position))
                .unwrap_or_else(|| source.into());
            if exclusion.is_none() && !measure(&text, e.channel.as_str()).valid {
                exclusion = Some("ad_content_too_long".into());
            }
            results.insert(slot.ad_id, (exclusion.clone(), text.clone()));
            (exclusion, text)
        };
        evidence.scans.push(p::Scan {
            slot_id: slot.slot_id,
            position: index + 1,
            ad_id: slot.ad_id,
            exclusion: exclusion.clone(),
        });
        selection.candidates.push(CandidateEvidence {
            ad_id: slot.ad_id,
            content_version: *cv,
            policy_version: *pv,
            exclusion: exclusion.clone(),
        });
        if let Some(reason) = exclusion {
            selection.exclusions.push(reason);
            continue;
        }
        selection.ad_id = Some(slot.ad_id);
        selection.content_version = *cv;
        selection.policy_version = *pv;
        selection.composer = "source-ad-text/1".into();
        content = text;
        evidence.selected_slot_id = Some(slot.slot_id);
        evidence.selected_position = Some(index + 1);
        evidence.after = p::advance(&slots, &before, index);
        break;
    }
    if selection.ad_id.is_none() {
        selection.exclusions.push("ad_pool_no_candidate".into());
    }
    selection.measurement = measure(&content, e.channel.as_str());
    ensure(selection.measurement.valid, "channel_content_invalid")?;
    tracing::info!(pool_id=%pool,route_id=%snapshot.route_id,pool_revision=before.pool_revision,cursor_version=before.cursor_version,scan_count=evidence.scans.len(),unique_ad_checks=results.len(),selected_slot_id=?evidence.selected_slot_id,receive_order=?receive_order,"advertisement pool proposal prepared");
    selection.pool = Some(evidence);
    Ok(Prepared {
        selection,
        content,
        base: None,
        replacement: false,
    })
}
