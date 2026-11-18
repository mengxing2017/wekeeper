use crate::{
    business::{Actor, Error, Page, Result},
    distribution::domain::content::{OfficialSender, SenderQualificationInput},
};
use sqlx::{PgPool, Row};
use uuid::Uuid;
#[derive(Clone)]
pub struct Repository(pub PgPool);
impl Repository {
    pub async fn list(&self, actor: &Actor, page: Page) -> Result<Vec<OfficialSender>> {
        let mut tx = self.0.begin().await?;
        super::authorization::authorize(&mut tx, actor.id).await?;
        let (limit, offset) = page.fetch_bounds()?;
        let rows=sqlx::query("SELECT a.id,m.userid,m.name,m.verification='verified' AS member_verified,coalesce(m.status=1,false) AS member_active,a.active AS account_active,a.wecom_binding_revision,coalesce(q.revision,0) AS qualification_revision,coalesce(q.enabled,true) AS enabled FROM accounts a JOIN wecom_members m ON m.id=a.wecom_member_id LEFT JOIN wecom_sender_qualifications q ON q.account_id=a.id ORDER BY m.userid,a.id LIMIT $1 OFFSET $2").bind(limit).bind(offset).fetch_all(&mut *tx).await?;
        let out = rows
            .into_iter()
            .map(|r| {
                let verified: bool = r.get("member_verified");
                let active: bool = r.get("member_active");
                let account: bool = r.get("account_active");
                let enabled: bool = r.get("enabled");
                let eligible = verified && active && account && enabled;
                OfficialSender {
                    id: r.get("id"),

                    userid: r.get("userid"),
                    display_name: r.get("name"),
                    member_verified: verified,
                    member_active: active,
                    account_active: account,
                    binding_revision: r.get("wecom_binding_revision"),
                    qualification_revision: r.get("qualification_revision"),
                    enabled,
                    eligible,
                    reason: if !enabled {
                        Some("官方成员发送资格已停用".into())
                    } else if !eligible {
                        Some("官方成员或平台身份不可用，请核对通讯录和映射".into())
                    } else {
                        Some("成员基础资格满足；具体目标、目录时效和渠道权限仍在任务中核验".into())
                    },
                }
            })
            .collect();
        tx.commit().await?;
        Ok(out)
    }
    pub async fn qualify(
        &self,
        actor: &Actor,
        id: Uuid,
        input: SenderQualificationInput,
    ) -> Result<()> {
        actor.platform()?;
        let mut tx = self.0.begin().await?;
        super::broadcast::guard(&mut tx).await?;
        super::authorization::authorize(&mut tx, actor.id).await?;
        sqlx::query("SELECT a.id FROM accounts a JOIN wecom_members m ON m.id=a.wecom_member_id WHERE a.id=$1 FOR SHARE OF a,m").bind(id).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
        let updated=sqlx::query("INSERT INTO wecom_sender_qualifications(account_id,enabled,updated_by) SELECT $1,$2,$3  WHERE $4=0 ON CONFLICT(account_id) DO NOTHING").bind(id).bind(input.enabled).bind(actor.id).bind(input.revision).execute(&mut *tx).await?.rows_affected();
        if updated == 0 {
            let n=sqlx::query("UPDATE wecom_sender_qualifications SET enabled=$2,revision=revision+1,updated_by=$3,updated_at=now() WHERE account_id=$1 AND revision=$4 AND $4>0").bind(id).bind(input.enabled).bind(actor.id).bind(input.revision).execute(&mut *tx).await?.rows_affected();
            if n != 1 {
                return Err(Error::Conflict);
            }
        }
        crate::audit::record(
            &mut tx,
            actor,
            "wecom.sender.qualification",
            id,
            serde_json::json!({"domain":"wecom","enabled":input.enabled}),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
