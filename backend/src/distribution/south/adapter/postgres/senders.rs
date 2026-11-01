use crate::{
    business::{Actor, Error, Page, Result},
    distribution::domain::content::{OfficialSender, SenderQualificationInput},
};
use sqlx::{PgPool, Row};
use uuid::Uuid;
#[derive(Clone)]
pub struct Repository(pub PgPool);
impl Repository {
    pub async fn list(
        &self,
        actor: &Actor,
        school: Option<Uuid>,
        page: Page,
    ) -> Result<Vec<OfficialSender>> {
        let mut tx = self.0.begin().await?;
        if let Some(s) = school {
            super::broadcast::authorize(&mut tx, actor.id, s, false).await?;
        } else {
            actor.platform()?;
        }
        let (limit, offset) = page.fetch_bounds()?;
        let rows=sqlx::query("SELECT a.id,sm.school_id,m.userid,m.name,m.verification='verified' AS member_verified,coalesce(m.status=1,false) AS member_active,a.active AS account_active,a.wecom_binding_revision,coalesce(q.revision,0) AS qualification_revision,coalesce(q.enabled,true) AS enabled FROM accounts a JOIN wecom_members m ON m.id=a.wecom_member_id JOIN school_memberships sm ON sm.account_id=a.id JOIN schools s ON s.id=sm.school_id LEFT JOIN wecom_sender_qualifications q ON q.school_id=sm.school_id AND q.account_id=a.id WHERE ($1::uuid IS NULL OR sm.school_id=$1) AND s.active AND sm.role IN ('operator','admin') ORDER BY sm.school_id,m.userid,a.id LIMIT $2 OFFSET $3")
            .bind(school).bind(limit).bind(offset).fetch_all(&mut *tx).await?;
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
                    school_id: r.get("school_id"),
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
                        Some("学校发送资格已停用".into())
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
        school: Uuid,
        id: Uuid,
        input: SenderQualificationInput,
    ) -> Result<()> {
        actor.platform()?;
        let mut tx = self.0.begin().await?;
        super::broadcast::guard(&mut tx).await?;
        sqlx::query("SELECT a.id FROM accounts a JOIN wecom_members m ON m.id=a.wecom_member_id JOIN school_memberships s ON s.account_id=a.id WHERE a.id=$1 AND s.school_id=$2 AND s.role IN ('admin','operator') FOR SHARE OF a,m,s").bind(id).bind(school).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
        let updated=sqlx::query("INSERT INTO wecom_sender_qualifications(school_id,account_id,enabled,updated_by) SELECT $1,$2,$3,$4 WHERE $5=0 ON CONFLICT(school_id,account_id) DO NOTHING")
            .bind(school).bind(id).bind(input.enabled).bind(actor.id).bind(input.revision).execute(&mut *tx).await?.rows_affected();
        if updated == 0 {
            let n=sqlx::query("UPDATE wecom_sender_qualifications SET enabled=$3,revision=revision+1,updated_by=$4,updated_at=now() WHERE school_id=$1 AND account_id=$2 AND revision=$5 AND $5>0")
                .bind(school).bind(id).bind(input.enabled).bind(actor.id).bind(input.revision).execute(&mut *tx).await?.rows_affected();
            if n != 1 {
                return Err(Error::Conflict);
            }
        }
        crate::audit::record(
            &mut tx,
            actor,
            school,
            "wecom.sender.qualification",
            id,
            serde_json::json!({"domain":"wecom","enabled":input.enabled}),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
