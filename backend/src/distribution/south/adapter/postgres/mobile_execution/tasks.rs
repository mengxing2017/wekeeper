use super::*;

// Read/cancel historical tasks only. Shared task creation replaces the retired
// school-bound execution plane; never silently reuse it for shared bindings.
pub(super) async fn write(
    tx: &mut Tx<'_>,
    school: Uuid,
    _actor: Uuid,
    action: &str,
    id: Uuid,
    input: &Value,
) -> Result<Value> {
    match action {
        "cancel-task" => {
            let v: RevisionInput = serde_json::from_value(input.clone())
                .map_err(|_| Error::Invalid("取消字段无效".into()))?;
            validate_text(&v.reason, 1, 2000, 8000)?;
            // Only certainly unclaimed queued work may release a reservation here.
            let n=sqlx::query("UPDATE mobile_execution_tasks t SET state='cancelled',revision=revision+1,cancelled_at=clock_timestamp() WHERE school_id=$1 AND id=$2 AND revision=$3 AND state='queued' AND NOT EXISTS(SELECT 1 FROM mobile_execution_task_items i WHERE i.task_id=t.id AND i.state<>'pending')")
                .bind(school).bind(id).bind(v.expected_revision).execute(&mut **tx).await?.rows_affected();
            if n != 1 {
                return Err(Error::Policy("mobile_task_not_cancellable"));
            }
            sqlx::query("UPDATE mobile_execution_task_items SET state='cancelled' WHERE school_id=$1 AND task_id=$2 AND state='pending'").bind(school).bind(id).execute(&mut **tx).await?;
            sqlx::query("DELETE FROM mobile_execution_device_reservations WHERE school_id=$1 AND task_id=$2").bind(school).bind(id).execute(&mut **tx).await?;
            object(tx, "mobile_execution_tasks", id).await
        }
        _ => Err(Error::NotFound),
    }
}
