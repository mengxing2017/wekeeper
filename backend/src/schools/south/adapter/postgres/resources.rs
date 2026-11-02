use super::Repository;
use crate::{
    business::{Actor, Error, Result, require},
    distribution::south::adapter::postgres::product_preview as preview,
    schools::{
        domain::{ImageRef, ResourceAction, ResourceInput},
        south::port::resources::{Command, Store},
    },
};
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;
async fn image_check(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    s: Option<Uuid>,
    image: &Option<ImageRef>,
) -> Result<()> {
    match image {
        Some(ImageRef::Asset { id }) => {
            sqlx::query("SELECT id FROM operational_media WHERE id=$1 AND school_id IS NOT DISTINCT FROM $2 FOR SHARE").bind(id).bind(s).fetch_optional(&mut **tx).await?.ok_or(Error::Forbidden)?;
        }
        None => {}
    }
    Ok(())
}
async fn resource_check(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    s: Option<Uuid>,
    v: &ResourceInput,
) -> Result<()> {
    image_check(tx, s, &v.image_ref).await?;
    if let ResourceAction::InternalPage { page, object_id } = &v.action
        && page == "post"
    {
        let school = s.ok_or_else(|| Error::Invalid("平台默认资源不能引用学校私有动态".into()))?;
        sqlx::query("SELECT id FROM posts WHERE id=$1 AND school_id=$2 AND audit_status='approved' AND visibility='visible' FOR SHARE").bind(object_id).bind(school).fetch_optional(&mut **tx).await?.ok_or(Error::NotFound)?;
    }
    Ok(())
}
impl Store for Repository {
    async fn command(&self, a: &Actor, s: Option<Uuid>, command: Command) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        preview::authorize(&mut tx, a, s, "admin").await?;
        sqlx::query("SELECT pg_advisory_xact_lock(78234610244)")
            .execute(&mut *tx)
            .await?;
        let result = match command {
            Command::Preview(input) => {
                resource_check(&mut tx, s, &input).await?;
                if let Some(id) = input.id {
                    sqlx::query("SELECT id FROM school_resources WHERE id=$1 AND school_id IS NOT DISTINCT FROM $2 AND revision=$3 FOR SHARE").bind(id).bind(s).bind(input.revision).fetch_optional(&mut *tx).await?.ok_or(Error::Conflict)?;
                }
                preview::create(&mut tx, a, s, "resource", json!(input), json!({})).await?
            }
            Command::Confirm { preview_id, digest } => {
                let (payload, _, prior) =
                    preview::load(&mut tx, a, s, "resource", preview_id, &digest).await?;
                if let Some(prior) = prior {
                    return Ok(prior);
                }
                let v: ResourceInput =
                    serde_json::from_value(payload.clone()).map_err(|_| Error::Conflict)?;
                v.validate()?;
                resource_check(&mut tx, s, &v).await?;
                let id = v.id.unwrap_or_else(Uuid::now_v7);
                if v.id.is_none() {
                    sqlx::query("INSERT INTO school_resources(id,school_id,kind,title,content,active,sort_order,starts_at,ends_at,image_ref,action) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)").bind(id).bind(s).bind(&v.kind).bind(&v.title).bind(&v.content).bind(v.active).bind(v.sort_order).bind(v.starts_at).bind(v.ends_at).bind(json!(v.image_ref)).bind(json!(v.action)).execute(&mut *tx).await?;
                } else {
                    let n=sqlx::query("UPDATE school_resources SET title=$3,content=$4,active=$5,sort_order=$6,starts_at=$7,ends_at=$8,image_ref=$9,action=$10,revision=revision+1 WHERE id=$1 AND school_id IS NOT DISTINCT FROM $2 AND revision=$11 AND kind=$12").bind(id).bind(s).bind(&v.title).bind(&v.content).bind(v.active).bind(v.sort_order).bind(v.starts_at).bind(v.ends_at).bind(json!(v.image_ref)).bind(json!(v.action)).bind(v.revision).bind(&v.kind).execute(&mut *tx).await?.rows_affected();
                    require(n == 1, "资源已变化，请重新核对")?;
                }
                sqlx::query("INSERT INTO resource_publications(id,resource_id,school_id,actor_id,snapshot) VALUES($1,$2,$3,$4,$5)").bind(Uuid::now_v7()).bind(id).bind(s).bind(a.id).bind(payload).execute(&mut *tx).await?;
                let result = json!({"id":id,"published":true});
                preview::finish(&mut tx, preview_id, &result).await?;
                result
            }
        };
        tx.commit().await?;
        Ok(result)
    }
    async fn upload(
        &self,
        a: &Actor,
        s: Option<Uuid>,
        mime: String,
        width: u32,
        height: u32,
        bytes: Vec<u8>,
    ) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        preview::authorize(&mut tx, a, s, "admin").await?;
        let id = Uuid::now_v7();
        sqlx::query("INSERT INTO operational_media(id,school_id,owner_id,mime_type,size_bytes,width,height,content) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(id).bind(s).bind(a.id).bind(mime).bind(bytes.len() as i64).bind(width as i32).bind(height as i32).bind(bytes).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(
            json!({"id":id,"image_ref":{"kind":"asset","id":id},"url":format!("{}/resource-artwork/{id}",s.map(|s|format!("/api/v1/schools/{s}")).unwrap_or("/api/v1/platform".into()))}),
        )
    }
    async fn media_list(&self, a: &Actor, s: Option<Uuid>) -> Result<Vec<Value>> {
        let mut tx = self.0.begin().await?;
        preview::authorize(&mut tx, a, s, "admin").await?;
        Ok(sqlx::query_scalar("SELECT jsonb_build_object('id',id,'image_ref',jsonb_build_object('kind','asset','id',id),'width',width,'height',height,'size_bytes',size_bytes,'created_at',created_at) FROM operational_media WHERE school_id IS NOT DISTINCT FROM $1 ORDER BY created_at DESC,id DESC").bind(s).fetch_all(&mut *tx).await?)
    }
    async fn media(
        &self,
        a: &Actor,
        s: Option<Uuid>,
        id: Uuid,
        published: bool,
    ) -> Result<(String, Vec<u8>)> {
        let mut tx = self.0.begin().await?;
        preview::authorize(&mut tx, a, s, if published { "member" } else { "admin" }).await?;
        let row=if published{
 let s=s.ok_or(Error::Forbidden)?;
 let resources=self.resources(s,false).await?;
 let resource=resources.iter().find(|v|v["id"].as_str()==Some(&id.to_string())).ok_or(Error::NotFound)?;
 let image:ImageRef=serde_json::from_value(resource["image_ref"].clone()).map_err(|_|Error::NotFound)?;
 let ImageRef::Asset{id:asset}=image;
 sqlx::query("SELECT mime_type,content FROM operational_media WHERE id=$1 AND (school_id=$2 OR school_id IS NULL)").bind(asset).bind(s).fetch_optional(&mut *tx).await?
 }else{sqlx::query("SELECT mime_type,content FROM operational_media WHERE id=$1 AND school_id IS NOT DISTINCT FROM $2").bind(id).bind(s).fetch_optional(&mut *tx).await?}.ok_or(Error::NotFound)?;
        Ok((row.get("mime_type"), row.get("content")))
    }
}

impl Repository {
    pub async fn cleanup_artwork(&self) -> Result<()> {
        let mut tx = self.0.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(78234610244)")
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM operational_media m WHERE created_at<now()-interval '7 days' AND NOT EXISTS(SELECT 1 FROM school_resources r WHERE r.image_ref->>'kind'='asset' AND r.image_ref->>'id'=m.id::text) AND NOT EXISTS(SELECT 1 FROM resource_publications r WHERE r.snapshot->'image_ref'->>'kind'='asset' AND r.snapshot->'image_ref'->>'id'=m.id::text) AND NOT EXISTS(SELECT 1 FROM product_previews p WHERE p.kind='resource' AND p.expires_at>now() AND p.payload->'image_ref'->>'id'=m.id::text)").execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
}
