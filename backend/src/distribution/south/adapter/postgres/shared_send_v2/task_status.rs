//! Batch observations for list and detail; outcome precedence lives in the domain.
use super::*;
pub(super) async fn attach(tx: &mut Tx<'_>, school: Uuid, tasks: &mut [Value]) -> Result<()> {
    let ids: Vec<Uuid> = tasks
        .iter()
        .map(|task| uid(task, "id"))
        .collect::<Result<_>>()?;
    let rows: Vec<(Uuid, Value)> = sqlx::query_as("SELECT i.task_id,jsonb_build_object('dispatch_state',ds.state,'attempt_state',a.state,'sent',EXISTS(SELECT 1 FROM mobile_v9_attempts sent LEFT JOIN mobile_execution_grants g ON g.attempt_id=sent.id WHERE sent.item_id=i.id AND (sent.state IN ('observed_sent','manually_confirmed_sent') OR g.result_body->>'outcome'='observed_sent'))) FROM mobile_v9_task_items i JOIN mobile_v9_attempts a ON a.item_id=i.id AND a.current LEFT JOIN mobile_v9_dispatch_state ds ON ds.item_id=i.id WHERE i.school_id=$1 AND i.task_id=ANY($2)")
        .bind(school).bind(&ids).fetch_all(&mut **tx).await?;
    for task in tasks {
        let id = uid(task, "id")?;
        let targets: Vec<TaskTargetProgress> = rows
            .iter()
            .filter(|(task_id, _)| *task_id == id)
            .map(|(_, value)| serde_json::from_value(value.clone()).map_err(|_| Error::Conflict))
            .collect::<Result<_>>()?;
        task["status"] =
            serde_json::to_value(TaskStatus::project(task["stopped"] == true, &targets))
                .map_err(|_| Error::Conflict)?;
    }
    Ok(())
}
