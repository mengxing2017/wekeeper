//! Unit schemes and task metadata stay on the server; no device protocol fields are added.
use super::*;
use crate::distribution::domain::shared_mobile::business_scheme::{
    BusinessSchemeInput, SchemeMutation, SendingStatus, TaskPreviewInput,
};

async fn unit_exists(tx: &mut Tx<'_>, unit: Option<Uuid>) -> Result<()> {
    if let Some(unit) = unit {
        require(
            sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM business_units WHERE id=$1)",
            )
            .bind(unit)
            .fetch_one(&mut **tx)
            .await?,
            "business_unit_not_found",
        )?;
    }
    Ok(())
}
async fn latest(tx: &mut Tx<'_>, unit: Option<Uuid>) -> Result<Option<Value>> {
    sqlx::query_scalar("SELECT to_jsonb(v) FROM mobile_business_scheme_versions v WHERE business_unit_id IS NOT DISTINCT FROM $1 ORDER BY revision DESC LIMIT 1")
        .bind(unit).fetch_optional(&mut **tx).await.map_err(Into::into)
}
pub(super) async fn resolve(tx: &mut Tx<'_>, unit: Option<Uuid>) -> Result<Value> {
    unit_exists(tx, unit).await?;
    let own = if unit.is_some() {
        latest(tx, unit).await?
    } else {
        None
    };
    let own_revision = own
        .as_ref()
        .and_then(|v| v["revision"].as_i64())
        .unwrap_or(0);
    let selected = match own.filter(|v| v["action"] == "save") {
        Some(v) => Some((v, "unit")),
        None => latest(tx, None).await?.map(|v| (v, "default")),
    };
    let mut p = default_policy();
    if let Some((v, source)) = selected {
        p = v["body"].clone();
        p["id"] = v["id"].clone();
        p["source"] = json!(source);
        p["revision"] = v["revision"].clone();
    } else {
        p["source"] = json!("builtin");
        p["builtin_version"] = json!("mobile_business_default/1");
    }
    p["business_unit_id"] = json!(unit);
    p["scope_revision"] = if unit.is_some() {
        json!(own_revision)
    } else {
        p["revision"].clone()
    };
    Ok(p)
}
pub(super) async fn status_for(tx: &mut Tx<'_>, unit: Option<Uuid>) -> Result<SendingStatus> {
    let p = resolve(tx, unit).await?;
    Ok(SendingStatus::project(if p["source"] == "builtin" {
        None
    } else {
        p["mode"].as_str()
    }))
}
pub(super) async fn current_for_attempt(tx: &mut Tx<'_>, attempt: Uuid) -> Result<Value> {
    let unit: Option<Uuid> = sqlx::query_scalar("SELECT t.business_unit_id FROM mobile_v9_attempts a JOIN mobile_v9_task_items i ON i.id=a.item_id JOIN mobile_v9_tasks t ON t.id=i.task_id WHERE a.id=$1")
        .bind(attempt).fetch_one(&mut **tx).await?;
    resolve(tx, unit).await
}
pub(super) async fn frozen_for_attempt(tx: &mut Tx<'_>, attempt: Uuid) -> Result<Value> {
    sqlx::query_scalar("SELECT t.scheme_snapshot FROM mobile_v9_attempts a JOIN mobile_v9_task_items i ON i.id=a.item_id JOIN mobile_v9_tasks t ON t.id=i.task_id WHERE a.id=$1")
        .bind(attempt).fetch_one(&mut **tx).await.map_err(Into::into)
}
pub(super) async fn read(repo: &Repository, a: &Actor, unit: Option<Uuid>) -> Result<Value> {
    let mut tx = repo.0.begin().await?;
    authorize(&mut tx, a.id, true).await?;
    let p = resolve(&mut tx, unit).await?;
    tx.commit().await?;
    Ok(p)
}
pub(super) async fn write(
    repo: &Repository,
    a: &Actor,
    key: &str,
    unit: Option<Uuid>,
    input: Option<BusinessSchemeInput>,
    mutation: SchemeMutation,
) -> Result<Value> {
    let mut tx = repo.0.begin().await?;
    guard(&mut tx).await?;
    authorize(&mut tx, a.id, true).await?;
    unit_exists(&mut tx, unit).await?;
    require(
        mutation.confirmed && mutation.expected_revision >= 0,
        "confirmation_required",
    )?;
    crate::business::text(&mutation.reason, 1, 2000)?;
    require(
        input.is_some() || unit.is_some(),
        "shared_send_policy_invalid",
    )?;
    let scope = format!("business-scheme:{unit:?}");
    let body = json!({"input":input,"mutation":mutation});
    if let Some(old) = replay(&mut tx, a.id, 0, &scope, key, &body).await? {
        tx.commit().await?;
        return Ok(old);
    }
    let revision = latest(&mut tx, unit)
        .await?
        .and_then(|v| v["revision"].as_i64())
        .unwrap_or(0);
    require(revision == mutation.expected_revision, "revision_conflict")?;
    let value = if let Some(input) = input {
        let p = input.policy()?;
        require(
            sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM pg_timezone_names WHERE name=$1)",
            )
            .bind(&p.timezone)
            .fetch_one(&mut *tx)
            .await?,
            "shared_send_policy_invalid",
        )?;
        json!(p)
    } else {
        Value::Null
    };
    sqlx::query("INSERT INTO mobile_business_scheme_versions(id,business_unit_id,revision,action,body,created_by,reason) VALUES($1,$2,$3,$4,$5,$6,$7)")
        .bind(Uuid::now_v7()).bind(unit).bind(revision+1).bind(if value.is_null(){"inherit"}else{"save"}).bind(&value).bind(a.id).bind(&mutation.reason).execute(&mut *tx).await?;
    let result = resolve(&mut tx, unit).await?;
    sqlx::query("INSERT INTO audit_events(id,actor_id,action,object_id,detail,request_id) VALUES($1,$2,'mobile.business_scheme',$3,$4,$5)")
        .bind(Uuid::now_v7()).bind(a.id).bind(unit.unwrap_or(a.id)).bind(json!({"scope":scope,"previous_revision":revision,"new_revision":revision+1,"reason":mutation.reason,"effective_scheme":result})).bind(&a.request_id).execute(&mut *tx).await?;

    remember(&mut tx, a.id, 0, &scope, key, &body, &result).await?;
    tx.commit().await?;
    Ok(result)
}
pub(super) async fn preview(
    repo: &Repository,
    a: &Actor,
    key: &str,
    input: TaskPreviewInput,
) -> Result<Value> {
    let mut tx = repo.0.begin().await?;
    guard(&mut tx).await?;
    authorize(&mut tx, a.id, true).await?;
    let body = json!(input);
    if let Some(old) = replay(&mut tx, a.id, 0, "task-preview", key, &body).await? {
        tx.commit().await?;
        return Ok(old);
    }
    unit_exists(&mut tx, input.business_unit_id).await?;
    let draft = tasks::write(
        &mut tx,
        a,
        TaskCommand::Draft(
            input.draft_id,
            DraftInput {
                client_kind: input.client_kind,
                target_ids: input.target_ids,
                message: input.message,
                content_type: "text/plain".into(),
                expected_revision: input.expected_revision,
                between_targets_seconds: input.between_targets_seconds,
            },
        ),
    )
    .await?;
    sqlx::query("UPDATE mobile_v9_task_drafts SET business_unit_id=$2 WHERE id=$1")
        .bind(uid(&draft, "id")?)
        .bind(input.business_unit_id)
        .execute(&mut *tx)
        .await?;
    let preview = tasks::write(
        &mut tx,
        a,
        TaskCommand::Preview(PreviewInput {
            draft_id: uid(&draft, "id")?,
            expected_revision: draft["revision"].as_i64().ok_or(Error::Conflict)?,
            selections: input.selections,
        }),
    )
    .await?;
    let mut draft = draft;
    draft["business_unit_id"] = json!(input.business_unit_id);
    let result = json!({"id":preview["id"],"draft":draft,"preview":preview});
    remember(&mut tx, a.id, 0, "task-preview", key, &body, &result).await?;
    tx.commit().await?;
    Ok(result)
}
