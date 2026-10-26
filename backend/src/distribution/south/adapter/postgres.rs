use crate::business::{Actor, Error, Result, hash, require};
use crate::distribution::domain::blocks_resend;
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use std::collections::BTreeMap;
use uuid::Uuid;
#[derive(Clone)]
pub struct Repository(pub PgPool);
impl Repository {
    pub async fn claim(&self, a: &Actor, s: Uuid, id: Uuid) -> Result<()> {
        let mut tx = self.0.begin().await?;
        mobile_delivery::guard(&mut tx).await?;
        mobile_delivery::official_allowed(&mut tx, s).await?;
        sqlx::query("SELECT a.id FROM accounts a JOIN wecom_members m ON m.id=a.wecom_member_id WHERE a.id=$1 AND a.active AND a.wecom_binding_revision=$2 AND m.verification='verified' AND m.status=1 FOR SHARE OF a,m").bind(a.id).bind(a.wecom_binding_revision).fetch_optional(&mut *tx).await?.ok_or(Error::Forbidden)?;

        sqlx::query("SELECT g.id FROM delivery_attempts a JOIN delivery_drafts d ON d.id=a.draft_id JOIN wecom_groups g ON g.id=d.group_id WHERE a.school_id=$1 AND a.id=$2 AND a.actor_id=$3 FOR SHARE OF g").bind(s).bind(id).bind(a.id).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
        let draft:Uuid=sqlx::query_scalar("SELECT d.id FROM delivery_attempts a JOIN delivery_drafts d ON d.id=a.draft_id JOIN wecom_groups g ON g.id=d.group_id JOIN schools s ON s.id=d.school_id WHERE a.school_id=$1 AND a.id=$2 AND a.actor_id=$3 AND d.state='ready' AND g.active AND s.active FOR UPDATE OF a FOR SHARE OF d,g,s").bind(s).bind(id).bind(a.id).fetch_optional(&mut *tx).await?.ok_or(Error::Conflict)?;
        let posts=sqlx::query("SELECT p.revision,dp.post_revision,p.audit_status,p.visibility FROM delivery_draft_posts dp JOIN posts p ON p.id=dp.post_id WHERE dp.draft_id=$1 FOR SHARE OF p").bind(draft).fetch_all(&mut *tx).await?;
        require(
            !posts.is_empty()
                && posts.iter().all(|p| {
                    p.get::<i64, _>("revision") == p.get::<i64, _>("post_revision")
                        && p.get::<String, _>("audit_status") == "approved"
                        && p.get::<String, _>("visibility") == "visible"
                }),
            "草稿内容已变化，禁止调用发送",
        )?;
        let n=sqlx::query("UPDATE delivery_attempts SET client_invoked_at=now() WHERE school_id=$1 AND id=$2 AND actor_id=$3 AND state='prepared' AND client_invoked_at IS NULL AND created_at>now()-interval '2 minutes' AND wecom_binding_revision=$4").bind(s).bind(id).bind(a.id).bind(a.wecom_binding_revision).execute(&mut *tx).await?.rows_affected();
        if n != 1 {
            return Err(Error::Conflict);
        }
        mobile_delivery::project(&mut tx).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn templates(&self, s: Uuid) -> Result<Vec<Value>> {
        Ok(sqlx::query_scalar("SELECT to_jsonb(t)||jsonb_build_object('versions',coalesce((SELECT jsonb_agg(to_jsonb(v) ORDER BY v.created_at DESC) FROM template_versions v WHERE v.template_id=t.id),'[]'::jsonb)) FROM templates t WHERE school_id=$1 ORDER BY created_at DESC").bind(s).fetch_all(&self.0).await?)
    }
    pub async fn template(&self, s: Uuid, name: &str) -> Result<Value> {
        Ok(sqlx::query_scalar("INSERT INTO templates(id,school_id,name) VALUES($1,$2,$3) RETURNING to_jsonb(templates)").bind(Uuid::now_v7()).bind(s).bind(name).fetch_one(&self.0).await?)
    }
    pub async fn version(&self, s: Uuid, id: Uuid, body: &str) -> Result<Value> {
        sqlx::query_scalar("INSERT INTO template_versions(id,school_id,template_id,body) SELECT $1,$2,id,$4 FROM templates WHERE school_id=$2 AND id=$3 RETURNING to_jsonb(template_versions)").bind(Uuid::now_v7()).bind(s).bind(id).bind(body).fetch_optional(&self.0).await?.ok_or(Error::NotFound)
    }
    pub async fn publish(&self, a: &Actor, s: Uuid, id: Uuid, revision: i64) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        mobile_delivery::guard(&mut tx).await?;
        let item=sqlx::query_scalar("UPDATE template_versions SET published=true,revision=revision+1 WHERE school_id=$1 AND id=$2 AND revision=$3 AND NOT published RETURNING to_jsonb(template_versions)").bind(s).bind(id).bind(revision).fetch_optional(&mut *tx).await?.ok_or(Error::Conflict)?;
        crate::audit::record(&mut tx, a, s, "template.publish", id, json!({})).await?;
        mobile_delivery::project(&mut tx).await?;
        tx.commit().await?;
        Ok(item)
    }
    pub async fn drafts(&self, s: Uuid, limit: i64, offset: i64) -> Result<Vec<Value>> {
        Ok(sqlx::query_scalar("SELECT to_jsonb(d)||jsonb_build_object('group_name',g.name,'chat_id',g.chat_id) FROM delivery_drafts d JOIN wecom_groups g ON g.id=d.group_id WHERE d.school_id=$1 ORDER BY d.created_at DESC,d.id DESC LIMIT $2 OFFSET $3").bind(s).bind(limit).bind(offset).fetch_all(&self.0).await?)
    }
    pub async fn get_draft(&self, s: Uuid, id: Uuid) -> Result<Value> {
        sqlx::query_scalar("SELECT to_jsonb(d)||jsonb_build_object('group_name',g.name,'chat_id',g.chat_id) FROM delivery_drafts d JOIN wecom_groups g ON g.id=d.group_id WHERE d.school_id=$1 AND d.id=$2").bind(s).bind(id).fetch_optional(&self.0).await?.ok_or(Error::NotFound)
    }
    pub async fn related(
        &self,
        s: Uuid,
        id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<crate::distribution::domain::RelatedHistory> {
        self.get_draft(s, id).await?;
        sqlx::query("UPDATE delivery_attempts SET state='unknown',updated_at=now() WHERE school_id=$1 AND state='prepared' AND created_at<now()-interval '2 minutes'").bind(s).execute(&self.0).await?;
        let rows: Vec<Value> = sqlx::query_scalar("SELECT to_jsonb(a)||jsonb_build_object('group_name',g.name,'content',prior.content,'actor_name',u.display_name) FROM delivery_attempts a JOIN delivery_drafts prior ON prior.id=a.draft_id JOIN delivery_drafts current ON current.id=$1 JOIN wecom_groups g ON g.id=prior.group_id JOIN accounts u ON u.id=a.actor_id WHERE prior.school_id=current.school_id AND prior.group_id=current.group_id AND (prior.id=current.id OR EXISTS(SELECT 1 FROM delivery_draft_posts x JOIN delivery_draft_posts y ON y.post_id=x.post_id WHERE x.draft_id=prior.id AND y.draft_id=current.id)) ORDER BY a.created_at DESC,a.id DESC").bind(id).fetch_all(&self.0).await?;
        let latest_attempt_id = rows
            .first()
            .and_then(|r| r["id"].as_str())
            .and_then(|id| id.parse().ok());
        let blocked = rows.iter().any(blocks_resend);
        Ok(crate::distribution::domain::RelatedHistory {
            latest_attempt_id,
            blocked,
            items: rows
                .into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .collect(),
        })
    }
    pub async fn draft(
        &self,
        a: &Actor,
        s: Uuid,
        group: Uuid,
        version: Uuid,
        ids: Vec<Uuid>,
        origin: &str,
    ) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        mobile_delivery::guard(&mut tx).await?;
        let name: String =
            sqlx::query_scalar("SELECT name FROM schools WHERE id=$1 AND active FOR SHARE")
                .bind(s)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(Error::NotFound)?;
        sqlx::query(
            "SELECT id FROM wecom_groups WHERE school_id=$1 AND id=$2 AND active FOR SHARE",
        )
        .bind(s)
        .bind(group)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Error::NotFound)?;
        let body:String=sqlx::query_scalar("SELECT body FROM template_versions WHERE school_id=$1 AND id=$2 AND published FOR SHARE").bind(s).bind(version).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
        let mut snippets = Vec::new();
        let mut revisions = Vec::new();
        for id in &ids {
            let row=sqlx::query("SELECT title,content,revision FROM posts WHERE school_id=$1 AND id=$2 AND audit_status='approved' AND visibility='visible' FOR SHARE").bind(s).bind(id).fetch_optional(&mut *tx).await?.ok_or(Error::Conflict)?;
            let content: String = row.get("content");
            let values = BTreeMap::from([
                ("school.name", name.clone()),
                ("post.title", row.get("title")),
                ("post.summary", content.chars().take(200).collect()),
                (
                    "post.detail_url",
                    format!("{origin}/community/{s}/posts/{id}"),
                ),
            ]);
            snippets.push(crate::distribution::domain::render(&body, &values)?);
            revisions.push(row.get::<i64, _>("revision"));
        }
        let content = snippets.join("\n\n");
        require(
            content.len() <= 2000,
            "合并消息超过 2000 字节，请减少内容或缩短模板",
        )?;
        let id = Uuid::now_v7();
        let item=sqlx::query_scalar("INSERT INTO delivery_drafts(id,school_id,group_id,template_version_id,creator_id,content,content_hash) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING to_jsonb(delivery_drafts)").bind(id).bind(s).bind(group).bind(version).bind(a.id).bind(&content).bind(hash(&content)).fetch_one(&mut *tx).await?;
        for (post, rev) in ids.into_iter().zip(revisions) {
            sqlx::query("INSERT INTO delivery_draft_posts VALUES($1,$2,$3,$4)")
                .bind(id)
                .bind(s)
                .bind(post)
                .bind(rev)
                .execute(&mut *tx)
                .await?;
        }
        crate::audit::record(&mut tx, a, s, "draft.create", id, json!({})).await?;
        mobile_delivery::project(&mut tx).await?;
        tx.commit().await?;
        Ok(item)
    }
    pub async fn context(&self, s: Uuid, id: Uuid) -> Result<(String, String)> {
        let row=sqlx::query("SELECT g.corp_id,g.chat_id FROM delivery_drafts d JOIN wecom_groups g ON g.id=d.group_id WHERE d.school_id=$1 AND d.id=$2 AND g.active").bind(s).bind(id).fetch_optional(&self.0).await?.ok_or(Error::NotFound)?;
        Ok((row.get("corp_id"), row.get("chat_id")))
    }
    #[allow(clippy::too_many_arguments)] // Transaction inputs include the previous attempt evidence.
    pub async fn attempt(
        &self,
        a: &Actor,
        s: Uuid,
        draft: Uuid,
        key: &str,
        chat: &str,
        resend: Option<Uuid>,
        reason: &str,
    ) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        mobile_delivery::guard(&mut tx).await?;
        mobile_delivery::official_allowed(&mut tx, s).await?;
        sqlx::query("SELECT a.id FROM accounts a JOIN wecom_members m ON m.id=a.wecom_member_id WHERE a.id=$1 AND a.active AND a.wecom_binding_revision=$2 AND m.verification='verified' AND m.status=1 FOR SHARE OF a,m").bind(a.id).bind(a.wecom_binding_revision).fetch_optional(&mut *tx).await?.ok_or(Error::Forbidden)?;
        // A group lock serializes overlapping content across different drafts as well.
        sqlx::query("SELECT g.id FROM wecom_groups g JOIN delivery_drafts d ON d.group_id=g.id WHERE d.school_id=$1 AND d.id=$2 FOR UPDATE OF g").bind(s).bind(draft).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
        // Serialize every attempt for the same draft, including concurrent idempotency keys.
        let row=sqlx::query("SELECT d.state,g.chat_id,g.active FROM delivery_drafts d JOIN wecom_groups g ON g.id=d.group_id WHERE d.school_id=$1 AND d.id=$2 FOR UPDATE OF d,g").bind(s).bind(draft).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
        if let Some(existing) = sqlx::query_scalar::<_, Value>(
            "SELECT to_jsonb(a) FROM delivery_attempts a WHERE actor_id=$1 AND idempotency_key=$2",
        )
        .bind(a.id)
        .bind(key)
        .fetch_optional(&mut *tx)
        .await?
        {
            require(
                existing["draft_id"].as_str() == Some(draft.to_string().as_str()),
                "幂等键已被其他草稿使用",
            )?;
            return Ok(existing);
        }
        require(
            row.get::<String, _>("state") == "ready"
                && row.get::<bool, _>("active")
                && row.get::<String, _>("chat_id") == chat,
            "草稿或目标群已变化",
        )?;
        let posts=sqlx::query("SELECT p.revision,p.audit_status,p.visibility,dp.post_revision FROM delivery_draft_posts dp JOIN posts p ON p.id=dp.post_id AND p.school_id=dp.school_id WHERE dp.draft_id=$1 FOR SHARE OF p").bind(draft).fetch_all(&mut *tx).await?;
        require(
            !posts.is_empty()
                && posts.iter().all(|p| {
                    p.get::<i64, _>("revision") == p.get::<i64, _>("post_revision")
                        && p.get::<String, _>("audit_status") == "approved"
                        && p.get::<String, _>("visibility") == "visible"
                }),
            "内容已修改、撤审或隐藏，请重新生成草稿",
        )?;
        sqlx::query("UPDATE delivery_attempts SET state='unknown',updated_at=now() WHERE school_id=$1 AND state='prepared' AND created_at<now()-interval '2 minutes'").bind(s).execute(&mut *tx).await?;
        let previous:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(a) FROM delivery_attempts a JOIN delivery_drafts prior ON prior.id=a.draft_id JOIN delivery_drafts current ON current.id=$1 WHERE prior.school_id=current.school_id AND prior.group_id=current.group_id AND (prior.id=current.id OR EXISTS(SELECT 1 FROM delivery_draft_posts x JOIN delivery_draft_posts y ON y.post_id=x.post_id WHERE x.draft_id=prior.id AND y.draft_id=current.id)) ORDER BY a.created_at DESC,a.id DESC").bind(draft).fetch_all(&mut *tx).await?;
        require(
            !previous.iter().any(blocks_resend),
            "相关内容仍有执行中或未核对的尝试，请先人工核对",
        )?;
        if let Some(p) = previous.first() {
            require(
                resend.is_some_and(|id| p["id"].as_str() == Some(id.to_string().as_str()))
                    && !reason.trim().is_empty(),
                "再次分享须引用上次尝试并填写原因",
            )?;
        } else {
            require(resend.is_none(), "没有可重发的历史尝试")?;
        }
        let id = Uuid::now_v7();
        let item=sqlx::query_scalar("INSERT INTO delivery_attempts(id,school_id,draft_id,actor_id,idempotency_key,resend_of,resend_reason,wecom_binding_revision) VALUES($1,$2,$3,$4,$5,$6,$7,$8) RETURNING to_jsonb(delivery_attempts)").bind(id).bind(s).bind(draft).bind(a.id).bind(key).bind(resend).bind(reason).bind(a.wecom_binding_revision).fetch_one(&mut *tx).await?;
        crate::audit::record(
            &mut tx,
            a,
            s,
            "attempt.prepare",
            id,
            json!({"draft_id":draft}),
        )
        .await?;
        mobile_delivery::project(&mut tx).await?;
        tx.commit().await?;
        Ok(item)
    }
    pub async fn attempts(&self, s: Uuid, limit: i64, offset: i64) -> Result<Vec<Value>> {
        sqlx::query("UPDATE delivery_attempts SET state='unknown',updated_at=now() WHERE school_id=$1 AND state='prepared' AND created_at<now()-interval '2 minutes'").bind(s).execute(&self.0).await?;
        Ok(sqlx::query_scalar("SELECT to_jsonb(a)||jsonb_build_object('group_name',g.name,'content',d.content,'actor_name',u.display_name) FROM delivery_attempts a JOIN delivery_drafts d ON d.id=a.draft_id JOIN wecom_groups g ON g.id=d.group_id JOIN accounts u ON u.id=a.actor_id WHERE a.school_id=$1 ORDER BY a.created_at DESC,a.id DESC LIMIT $2 OFFSET $3").bind(s).bind(limit).bind(offset).fetch_all(&self.0).await?)
    }
    pub async fn result(&self, a: &Actor, s: Uuid, id: Uuid, v: Value) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        mobile_delivery::guard(&mut tx).await?;
        let old:Value=sqlx::query_scalar("SELECT to_jsonb(a) FROM delivery_attempts a WHERE school_id=$1 AND id=$2 AND actor_id=$3 FOR UPDATE").bind(s).bind(id).bind(a.id).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
        if !old["client_result"].is_null() {
            require(old["client_result"] == v, "已有不同的客户端结果")?;
            return Ok(old);
        }
        let state = v["state"].as_str().unwrap_or("");
        require(
            !old["client_invoked_at"].is_null(),
            "尚未领取发送资格，不能上报客户端结果",
        )?;
        require(
            matches!(old["state"].as_str(), Some("prepared" | "unknown")),
            "当前状态不能接收客户端结果",
        )?;
        let item=sqlx::query_scalar("UPDATE delivery_attempts SET state=$3,client_result=$4,updated_at=now() WHERE school_id=$1 AND id=$2 RETURNING to_jsonb(delivery_attempts)").bind(s).bind(id).bind(state).bind(&v).fetch_one(&mut *tx).await?;
        crate::audit::record(
            &mut tx,
            a,
            s,
            "attempt.client_result",
            id,
            json!({"state":state}),
        )
        .await?;
        mobile_delivery::project(&mut tx).await?;
        tx.commit().await?;
        Ok(item)
    }
    pub async fn reconcile(&self, a: &Actor, s: Uuid, id: Uuid, v: Value) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        mobile_delivery::guard(&mut tx).await?;
        let item=sqlx::query_scalar("UPDATE delivery_attempts SET reconciliation=$3,reconciliation_note=$4,reconciled_by=$5,reconciled_at=now() WHERE school_id=$1 AND id=$2 AND (state<>'prepared' OR created_at<now()-interval '2 minutes') RETURNING to_jsonb(delivery_attempts)").bind(s).bind(id).bind(v["decision"].as_str()).bind(v["reason"].as_str()).bind(a.id).fetch_optional(&mut *tx).await?.ok_or(Error::Conflict)?;
        crate::audit::record(&mut tx, a, s, "attempt.reconcile", id, v).await?;
        mobile_delivery::project(&mut tx).await?;
        tx.commit().await?;
        Ok(item)
    }
}

impl crate::distribution::south::port::Store for Repository {
    async fn claim(&self, a: &Actor, s: Uuid, id: Uuid) -> Result<()> {
        Repository::claim(self, a, s, id).await
    }
    async fn get_draft(&self, s: Uuid, id: Uuid) -> Result<Value> {
        Repository::get_draft(self, s, id).await
    }
    async fn related(
        &self,
        s: Uuid,
        id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<crate::distribution::domain::RelatedHistory> {
        Repository::related(self, s, id, limit, offset).await
    }
    async fn templates(&self, s: Uuid) -> Result<Vec<Value>> {
        Repository::templates(self, s).await
    }
    async fn template(&self, s: Uuid, name: &str) -> Result<Value> {
        Repository::template(self, s, name).await
    }
    async fn version(&self, s: Uuid, id: Uuid, body: &str) -> Result<Value> {
        Repository::version(self, s, id, body).await
    }
    async fn publish(&self, a: &Actor, s: Uuid, id: Uuid, revision: i64) -> Result<Value> {
        Repository::publish(self, a, s, id, revision).await
    }
    async fn drafts(&self, s: Uuid, limit: i64, offset: i64) -> Result<Vec<Value>> {
        Repository::drafts(self, s, limit, offset).await
    }
    async fn draft(
        &self,
        a: &Actor,
        s: Uuid,
        group: Uuid,
        version: Uuid,
        ids: Vec<Uuid>,
        origin: &str,
    ) -> Result<Value> {
        Repository::draft(self, a, s, group, version, ids, origin).await
    }
    async fn context(&self, s: Uuid, id: Uuid) -> Result<(String, String)> {
        Repository::context(self, s, id).await
    }
    async fn attempt(
        &self,
        a: &Actor,
        s: Uuid,
        draft: Uuid,
        key: &str,
        chat: &str,
        resend: Option<Uuid>,
        reason: &str,
    ) -> Result<Value> {
        Repository::attempt(self, a, s, draft, key, chat, resend, reason).await
    }
    async fn attempts(&self, s: Uuid, limit: i64, offset: i64) -> Result<Vec<Value>> {
        Repository::attempts(self, s, limit, offset).await
    }
    async fn result(&self, a: &Actor, s: Uuid, id: Uuid, v: Value) -> Result<Value> {
        Repository::result(self, a, s, id, v).await
    }
    async fn reconcile(&self, a: &Actor, s: Uuid, id: Uuid, v: Value) -> Result<Value> {
        Repository::reconcile(self, a, s, id, v).await
    }
}

pub mod broadcast;

pub mod mobile_delivery;
