//! Device observations are independent of UI readiness, recovery, leases and sending.
use super::*;
use chrono::{DateTime, Utc};
#[derive(serde::Serialize, serde::Deserialize)]
struct Cursor {
    device: Uuid,
    generation: i64,
    revision: i64,
    at: DateTime<Utc>,
    last_at: DateTime<Utc>,
    last_id: Uuid,
}
pub(super) async fn read(repo: &Repository, c: &Credential, q: InboxQuery) -> Result<Value> {
    let limit = q.limit.unwrap_or(50);
    require((1..=100).contains(&limit), "shared_inbox_limit_invalid")?;
    let mut tx = repo.0.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
        .execute(&mut *tx)
        .await?;
    authenticate(&mut tx, c, false).await?;
    let revision:i64=sqlx::query_scalar("SELECT coalesce((SELECT revision FROM mobile_v9_device_inbox_versions WHERE device_id=$1),0)").bind(c.device_id).fetch_one(&mut *tx).await?;
    if let Some(ref cursor) = q.cursor {
        require(cursor.len() <= 4096, "shared_inbox_cursor_invalid")?;
    }
    let cursor = q
        .cursor
        .map(|s| {
            serde_json::from_str::<Cursor>(&s)
                .map_err(|_| Error::Policy("shared_inbox_cursor_invalid"))
        })
        .transpose()?;
    let at = if let Some(ref cursor) = cursor {
        require(
            cursor.device == c.device_id && cursor.generation == c.generation,
            "shared_inbox_cursor_invalid",
        )?;
        require(cursor.revision == revision, "shared_inbox_snapshot_changed")?;
        require(
            cursor.at <= Utc::now() && cursor.at > Utc::now() - chrono::Duration::hours(24),
            "shared_inbox_cursor_invalid",
        )?;
        cursor.at
    } else {
        Utc::now()
    };
    let after = cursor
        .as_ref()
        .map(|x| x.last_at)
        .unwrap_or(DateTime::<Utc>::UNIX_EPOCH);
    let after_id = cursor.as_ref().map(|x| x.last_id).unwrap_or(Uuid::nil());
    let rows:Vec<Value>=sqlx::query_scalar(r#"SELECT jsonb_build_object('task',to_jsonb(t),'attempt',to_jsonb(a)||jsonb_build_object('plan',p.plan),'school_name',s.name,'device_name',d.name,'target_name',coalesce(nullif(p.plan->>'target_name',''),nullif(mt.alias,''),mt.search_text,'目标群已移除'),'assigned_at',a.inbox_assigned_at,'position',i.position,'total',(SELECT count(*) FROM mobile_v9_task_items WHERE task_id=t.id),'assigned_total',(SELECT count(DISTINCT pp.item_id) FROM mobile_v9_plans pp JOIN mobile_v9_task_items ii ON ii.id=pp.item_id WHERE ii.task_id=t.id AND pp.device_id=$1)) FROM mobile_v9_attempts a JOIN mobile_v9_plans p ON p.id=a.plan_id JOIN mobile_v9_task_items i ON i.id=a.item_id JOIN mobile_v9_tasks t ON t.id=i.task_id JOIN schools s ON s.id=t.school_id JOIN execution_devices d ON d.id=p.device_id LEFT JOIN mobile_targets mt ON mt.id=i.target_id WHERE p.device_id=$1 AND (p.plan->>'credential_generation')::bigint=$2 AND ((a.current AND a.state IN ('queued','leased','prepared','invocation_issued')) OR a.state='unknown' OR a.inbox_changed_at>=$3-interval '7 days') AND (a.inbox_assigned_at,a.id)>($4,$5) ORDER BY a.inbox_assigned_at,a.id LIMIT $6"#).bind(c.device_id).bind(c.generation).bind(at).bind(after).bind(after_id).bind(limit+1).fetch_all(&mut *tx).await?;
    let more = rows.len() > limit as usize;
    let mut items = Vec::new();
    for row in rows.iter().take(limit as usize) {
        let task = &row["task"];
        let attempt = &row["attempt"];
        let plan = &attempt["plan"];
        let position = row["position"].as_i64().ok_or(Error::Conflict)?;
        let status =
            execution_status::assess(&mut tx, uid(task, "school_id")?, task, position, attempt)
                .await?;
        items.push(json!({"task_id":task["id"],"item_id":attempt["item_id"],"attempt_id":attempt["id"],"assignment_revision":1,"school_id":task["school_id"],"school_name":row["school_name"],"target_id":plan["target_id"],"target_name":row["target_name"],"target":plan["target"],"search_text":plan["target"]["search_query"],"device_id":c.device_id,"device_name":row["device_name"],"position":position,"total":row["total"],"assigned_total":row["assigned_total"],"assigned_at":row["assigned_at"],"confirmed_until":task["confirmed_until"],"attempt_state":attempt["state"],"current":attempt["current"],"task_stopped":task["stopped"],"confirmation_expired":task["confirmed_until"].as_str().and_then(|x|x.parse::<DateTime<Utc>>().ok()).is_none_or(|x|x<=Utc::now()),"next_eligible_at":task["next_eligible_at"],"execution_status":status}));
    }
    let next = if more {
        let last = rows.get(limit as usize - 1).ok_or(Error::Conflict)?;
        Some(
            serde_json::to_string(&Cursor {
                device: c.device_id,
                generation: c.generation,
                revision,
                at,
                last_at: serde_json::from_value(last["assigned_at"].clone())
                    .map_err(|_| Error::Conflict)?,
                last_id: uid(&last["attempt"], "id")?,
            })
            .map_err(|_| Error::Conflict)?,
        )
    } else {
        None
    };
    tx.commit().await?;
    Ok(
        json!({"id":c.device_id,"device_id":c.device_id,"credential_generation":c.generation,"queue_revision":revision,"observed_at":Utc::now(),"items":items,"page":{"has_more":more,"next_cursor":next}}),
    )
}
pub(super) async fn receipt(
    repo: &Repository,
    c: &Credential,
    key: &str,
    v: InboxReceipt,
) -> Result<Value> {
    require(
        !v.items.is_empty() && v.items.len() <= 100 && v.queue_revision >= 0,
        "shared_inbox_receipt_invalid",
    )?;
    let body = json!(v);
    let h = digest(&body)?;
    let mut tx = repo.0.begin().await?;
    authenticate(&mut tx, c, false).await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,124))")
        .bind(format!("{}:{}:{key}", c.device_id, c.generation))
        .execute(&mut *tx)
        .await?;
    let old:Option<(String,Value)>=sqlx::query_as("SELECT request_hash,response FROM mobile_v9_inbox_receipt_operations WHERE device_id=$1 AND generation=$2 AND idempotency_key=$3").bind(c.device_id).bind(c.generation).bind(key).fetch_optional(&mut *tx).await?;
    if let Some((hash, response)) = old {
        require(hash == h, "idempotency_payload_conflict")?;
        return Ok(response);
    }
    let mut accepted = Vec::new();
    let mut rejected = Vec::new();
    for item in &v.items {
        let owned:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_v9_attempts a JOIN mobile_v9_plans p ON p.id=a.plan_id WHERE a.id=$1 AND p.device_id=$2 AND (p.plan->>'credential_generation')::bigint=$3)").bind(item.attempt_id).bind(c.device_id).bind(c.generation).fetch_one(&mut *tx).await?;
        if !owned || item.assignment_revision != 1 {
            rejected.push(
                json!({"attempt_id":item.attempt_id,"code":"shared_inbox_assignment_mismatch"}),
            );
            continue;
        }
        let at:DateTime<Utc>=sqlx::query_scalar("INSERT INTO mobile_v9_task_receipts(attempt_id,device_id,credential_generation,assignment_revision) VALUES($1,$2,$3,1) ON CONFLICT(attempt_id,device_id,credential_generation,assignment_revision) DO UPDATE SET received_at=mobile_v9_task_receipts.received_at RETURNING received_at").bind(item.attempt_id).bind(c.device_id).bind(c.generation).fetch_one(&mut *tx).await?;
        accepted.push(json!({"attempt_id":item.attempt_id,"received_at":at}));
    }
    let response = json!({"id":v.receipt_id,"accepted":accepted,"rejected":rejected});
    sqlx::query(
        "INSERT INTO mobile_v9_inbox_receipt_operations VALUES($1,$2,$3,$4,$5,clock_timestamp())",
    )
    .bind(c.device_id)
    .bind(c.generation)
    .bind(key)
    .bind(h)
    .bind(&response)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(response)
}
impl Repository {
    pub async fn dispatch_task_wakes(&self) -> Result<()> {
        let mut tx = self.0.begin().await?;
        let rows:Vec<(Uuid,Uuid,Uuid)>=sqlx::query_as("SELECT id,device_id,actor_id FROM mobile_v9_task_wake_outbox ORDER BY created_at LIMIT 100 FOR UPDATE SKIP LOCKED").fetch_all(&mut *tx).await?;
        let mut notified = std::collections::HashSet::new();
        for (id, device, actor) in rows {
            if notified.insert(device) {
                sqlx::query("INSERT INTO device_control_commands(id,device_id,kind,request_key,request_digest,actor_id) VALUES($1,$2,'task_available',$3,$4,$5) ON CONFLICT(device_id,request_key) DO NOTHING").bind(id).bind(device).bind(format!("shared-inbox:{id}")).bind(hash(&id.to_string())).bind(actor).execute(&mut *tx).await?;
            }
            sqlx::query("DELETE FROM mobile_v9_task_wake_outbox WHERE id=$1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}
