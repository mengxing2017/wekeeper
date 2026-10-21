use crate::{
    business::{Actor, Error, Result},
    integrations::domain::{assets::*, directory::Job},
};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;
#[derive(Clone)]
pub struct Repository(pub PgPool);
async fn generation(tx: &mut Transaction<'_, Postgres>, g: i64) -> Result<()> {
    sqlx::query(
        "SELECT singleton FROM wecom_directory_state WHERE credential_generation=$1 FOR UPDATE",
    )
    .bind(g)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(Error::Conflict)?;
    Ok(())
}
async fn fence(tx: &mut Transaction<'_, Postgres>, j: &AssetJob) -> Result<()> {
    generation(tx, j.fence.generation).await?;
    sqlx::query("SELECT id FROM wecom_asset_sync_runs WHERE id=$1 AND lease_id=$2 AND generation=$3 AND state='running' AND lease_until>now() FOR UPDATE").bind(j.fence.id).bind(j.fence.lease).bind(j.fence.generation).fetch_optional(&mut **tx).await?.ok_or(Error::Conflict)?;
    Ok(())
}
impl Repository {
    pub async fn enqueue(
        &self,
        actor: Option<Uuid>,
        g: i64,
        dataset: &str,
        interval: i64,
    ) -> Result<Vec<Value>> {
        let mut tx = self.0.begin().await?;
        generation(&mut tx, g).await?;
        let mut result = vec![];
        for d in ["groups"] {
            if dataset != d {
                continue;
            }
            if let Some(v)=sqlx::query_scalar("SELECT to_jsonb(r)-'lease_id'-'lease_until'-'scope' FROM wecom_asset_sync_runs r WHERE dataset=$1 AND state IN ('queued','running')").bind(d).fetch_optional(&mut *tx).await? { result.push(v); continue; }
            if actor.is_none() {
                let due:bool=sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM wecom_asset_sync_runs WHERE dataset=$1 AND created_at>now()-$2*interval '1 second')").bind(d).bind(interval).fetch_one(&mut *tx).await?;
                if !due {
                    continue;
                }
            }
            let id = Uuid::now_v7();
            let v=sqlx::query_scalar("INSERT INTO wecom_asset_sync_runs(id,dataset,actor_id,generation) VALUES($1,$2,$3,$4) RETURNING to_jsonb(wecom_asset_sync_runs)-'lease_id'-'lease_until'-'scope'").bind(id).bind(d).bind(actor).bind(g).fetch_one(&mut *tx).await?;
            result.push(v);
            sqlx::query("INSERT INTO audit_events(id,actor_id,action,object_id) VALUES($1,$2,'wecom.assets.sync',$3)").bind(Uuid::now_v7()).bind(actor).bind(id).execute(&mut *tx).await?;
        }
        sqlx::query("DELETE FROM wecom_asset_sync_runs r WHERE r.state IN ('failed','succeeded') AND r.finished_at<now()-interval '30 days' AND NOT EXISTS(SELECT 1 FROM wecom_asset_sync_items i WHERE i.run_id=r.id) AND NOT EXISTS(SELECT 1 FROM wecom_asset_sync_state s WHERE s.current_run_id=r.id)").execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn status(&self) -> Result<Value> {
        Ok(sqlx::query_scalar("SELECT jsonb_build_object('credential_generation',(SELECT credential_generation FROM wecom_directory_state),'datasets',(SELECT jsonb_agg(to_jsonb(s)-'scope'||jsonb_build_object('freshness',CASE WHEN last_success_at<now()-interval '24 hours' THEN 'stale' ELSE freshness END)||jsonb_build_object('scope_count',jsonb_array_length(scope),'latest_run',(SELECT to_jsonb(r)-'lease_id'-'lease_until'-'scope' FROM wecom_asset_sync_runs r WHERE r.dataset=s.dataset ORDER BY created_at DESC,id DESC LIMIT 1)) ORDER BY dataset) FROM wecom_asset_sync_state s))").fetch_one(&self.0).await?)
    }
    pub async fn claim(&self, g: i64) -> Result<Option<AssetJob>> {
        let mut tx = self.0.begin().await?;
        generation(&mut tx, g).await?;
        let lease = Uuid::new_v4();
        let row=sqlx::query("UPDATE wecom_asset_sync_runs SET state='running',stage='collect',lease_id=$1,lease_until=now()+interval '2 minutes',started_at=now(),processed=0,pages=0,total=0,complete=false,error_code=NULL,failed_owner=NULL WHERE id=(SELECT id FROM wecom_asset_sync_runs WHERE generation=$2 AND (state='queued' OR (state='running' AND lease_until<now())) ORDER BY created_at,id FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING id,dataset,started_at").bind(lease).bind(g).fetch_optional(&mut *tx).await?;
        let j = row.map(|r| AssetJob {
            dataset: r.get("dataset"),
            fence: Job {
                id: r.get("id"),
                generation: g,
                lease,
                started_at: r.get("started_at"),
            },
        });
        if let Some(j) = &j {
            sqlx::query("DELETE FROM wecom_asset_sync_items WHERE run_id=$1")
                .bind(j.fence.id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(j)
    }
    pub async fn renew(&self, j: &AssetJob) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        fence(&mut tx, j).await?;
        let v=sqlx::query_scalar("UPDATE wecom_asset_sync_runs SET lease_until=now()+interval '2 minutes' WHERE id=$1 RETURNING jsonb_build_object('stage',stage,'processed',processed,'total',total,'pages',pages)").bind(j.fence.id).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(v)
    }
    pub async fn stage(&self, j: &AssetJob, kind: &str, key: &str, data: Value) -> Result<()> {
        let mut tx = self.0.begin().await?;
        fence(&mut tx, j).await?;
        let old: Option<Value> = sqlx::query_scalar(
            "SELECT data FROM wecom_asset_sync_items WHERE run_id=$1 AND kind=$2 AND key=$3",
        )
        .bind(j.fence.id)
        .bind(kind)
        .bind(key)
        .fetch_optional(&mut *tx)
        .await?;
        if old.as_ref().is_some_and(|v| v != &data) {
            return Err(Error::Invalid("inconsistent_snapshot".into()));
        }
        sqlx::query("INSERT INTO wecom_asset_sync_items(run_id,kind,key,data) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING").bind(j.fence.id).bind(kind).bind(key).bind(data).execute(&mut *tx).await?;
        sqlx::query("UPDATE wecom_asset_sync_runs SET stage=$2,processed=processed+1 WHERE id=$1")
            .bind(j.fence.id)
            .bind(kind)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn page(&self, j: &AssetJob) -> Result<()> {
        let mut tx = self.0.begin().await?;
        fence(&mut tx, j).await?;
        sqlx::query("UPDATE wecom_asset_sync_runs SET pages=pages+1 WHERE id=$1")
            .bind(j.fence.id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn items(
        &self,
        j: &AssetJob,
        kind: &str,
        after: &str,
    ) -> Result<Vec<(String, Value)>> {
        Ok(sqlx::query_as("SELECT key,data FROM wecom_asset_sync_items WHERE run_id=$1 AND kind=$2 AND key>$3 ORDER BY key LIMIT 100").bind(j.fence.id).bind(kind).bind(after).fetch_all(&self.0).await?)
    }
    pub async fn scope(&self, j: &AssetJob, scope: &[String]) -> Result<()> {
        let mut tx = self.0.begin().await?;
        fence(&mut tx, j).await?;
        sqlx::query(
            "UPDATE wecom_asset_sync_runs SET scope=$2,total=jsonb_array_length($2) WHERE id=$1",
        )
        .bind(j.fence.id)
        .bind(json!(scope))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn owners(&self, g: i64) -> Result<Vec<String>> {
        let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM wecom_directory_state s JOIN wecom_directory_sync_runs r ON r.id=s.snapshot_id WHERE r.state='succeeded' AND r.generation=$1 AND s.credential_generation=$1)").bind(g).fetch_one(&self.0).await?;
        if !valid {
            return Err(Error::Invalid("directory_snapshot_required".into()));
        }
        Ok(sqlx::query_scalar("SELECT DISTINCT ON (lower(userid COLLATE \"C\")) userid FROM (SELECT userid FROM wecom_members m WHERE verification='verified' AND (status=1 OR NOT EXISTS(SELECT 1 FROM wecom_asset_owner_exclusions e WHERE e.owner_key=m.userid_key AND e.generation=(SELECT credential_generation FROM wecom_directory_state))) UNION ALL SELECT data->>'owner' FROM wecom_asset_objects o WHERE kind='groups' AND NOT EXISTS(SELECT 1 FROM wecom_asset_owner_exclusions e WHERE e.owner_key=o.member_key AND e.generation=(SELECT credential_generation FROM wecom_directory_state))) t WHERE userid IS NOT NULL ORDER BY lower(userid COLLATE \"C\"),userid").fetch_all(&self.0).await?)
    }
    pub async fn configured_groups(&self) -> Result<Vec<String>> {
        Ok(
            sqlx::query_scalar("SELECT chat_id FROM wecom_groups wg WHERE NOT EXISTS(SELECT 1 FROM wecom_asset_objects g JOIN wecom_asset_owner_exclusions e ON e.owner_key=g.member_key WHERE g.kind='groups' AND g.key=wg.chat_id AND e.generation=(SELECT credential_generation FROM wecom_directory_state)) ORDER BY chat_id")
                .fetch_all(&self.0)
                .await?,
        )
    }
    pub async fn contains(&self, j: &AssetJob, kind: &str, key: &str) -> Result<bool> {
        Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM wecom_asset_sync_items WHERE run_id=$1 AND kind=$2 AND key=$3)").bind(j.fence.id).bind(kind).bind(key).fetch_one(&self.0).await?)
    }
    pub async fn complete(&self, j: &AssetJob) -> Result<()> {
        let mut tx = self.0.begin().await?;
        fence(&mut tx, j).await?;
        sqlx::query("UPDATE wecom_asset_sync_runs SET complete=true,stage='publish' WHERE id=$1")
            .bind(j.fence.id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn finish(&self, j: &AssetJob) -> Result<()> {
        let mut tx = self.0.begin().await?;
        fence(&mut tx, j).await?;
        let complete: bool =
            sqlx::query_scalar("SELECT complete FROM wecom_asset_sync_runs WHERE id=$1")
                .bind(j.fence.id)
                .fetch_one(&mut *tx)
                .await?;
        if !complete {
            return Err(Error::Conflict);
        }
        let kinds = vec!["groups", "members"];
        // An object verified after scan start wins together with its complete member relation set.
        sqlx::query("INSERT INTO wecom_asset_objects(id,kind,corp_id,key,parent_key,member_key,member_type,data,last_seen_run_id,checked_at,credential_generation,visibility,verification,source) SELECT gen_random_uuid(),i.kind,s.corp_id,i.key,i.data->>'parent_key',i.data->>'member_key',(i.data->>'member_type')::int,i.data,$1,$2,$3,'visible','verified',CASE WHEN i.data->>'point_verification'='true' THEN 'point' ELSE 'snapshot' END FROM wecom_asset_sync_items i CROSS JOIN wecom_directory_state s WHERE i.run_id=$1 AND i.kind=ANY($4) AND (i.kind<>'members' OR NOT EXISTS(SELECT 1 FROM wecom_asset_objects p WHERE p.kind='groups' AND p.key=i.data->>'parent_key' AND p.checked_at>$2)) ON CONFLICT(corp_id,kind,key) DO UPDATE SET parent_key=excluded.parent_key,member_key=excluded.member_key,member_type=excluded.member_type,data=excluded.data,last_seen_run_id=excluded.last_seen_run_id,checked_at=excluded.checked_at,credential_generation=excluded.credential_generation,visibility='visible',verification='verified',source=excluded.source WHERE wecom_asset_objects.checked_at<=excluded.checked_at")
   .bind(j.fence.id).bind(j.fence.started_at).bind(j.fence.generation).bind(&kinds).execute(&mut *tx).await?;
        sqlx::query("UPDATE wecom_asset_objects o SET visibility='not_seen',verification='unverified' WHERE kind=ANY($1) AND last_seen_run_id IS DISTINCT FROM $2 AND checked_at<=$3 AND (kind<>'members' OR NOT EXISTS(SELECT 1 FROM wecom_asset_objects p WHERE p.kind='groups' AND p.key=o.parent_key AND p.checked_at>$3))").bind(&kinds).bind(j.fence.id).bind(j.fence.started_at).execute(&mut *tx).await?;
        sqlx::query("UPDATE wecom_asset_sync_state s SET scope_changed=NOT (r.scope @> s.scope),scope=r.scope,current_run_id=r.id,last_success_at=now(),published_generation=r.generation,freshness='verified' FROM wecom_asset_sync_runs r WHERE r.id=$1 AND s.dataset=r.dataset").bind(j.fence.id).execute(&mut *tx).await?;
        sqlx::query("UPDATE wecom_asset_sync_runs r SET state='succeeded',stage='published',complete=true,finished_at=now(),lease_until=NULL,scope_changed=s.scope_changed FROM wecom_asset_sync_state s WHERE r.id=$1 AND s.dataset=r.dataset").bind(j.fence.id).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM wecom_asset_sync_items i USING wecom_asset_sync_runs r WHERE i.run_id=r.id AND r.state IN ('failed','succeeded') AND r.id NOT IN (SELECT id FROM wecom_asset_sync_runs WHERE dataset=r.dataset AND state='succeeded' ORDER BY finished_at DESC LIMIT 2)").execute(&mut *tx).await?;
        sqlx::query("DELETE FROM wecom_asset_sync_runs r WHERE state IN ('failed','succeeded') AND finished_at<now()-interval '30 days' AND NOT EXISTS(SELECT 1 FROM wecom_asset_sync_items i WHERE i.run_id=r.id) AND NOT EXISTS(SELECT 1 FROM wecom_asset_sync_state s WHERE s.current_run_id=r.id)").execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn fail(&self, j: &AssetJob, code: &str) -> Result<()> {
        let mut tx = self.0.begin().await?;
        fence(&mut tx, j).await?;
        sqlx::query("UPDATE wecom_asset_sync_runs SET state='failed',finished_at=now(),lease_until=NULL,error_code=$2 WHERE id=$1").bind(j.fence.id).bind(code).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM wecom_asset_sync_items WHERE run_id=$1")
            .bind(j.fence.id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn list(
        &self,
        kind: &str,
        q: &AssetQuery,
        parent: Option<Uuid>,
        id: Option<Uuid>,
    ) -> Result<Vec<Value>> {
        let (limit, offset) = q.pagination().fetch_bounds()?;
        if kind == "sync-runs" {
            return Ok(sqlx::query_scalar("SELECT to_jsonb(r)-'lease_id'-'lease_until'-'scope'||jsonb_build_object('scope_count',jsonb_array_length(scope)) FROM wecom_asset_sync_runs r WHERE ($1::uuid IS NULL OR id=$1) AND ($2::text IS NULL OR dataset=$2) ORDER BY created_at DESC,id DESC LIMIT $3 OFFSET $4").bind(id).bind(&q.dataset).bind(limit).bind(offset).fetch_all(&self.0).await?);
        }
        if kind == "member-statistics" {
            return self.statistics(q).await;
        }
        Ok(sqlx::query_scalar("SELECT (o.data-'member_key'-'parent_key'-'type')||jsonb_build_object('id',o.id,'corp_id',o.corp_id,'checked_at',o.checked_at,'credential_generation',o.credential_generation,'visibility',o.visibility,'verification',CASE WHEN o.credential_generation=s.credential_generation AND o.checked_at>now()-interval '24 hours' THEN o.verification ELSE 'unverified' END,'source',o.source,'member_id',m.id,'member_name',m.name,'bound_account_id',a.id,'school_id',wg.school_id) FROM wecom_asset_objects o CROSS JOIN wecom_directory_state s LEFT JOIN wecom_members m ON m.userid_key=o.member_key AND (o.member_type IS NULL OR o.member_type=1) LEFT JOIN accounts a ON a.wecom_member_id=m.id LEFT JOIN wecom_groups wg ON wg.corp_id=o.corp_id AND wg.chat_id=o.key AND o.kind='groups' WHERE o.kind=$1 AND ($2::uuid IS NULL OR o.id=$2) AND ($3::uuid IS NULL OR o.parent_key=(SELECT key FROM wecom_asset_objects WHERE id=$3 AND kind='groups')) AND ($4::text IS NULL OR strpos(lower(coalesce(o.data->>'name','')||' '||o.key),lower($4))>0) AND ($5::text IS NULL OR CASE WHEN o.credential_generation=s.credential_generation AND o.checked_at>now()-interval '24 hours' THEN o.verification ELSE 'unverified' END=$5) AND ($6::uuid IS NULL OR m.id=$6) AND ($7::uuid IS NULL OR EXISTS(SELECT 1 FROM wecom_asset_objects gm JOIN wecom_members mm ON mm.userid_key=gm.member_key WHERE gm.kind='members' AND gm.member_type=1 AND gm.parent_key=o.key AND mm.id=$7 AND gm.verification='verified' AND gm.credential_generation=s.credential_generation AND gm.checked_at>now()-interval '24 hours')) AND ($8::bool IS NULL OR (wg.id IS NOT NULL)=$8) ORDER BY o.key,o.id LIMIT $9 OFFSET $10")
   .bind(kind).bind(id).bind(parent).bind(&q.search).bind(&q.verification).bind(q.owner_member_id).bind(q.participant_member_id).bind(q.associated).bind(limit).bind(offset).fetch_all(&self.0).await?)
    }
    async fn statistics(&self, q: &AssetQuery) -> Result<Vec<Value>> {
        let (limit, offset) = q.pagination().fetch_bounds()?;
        Ok(sqlx::query_scalar("SELECT jsonb_build_object('id',m.id,'userid',m.userid,'name',m.name,'owned_group_count',(SELECT count(*) FROM wecom_asset_objects g WHERE g.kind='groups' AND g.member_key=m.userid_key AND g.verification='verified' AND g.visibility='visible' AND g.credential_generation=s.credential_generation AND g.checked_at>now()-interval '24 hours'),'participating_group_count',(SELECT count(*) FROM wecom_asset_objects g WHERE g.kind='members' AND g.member_type=1 AND g.member_key=m.userid_key AND g.verification='verified' AND g.visibility='visible' AND g.credential_generation=s.credential_generation AND g.checked_at>now()-interval '24 hours'),'snapshots',(SELECT jsonb_agg(to_jsonb(st)-'scope') FROM wecom_asset_sync_state st)) FROM wecom_members m CROSS JOIN wecom_directory_state s WHERE ($1::uuid IS NULL OR m.id=$1) AND ($2::text IS NULL OR strpos(lower(m.name||' '||m.userid),lower($2))>0) ORDER BY m.userid_key LIMIT $3 OFFSET $4").bind(q.member_id).bind(&q.search).bind(limit).bind(offset).fetch_all(&self.0).await?)
    }
    pub async fn candidates(
        &self,
        a: &Actor,
        school: Uuid,
        q: &AssetQuery,
        g: i64,
    ) -> Result<Vec<Value>> {
        let (limit, offset) = q.pagination().fetch_bounds()?;
        Ok(sqlx::query_scalar("SELECT jsonb_build_object('id',o.id,'name',o.data->>'name','chat_id',o.key,'corp_id',o.corp_id,'owner',o.data->>'owner','owner_name',m.name,'member_count',o.data->'member_count','already_associated',EXISTS(SELECT 1 FROM wecom_groups wg WHERE wg.school_id=$2 AND wg.corp_id=o.corp_id AND wg.chat_id=o.key),'credential_generation',o.credential_generation,'checked_at',o.checked_at) FROM wecom_asset_objects o LEFT JOIN wecom_members m ON m.userid_key=o.member_key WHERE o.kind='groups' AND o.verification='verified' AND o.visibility='visible' AND o.credential_generation=$1 AND o.checked_at>now()-interval '24 hours' AND EXISTS(SELECT 1 FROM schools WHERE id=$2 AND active) AND NOT EXISTS(SELECT 1 FROM wecom_groups g WHERE g.corp_id=o.corp_id AND g.chat_id=o.key AND g.school_id<>$2) AND ($3 OR EXISTS(SELECT 1 FROM accounts a WHERE a.id=$4 AND a.active AND a.wecom_member_id=m.id AND m.status=1 AND m.verification='verified')) AND ($5::text IS NULL OR strpos(lower(coalesce(o.data->>'name','')||' '||o.key),lower($5))>0) ORDER BY o.key LIMIT $6 OFFSET $7").bind(g).bind(school).bind(a.platform_admin).bind(a.id).bind(&q.search).bind(limit).bind(offset).fetch_all(&self.0).await?)
    }
    pub async fn bind_group(
        &self,
        a: &Actor,
        school: Uuid,
        input: &GroupBinding,
        group: &Group,
    ) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        generation(&mut tx, input.expected_generation).await?;
        let row=sqlx::query("SELECT key,corp_id FROM wecom_asset_objects WHERE id=$1 AND kind='groups' AND credential_generation=$2 AND checked_at=$3 AND visibility='visible' AND verification='verified' FOR UPDATE").bind(input.directory_group_id).bind(input.expected_generation).bind(input.expected_checked_at).fetch_optional(&mut *tx).await?.ok_or(Error::Conflict)?;
        let key: String = row.get("key");
        if key != group.chat_id {
            return Err(Error::Conflict);
        }
        sqlx::query("SELECT id FROM schools WHERE id=$1 AND active FOR SHARE")
            .bind(school)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(Error::NotFound)?;
        sqlx::query("SELECT id FROM accounts WHERE id=$1 FOR SHARE")
            .bind(a.id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(Error::Forbidden)?;
        sqlx::query("SELECT school_id FROM school_memberships WHERE school_id=$1 AND account_id=$2 FOR SHARE").bind(school).bind(a.id).fetch_optional(&mut *tx).await?;
        let allowed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM accounts a LEFT JOIN wecom_members m ON m.id=a.wecom_member_id WHERE a.id=$1 AND a.active AND (a.platform_admin OR (a.wecom_binding_revision=$2 AND m.userid_key=lower($3 COLLATE \"C\") AND m.verification='verified' AND m.status=1 AND EXISTS(SELECT 1 FROM school_memberships r WHERE r.account_id=a.id AND r.school_id=$4 AND r.role='admin'))))").bind(a.id).bind(a.wecom_binding_revision).bind(&group.owner).bind(school).fetch_one(&mut *tx).await?;
        if !allowed {
            return Err(Error::Forbidden);
        }
        Self::observe_tx(
            &mut tx,
            input.expected_generation,
            group,
            chrono::Utc::now(),
        )
        .await?;
        let name = crate::business::text(
            input
                .name
                .as_deref()
                .or(group.name.as_deref())
                .unwrap_or(&key),
            1,
            200,
        )?;
        let v=sqlx::query_scalar("INSERT INTO wecom_groups(id,school_id,corp_id,chat_id,name,directory_group_id) VALUES($1,$2,$3,$4,$5,$6) RETURNING to_jsonb(wecom_groups)").bind(Uuid::now_v7()).bind(school).bind(row.get::<String,_>("corp_id")).bind(&key).bind(name).bind(input.directory_group_id).fetch_one(&mut *tx).await?;
        sqlx::query("INSERT INTO audit_events(id,actor_id,action,object_id,detail) VALUES($1,$2,'wecom.group.bind',$3,$4)").bind(Uuid::now_v7()).bind(a.id).bind(input.directory_group_id).bind(json!({"school_id":school})).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(v)
    }
}
impl Repository {
    pub async fn candidate(&self, a: &Actor, school: Uuid, id: Uuid, g: i64) -> Result<Value> {
        sqlx::query_scalar("SELECT jsonb_build_object('id',o.id,'chat_id',o.key,'checked_at',o.checked_at) FROM wecom_asset_objects o WHERE o.id=$1 AND o.kind='groups' AND o.verification='verified' AND o.visibility='visible' AND o.credential_generation=$2 AND o.checked_at>now()-interval '24 hours' AND NOT EXISTS(SELECT 1 FROM wecom_groups wg WHERE wg.corp_id=o.corp_id AND wg.chat_id=o.key AND wg.school_id<>$3) AND ($4 OR EXISTS(SELECT 1 FROM accounts a JOIN wecom_members m ON a.wecom_member_id=m.id WHERE a.id=$5 AND a.active AND m.userid_key=o.member_key AND m.status=1 AND m.verification='verified'))").bind(id).bind(g).bind(school).bind(a.platform_admin).bind(a.id).fetch_optional(&self.0).await?.ok_or(Error::Forbidden)
    }
}
impl Repository {
    pub async fn observe(
        &self,
        generation_id: i64,
        group: &Group,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<Uuid> {
        let mut tx = self.0.begin().await?;
        generation(&mut tx, generation_id).await?;
        let id = Self::observe_tx(&mut tx, generation_id, group, at).await?;
        tx.commit().await?;
        Ok(id)
    }
    async fn observe_tx(
        tx: &mut Transaction<'_, Postgres>,
        generation_id: i64,
        group: &Group,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<Uuid> {
        let mut data = value(group);
        data.as_object_mut().unwrap().remove("member_list");
        let owner = crate::integrations::domain::directory::userid_key(&group.owner)?;
        data["member_key"] = json!(owner);
        data["member_count"] = json!(group.member_count());
        let id:Uuid=sqlx::query_scalar("INSERT INTO wecom_asset_objects(id,kind,corp_id,key,member_key,data,checked_at,credential_generation,visibility,verification,source) SELECT $1,'groups',corp_id,$2,$3,$4,$5,$6,'visible','verified','point' FROM wecom_directory_state ON CONFLICT(corp_id,kind,key) DO UPDATE SET member_key=excluded.member_key,data=excluded.data,checked_at=excluded.checked_at,credential_generation=excluded.credential_generation,visibility='visible',verification='verified',source='point' WHERE wecom_asset_objects.checked_at<=excluded.checked_at RETURNING id").bind(Uuid::now_v7()).bind(&group.chat_id).bind(owner).bind(data).bind(at).bind(generation_id).fetch_optional(&mut **tx).await?.ok_or(Error::Conflict)?;
        sqlx::query("UPDATE wecom_asset_objects SET visibility='not_seen',verification='unverified' WHERE kind='members' AND parent_key=$1").bind(&group.chat_id).execute(&mut **tx).await?;
        for m in &group.member_list {
            let k = if m.r#type == 1 {
                crate::integrations::domain::directory::userid_key(&m.userid)?
            } else {
                m.userid.clone()
            };
            let mut data = value(m);
            data["parent_key"] = json!(group.chat_id);
            data["member_key"] = json!(k);
            data["member_type"] = json!(m.r#type);
            sqlx::query("INSERT INTO wecom_asset_objects(id,kind,corp_id,key,parent_key,member_key,member_type,data,checked_at,credential_generation,visibility,verification,source) SELECT $1,'members',corp_id,$2,$3,$4,$5,$6,$7,$8,'visible','verified','point' FROM wecom_directory_state ON CONFLICT(corp_id,kind,key) DO UPDATE SET data=excluded.data,checked_at=excluded.checked_at,credential_generation=excluded.credential_generation,visibility='visible',verification='verified',source='point'").bind(Uuid::now_v7()).bind(edge(&group.chat_id,&edge(&m.r#type.to_string(),&k))).bind(&group.chat_id).bind(k).bind(m.r#type as i32).bind(data).bind(at).bind(generation_id).execute(&mut **tx).await?;
        }
        Ok(id)
    }
}
impl Repository {
    pub async fn failed_owner(&self, j: &AssetJob, owner: &str) -> Result<()> {
        let mut tx = self.0.begin().await?;
        fence(&mut tx, j).await?;
        sqlx::query("UPDATE wecom_asset_sync_runs SET failed_owner=$2 WHERE id=$1")
            .bind(j.fence.id)
            .bind(owner)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn confirm_scope(&self, a: &Actor, input: &OwnerScopeDecision) -> Result<()> {
        let mut tx = self.0.begin().await?;
        generation(&mut tx, input.expected_generation).await?;
        let owner = crate::integrations::domain::directory::userid_key(&input.owner_userid)?;
        sqlx::query("SELECT id FROM wecom_asset_sync_runs WHERE id=$1 AND dataset='groups' AND state='failed' AND generation=$2 AND lower(failed_owner COLLATE \"C\")=$3 AND error_code IN ('wecom_60011','wecom_60111','wecom_60121','wecom_84061') FOR UPDATE").bind(input.run_id).bind(input.expected_generation).bind(&owner).fetch_optional(&mut *tx).await?.ok_or(Error::Conflict)?;
        sqlx::query("INSERT INTO wecom_asset_owner_exclusions(owner_key,generation,actor_id,reason) VALUES($1,$2,$3,$4) ON CONFLICT(owner_key) DO UPDATE SET generation=excluded.generation,actor_id=excluded.actor_id,reason=excluded.reason,confirmed_at=now()").bind(&owner).bind(input.expected_generation).bind(a.id).bind(&input.reason).execute(&mut *tx).await?;
        sqlx::query("UPDATE wecom_asset_objects SET verification='unverified' WHERE kind='groups' AND member_key=$1").bind(&owner).execute(&mut *tx).await?;
        sqlx::query("UPDATE wecom_asset_objects m SET verification='unverified' WHERE kind='members' AND EXISTS(SELECT 1 FROM wecom_asset_objects g WHERE g.kind='groups' AND g.key=m.parent_key AND g.member_key=$1)").bind(&owner).execute(&mut *tx).await?;
        sqlx::query("UPDATE wecom_asset_sync_state SET scope_changed=true WHERE dataset='groups'")
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO audit_events(id,actor_id,action,object_id,detail) VALUES($1,$2,'wecom.assets.owner_scope.confirm',$3,$4)").bind(Uuid::now_v7()).bind(a.id).bind(input.run_id).bind(json!({"owner_userid":input.owner_userid,"generation":input.expected_generation,"reason":input.reason})).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
}

impl crate::integrations::south::port::assets::AssetStore for Repository {
    async fn enqueue(
        &self,
        actor: Option<Uuid>,
        g: i64,
        dataset: &str,
        interval: i64,
    ) -> Result<Vec<Value>> {
        Repository::enqueue(self, actor, g, dataset, interval).await
    }
    async fn status(&self) -> Result<Value> {
        Repository::status(self).await
    }
    async fn claim(&self, g: i64) -> Result<Option<AssetJob>> {
        Repository::claim(self, g).await
    }
    async fn renew(&self, j: &AssetJob) -> Result<Value> {
        Repository::renew(self, j).await
    }
    async fn stage(&self, j: &AssetJob, kind: &str, key: &str, data: Value) -> Result<()> {
        Repository::stage(self, j, kind, key, data).await
    }
    async fn page(&self, j: &AssetJob) -> Result<()> {
        Repository::page(self, j).await
    }
    async fn items(&self, j: &AssetJob, kind: &str, after: &str) -> Result<Vec<(String, Value)>> {
        Repository::items(self, j, kind, after).await
    }
    async fn scope(&self, j: &AssetJob, scope: &[String]) -> Result<()> {
        Repository::scope(self, j, scope).await
    }
    async fn owners(&self, g: i64) -> Result<Vec<String>> {
        Repository::owners(self, g).await
    }
    async fn configured_groups(&self) -> Result<Vec<String>> {
        Repository::configured_groups(self).await
    }
    async fn contains(&self, j: &AssetJob, kind: &str, key: &str) -> Result<bool> {
        Repository::contains(self, j, kind, key).await
    }
    async fn complete(&self, j: &AssetJob) -> Result<()> {
        Repository::complete(self, j).await
    }
    async fn finish(&self, j: &AssetJob) -> Result<()> {
        Repository::finish(self, j).await
    }
    async fn fail(&self, j: &AssetJob, code: &str) -> Result<()> {
        Repository::fail(self, j, code).await
    }
    async fn list(
        &self,
        kind: &str,
        q: &AssetQuery,
        parent: Option<Uuid>,
        id: Option<Uuid>,
    ) -> Result<Vec<Value>> {
        Repository::list(self, kind, q, parent, id).await
    }
    async fn candidates(
        &self,
        a: &Actor,
        school: Uuid,
        q: &AssetQuery,
        g: i64,
    ) -> Result<Vec<Value>> {
        Repository::candidates(self, a, school, q, g).await
    }
    async fn bind_group(
        &self,
        a: &Actor,
        school: Uuid,
        input: &GroupBinding,
        group: &Group,
    ) -> Result<Value> {
        Repository::bind_group(self, a, school, input, group).await
    }
    async fn candidate(&self, a: &Actor, school: Uuid, id: Uuid, g: i64) -> Result<Value> {
        Repository::candidate(self, a, school, id, g).await
    }
    async fn observe(
        &self,
        generation_id: i64,
        group: &Group,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<Uuid> {
        Repository::observe(self, generation_id, group, at).await
    }
    async fn failed_owner(&self, j: &AssetJob, owner: &str) -> Result<()> {
        Repository::failed_owner(self, j, owner).await
    }
    async fn confirm_scope(&self, a: &Actor, input: &OwnerScopeDecision) -> Result<()> {
        Repository::confirm_scope(self, a, input).await
    }
}
