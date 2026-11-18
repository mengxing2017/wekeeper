//! Shared official identity persistence for directory and usage confirmation.
use crate::business::{Error, Result, require};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;
type Tx<'a> = Transaction<'a, Postgres>;

pub(crate) async fn snapshot(
    tx: &mut Tx<'_>,
    account: Uuid,
    member: Uuid,
    generation: i64,
) -> Result<(Value, Value)> {
    sqlx::query(
        "SELECT singleton FROM wecom_directory_state WHERE credential_generation=$1 FOR SHARE",
    )
    .bind(generation)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(Error::Conflict)?;
    let member_row: Value = sqlx::query_scalar("SELECT to_jsonb(m) FROM wecom_members m WHERE id=$1 AND verification='verified' AND status=1 AND visibility='visible' AND checked_at>now()-interval '24 hours' FOR SHARE")
        .bind(member).fetch_optional(&mut **tx).await?.ok_or_else(||Error::Invalid("成员目录未准备，请由系统管理员处理".into()))?;
    let account_row: Value = sqlx::query_scalar("SELECT jsonb_build_object('id',id,'display_name',display_name,'wecom_member_id',wecom_member_id,'binding_revision',wecom_binding_revision) FROM accounts WHERE id=$1 AND active FOR UPDATE")
        .bind(account).fetch_optional(&mut **tx).await?.ok_or(Error::NotFound)?;
    require(
        account_row["wecom_member_id"].is_null() || account_row["wecom_member_id"] == json!(member),
        "该账号已绑定其他官方成员，请先解除原关系",
    )?;
    let collision: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM accounts WHERE wecom_member_id=$1 AND id<>$2)",
    )
    .bind(member)
    .bind(account)
    .fetch_one(&mut **tx)
    .await?;
    require(!collision, "该官方成员已关联其他账号，请由系统管理员核对")?;
    Ok((member_row, account_row))
}

/// Callers acquire the official-write barrier before authorization and row locks.
pub(crate) async fn write(
    tx: &mut Tx<'_>,
    actor: Uuid,
    account: Uuid,
    member: Option<Uuid>,
    revision: i64,
    generation: Option<i64>,
) -> Result<()> {
    crate::distribution::south::adapter::postgres::authorization::authorize(tx, actor).await?;
    let old = if let Some(member) = member {
        snapshot(tx, account, member, generation.ok_or(Error::Conflict)?)
            .await?
            .1
    } else {
        sqlx::query_scalar::<_, Value>("SELECT jsonb_build_object('wecom_member_id',wecom_member_id,'binding_revision',wecom_binding_revision) FROM accounts WHERE id=$1 FOR UPDATE")
            .bind(account).fetch_optional(&mut **tx).await?.ok_or(Error::NotFound)?
    };
    require(old["binding_revision"] == revision, "revision_conflict")?;
    if old["wecom_member_id"] == json!(member) {
        return Ok(());
    }
    sqlx::query("UPDATE accounts SET wecom_member_id=$2,wecom_binding_revision=wecom_binding_revision+1 WHERE id=$1")
        .bind(account).bind(member).execute(&mut **tx).await?;
    // Only OAuth sessions depend on the official identity. Password admin sessions stay valid.
    sqlx::query("DELETE FROM sessions WHERE account_id=$1 AND wecom_authenticated")
        .bind(account)
        .execute(&mut **tx)
        .await?;
    sqlx::query("INSERT INTO audit_events(id,actor_id,action,object_id,detail) VALUES($1,$2,'wecom.binding.update',$3,$4)")
        .bind(Uuid::now_v7()).bind(actor).bind(account).bind(json!({"member_id":member,"previous_revision":revision,"previous_member_id":old["wecom_member_id"]})).execute(&mut **tx).await?;
    Ok(())
}
