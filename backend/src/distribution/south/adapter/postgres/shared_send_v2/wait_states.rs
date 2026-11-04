//! Observation-only worker. Feedback failure never rolls back task or inspection work.
use super::*;
fn merge(old: &Value, current: &mut Value, now: chrono::DateTime<chrono::Utc>) {
    if let Some(rows) = current.as_array_mut() {
        for row in rows {
            let previous = old.as_array().and_then(|rows| {
                rows.iter()
                    .find(|v| v["code"] == row["code"] && v["scope_id"] == row["scope_id"])
            });
            let since = previous
                .and_then(|p| p.get("waiting_since"))
                .cloned()
                .unwrap_or(json!(now));
            let changed = previous
                .filter(|p| p["message"] == row["message"] && p["owner"] == row["owner"])
                .and_then(|p| p.get("reason_changed_at"))
                .cloned()
                .unwrap_or(json!(now));
            row["waiting_since"] = since;
            row["reason_changed_at"] = changed;
        }
    }
}
pub(super) async fn observations(tx: &mut Tx<'_>, attempt: Uuid) -> Result<Option<Value>> {
    use sqlx::Acquire;
    let mut read = tx.begin().await?;
    match sqlx::query_scalar(
        "SELECT conditions FROM mobile_v9_execution_wait_observations WHERE attempt_id=$1",
    )
    .bind(attempt)
    .fetch_optional(&mut *read)
    .await
    {
        Ok(value) => {
            read.commit().await?;
            Ok(value)
        }
        Err(error) => {
            read.rollback().await?;
            tracing::warn!(%error,"wait history unavailable; execution conditions remain authoritative");
            Ok(None)
        }
    }
}
impl Repository {
    pub async fn refresh_wait_states(&self) -> Result<()> {
        let rows:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('task',to_jsonb(t),'attempt',to_jsonb(a)||jsonb_build_object('plan',p.plan),'position',i.position,'old',o.conditions) FROM mobile_v9_attempts a JOIN mobile_v9_plans p ON p.id=a.plan_id JOIN mobile_v9_task_items i ON i.id=a.item_id JOIN mobile_v9_tasks t ON t.id=i.task_id LEFT JOIN mobile_v9_execution_wait_observations o ON o.attempt_id=a.id WHERE a.current AND a.state='queued' ORDER BY o.observed_at NULLS FIRST,a.id LIMIT 20").fetch_all(&self.0).await?;
        let threshold = std::env::var("WEKEEPER_SHARED_SEND_WAIT_ALERT_SECONDS")
            .ok()
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|n| *n >= 60)
            .unwrap_or(1800);
        for row in rows {
            let mut tx = self.0.begin().await?;
            sqlx::query("SET LOCAL statement_timeout='2s'")
                .execute(&mut *tx)
                .await?;
            let status = match execution_status::assess(
                &mut tx,
                uid(&row["task"], "school_id")?,
                &row["task"],
                row["position"].as_i64().ok_or(Error::Conflict)?,
                &row["attempt"],
            )
            .await
            {
                Ok(status) => status,
                Err(error) => {
                    tx.rollback().await?;
                    tracing::warn!(%error,"wait observation will retry");
                    continue;
                }
            };
            let now = chrono::Utc::now();
            let mut conditions = status["conditions"].clone();
            merge(&row["old"], &mut conditions, now);
            let persisted=sqlx::query("INSERT INTO mobile_v9_execution_wait_observations(attempt_id,conditions) SELECT $1,$2 WHERE EXISTS(SELECT 1 FROM mobile_v9_attempts WHERE id=$1 AND current AND state='queued') ON CONFLICT(attempt_id) DO UPDATE SET conditions=$2,observed_at=clock_timestamp()")
                .bind(uid(&row["attempt"],"id")?).bind(&conditions).execute(&mut *tx).await?;
            tx.commit().await?;
            if persisted.rows_affected() == 0 {
                continue;
            }
            for condition in conditions.as_array().into_iter().flatten() {
                let since = condition["waiting_since"]
                    .as_str()
                    .and_then(|s| s.parse::<chrono::DateTime<chrono::Utc>>().ok());
                if !since.is_some_and(|at| now - at >= chrono::Duration::seconds(threshold)) {
                    continue;
                }
                let feedback=sqlx::query("INSERT INTO mobile_v9_support_requests(id,device_id,school_id,reason,state,task_id,wait_code) VALUES($1,$2,$3,$4,'submitted',$5,$6) ON CONFLICT(task_id,wait_code) WHERE task_id IS NOT NULL AND wait_code IS NOT NULL DO NOTHING")
                    .bind(Uuid::now_v7()).bind(uid(&row["attempt"]["plan"],"device_id")?).bind(uid(&row["task"],"school_id")?).bind(format!("任务长时间等待：{}",condition["message"].as_str().unwrap_or("等待执行条件"))).bind(uid(&row["task"],"id")?).bind(format!("{}:{}",condition["code"].as_str().unwrap_or("waiting"),condition["scope_id"].as_str().unwrap_or(""))).execute(&self.0).await;
                if let Err(error) = feedback {
                    tracing::warn!(%error,"wait feedback failed; execution remains independent");
                }
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unchanged_wait_keeps_start_but_changed_reason_gets_a_real_observation_time() {
        let before = chrono::Utc::now() - chrono::Duration::hours(1);
        let now = chrono::Utc::now();
        let old = json!([{"code":"offline","scope_id":"device","message":"离线","owner":"system","waiting_since":before,"reason_changed_at":before}]);
        let mut same =
            json!([{"code":"offline","scope_id":"device","message":"离线","owner":"system"}]);
        merge(&old, &mut same, now);
        assert_eq!(same[0]["waiting_since"], json!(before));
        assert_eq!(same[0]["reason_changed_at"], json!(before));
        same[0]["message"] = json!("连接正在恢复");
        merge(&old, &mut same, now);
        assert_eq!(same[0]["waiting_since"], json!(before));
        assert_eq!(same[0]["reason_changed_at"], json!(now));
    }
}
