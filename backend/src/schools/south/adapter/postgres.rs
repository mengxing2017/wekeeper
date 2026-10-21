use crate::business::{Actor, Error, Result};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
#[derive(Clone)]
pub struct Repository(pub PgPool);
impl Repository {
    pub async fn list(&self, a: &Actor) -> Result<Vec<Value>> {
        Ok(sqlx::query_scalar("SELECT to_jsonb(s) FROM schools s WHERE ($1 OR id IN (SELECT school_id FROM school_memberships WHERE account_id=$2 AND role<>'revoked')) ORDER BY name").bind(a.platform_admin).bind(a.id).fetch_all(&self.0).await?)
    }
    pub async fn directory(&self) -> Result<Vec<Value>> {
        Ok(sqlx::query_scalar("SELECT jsonb_build_object('id',id,'name',name) FROM schools WHERE active ORDER BY name").fetch_all(&self.0).await?)
    }
    pub async fn create(&self, name: &str) -> Result<Value> {
        let id = Uuid::now_v7();
        sqlx::query("INSERT INTO schools(id,name) VALUES($1,$2)")
            .bind(id)
            .bind(name)
            .execute(&self.0)
            .await?;
        Ok(json!({"id":id}))
    }
    pub async fn update(
        &self,
        a: &Actor,
        id: Uuid,
        name: &str,
        config: Value,
        revision: i64,
    ) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        let item=sqlx::query_scalar("UPDATE schools SET name=$2,config=$3,revision=revision+1 WHERE id=$1 AND revision=$4 RETURNING to_jsonb(schools)").bind(id).bind(name).bind(config).bind(revision).fetch_optional(&mut *tx).await?.ok_or(Error::Conflict)?;
        crate::audit::record(&mut tx, a, id, "school.update", id, json!({})).await?;
        tx.commit().await?;
        Ok(item)
    }
    pub async fn bind(&self, a: &Actor, school: Uuid) -> Result<()> {
        let mut tx = self.0.begin().await?;
        let current: Option<Uuid> =
            sqlx::query_scalar("SELECT bound_school_id FROM accounts WHERE id=$1 FOR UPDATE")
                .bind(a.id)
                .fetch_one(&mut *tx)
                .await?;
        if current.is_some_and(|s| s != school) {
            return Err(Error::Forbidden);
        }
        let active: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM schools WHERE id=$1 AND active)")
                .bind(school)
                .fetch_one(&mut *tx)
                .await?;
        if !active {
            return Err(Error::NotFound);
        }
        sqlx::query("UPDATE accounts SET bound_school_id=$2 WHERE id=$1")
            .bind(a.id)
            .bind(school)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO school_memberships VALUES($1,$2,'member') ON CONFLICT DO NOTHING")
            .bind(school)
            .bind(a.id)
            .execute(&mut *tx)
            .await?;
        crate::audit::record(&mut tx, a, school, "school.bind", a.id, json!({})).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn members(&self, school: Uuid) -> Result<Vec<Value>> {
        Ok(sqlx::query_scalar("SELECT jsonb_build_object('id',a.id,'username',a.username,'display_name',a.display_name,'role',m.role,'wecom_name',w.name,'wecom_status',w.status,'wecom_verification',w.verification,'wecom_departments',coalesce((SELECT jsonb_agg(d.name ORDER BY d.id) FROM wecom_departments d WHERE d.id=ANY(w.department)),'[]'::jsonb)) FROM school_memberships m JOIN accounts a ON a.id=m.account_id LEFT JOIN wecom_members w ON w.id=a.wecom_member_id WHERE school_id=$1 ORDER BY a.username").bind(school).fetch_all(&self.0).await?)
    }
    pub async fn grant(&self, a: &Actor, school: Uuid, id: Uuid, role: &str) -> Result<()> {
        let mut tx = self.0.begin().await?;
        if role == "revoked" {
            sqlx::query(
                "UPDATE school_memberships SET role='revoked' WHERE school_id=$1 AND account_id=$2",
            )
            .bind(school)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query("INSERT INTO school_memberships VALUES($1,$2,$3) ON CONFLICT(school_id,account_id) DO UPDATE SET role=EXCLUDED.role").bind(school).bind(id).bind(role).execute(&mut *tx).await?;
        }
        crate::audit::record(&mut tx, a, school, "school.grant", id, json!({"role":role})).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn resources(&self, school: Uuid, operator: bool) -> Result<Vec<Value>> {
        Ok(sqlx::query_scalar("SELECT to_jsonb(r) FROM school_resources r WHERE (school_id=$1 OR (school_id IS NULL AND NOT EXISTS(SELECT 1 FROM school_resources custom WHERE custom.school_id=$1 AND custom.kind=r.kind AND custom.active))) AND ($2 OR (active AND kind<>'blocked_word' AND (starts_at IS NULL OR starts_at<=now()) AND (ends_at IS NULL OR ends_at>now()))) ORDER BY sort_order DESC,id").bind(school).bind(operator).fetch_all(&self.0).await?)
    }
    pub async fn resource(&self, a: &Actor, school: Uuid, v: Value) -> Result<Value> {
        let id = v["id"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(Uuid::now_v7);
        let scope = if school.is_nil() { None } else { Some(school) };
        let revision = v["revision"].as_i64().unwrap_or(0);
        let mut tx = self.0.begin().await?;
        let item=sqlx::query_scalar("INSERT INTO school_resources(id,school_id,kind,title,content,url,active,sort_order,starts_at,ends_at) SELECT $1,$2,$3,$4,$5,$6,$7,$8,$9::text::timestamptz,$10::text::timestamptz WHERE $11=0 ON CONFLICT(id) DO NOTHING RETURNING to_jsonb(school_resources)").bind(id).bind(scope).bind(v["kind"].as_str()).bind(v["title"].as_str()).bind(v["content"].as_str().unwrap_or("")).bind(v["url"].as_str().unwrap_or("")).bind(v["active"].as_bool().unwrap_or(true)).bind(v["sort_order"].as_i64().unwrap_or(0) as i32).bind(v["starts_at"].as_str()).bind(v["ends_at"].as_str()).bind(revision).fetch_optional(&mut *tx).await?;
        let item = if let Some(item) = item {
            item
        } else {
            sqlx::query_scalar("UPDATE school_resources SET title=$3,content=$4,url=$5,active=$6,sort_order=$7,starts_at=$8::text::timestamptz,ends_at=$9::text::timestamptz,revision=revision+1 WHERE id=$1 AND school_id IS NOT DISTINCT FROM $2 AND revision=$10 AND kind=$11 RETURNING to_jsonb(school_resources)").bind(id).bind(scope).bind(v["title"].as_str()).bind(v["content"].as_str().unwrap_or("")).bind(v["url"].as_str().unwrap_or("")).bind(v["active"].as_bool().unwrap_or(true)).bind(v["sort_order"].as_i64().unwrap_or(0) as i32).bind(v["starts_at"].as_str()).bind(v["ends_at"].as_str()).bind(revision).bind(v["kind"].as_str()).fetch_optional(&mut *tx).await?.ok_or(Error::Conflict)?
        };
        crate::audit::record(
            &mut tx,
            a,
            school,
            "resource.save",
            id,
            json!({"kind":v["kind"]}),
        )
        .await?;
        tx.commit().await?;
        Ok(item)
    }
    pub async fn click(&self, school: Uuid, id: Uuid) -> Result<()> {
        sqlx::query("UPDATE school_resources SET clicks=clicks+1 WHERE school_id=$1 AND id=$2 AND kind='banner' AND active").bind(school).bind(id).execute(&self.0).await?;
        Ok(())
    }
    pub async fn groups(&self, school: Uuid) -> Result<Vec<Value>> {
        Ok(sqlx::query_scalar(
            "SELECT to_jsonb(g) FROM wecom_groups g WHERE school_id=$1 ORDER BY name",
        )
        .bind(school)
        .fetch_all(&self.0)
        .await?)
    }
    pub async fn group(&self, a: &Actor, school: Uuid, v: Value) -> Result<Value> {
        let id = v["id"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(Uuid::now_v7);
        let rev = v["revision"].as_i64().unwrap_or(0);
        let mut tx = self.0.begin().await?;
        let item = if rev == 0 {
            sqlx::query_scalar("INSERT INTO wecom_groups(id,school_id,corp_id,chat_id,name,active) VALUES($1,$2,$3,$4,$5,$6) RETURNING to_jsonb(wecom_groups)").bind(id).bind(school).bind(v["corp_id"].as_str()).bind(v["chat_id"].as_str()).bind(v["name"].as_str()).bind(v["active"].as_bool().unwrap_or(true)).fetch_one(&mut *tx).await?
        } else {
            sqlx::query_scalar("UPDATE wecom_groups SET name=$3,active=$4,revision=revision+1 WHERE id=$1 AND school_id=$2 AND revision=$5 AND corp_id=$6 AND chat_id=$7 RETURNING to_jsonb(wecom_groups)").bind(id).bind(school).bind(v["name"].as_str()).bind(v["active"].as_bool().unwrap_or(true)).bind(rev).bind(v["corp_id"].as_str()).bind(v["chat_id"].as_str()).fetch_optional(&mut *tx).await?.ok_or(Error::Conflict)?
        };
        crate::audit::record(&mut tx, a, school, "group.save", id, json!({})).await?;
        tx.commit().await?;
        Ok(item)
    }
}

impl crate::schools::south::port::Store for Repository {
    async fn list(&self, a: &Actor) -> Result<Vec<Value>> {
        Repository::list(self, a).await
    }
    async fn directory(&self) -> Result<Vec<Value>> {
        Repository::directory(self).await
    }
    async fn create(&self, name: &str) -> Result<Value> {
        Repository::create(self, name).await
    }
    async fn update(
        &self,
        a: &Actor,
        id: Uuid,
        name: &str,
        config: Value,
        revision: i64,
    ) -> Result<Value> {
        Repository::update(self, a, id, name, config, revision).await
    }
    async fn bind(&self, a: &Actor, school: Uuid) -> Result<()> {
        Repository::bind(self, a, school).await
    }
    async fn members(&self, school: Uuid) -> Result<Vec<Value>> {
        Repository::members(self, school).await
    }
    async fn grant(&self, a: &Actor, school: Uuid, id: Uuid, role: &str) -> Result<()> {
        Repository::grant(self, a, school, id, role).await
    }
    async fn resources(&self, school: Uuid, operator: bool) -> Result<Vec<Value>> {
        Repository::resources(self, school, operator).await
    }
    async fn resource(&self, a: &Actor, school: Uuid, v: Value) -> Result<Value> {
        Repository::resource(self, a, school, v).await
    }
    async fn click(&self, school: Uuid, id: Uuid) -> Result<()> {
        Repository::click(self, school, id).await
    }
    async fn groups(&self, school: Uuid) -> Result<Vec<Value>> {
        Repository::groups(self, school).await
    }
    async fn group(&self, a: &Actor, school: Uuid, v: Value) -> Result<Value> {
        Repository::group(self, a, school, v).await
    }
}
