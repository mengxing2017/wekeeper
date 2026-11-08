use crate::business::{Page, Result};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;
#[derive(Clone)]
pub struct Repository(pub PgPool);
impl Repository {
    pub async fn materials(&self, s: Uuid, page: Page) -> Result<Vec<Value>> {
        let (limit, offset) = page.fetch_bounds()?;
        Ok(sqlx::query_scalar("SELECT jsonb_build_object('id',m.external_content_id,'title',m.title,'content',m.content,'version',m.version) FROM distribution_materials m JOIN distribution_sources src USING(namespace) JOIN distribution_source_organizations org USING(namespace,external_organization_id,school_id) WHERE m.school_id=$1 AND m.namespace='builtin.forum' AND m.external_organization_id=$1::text AND m.allowed AND src.enabled AND org.enabled AND ($2='' OR m.title ILIKE '%'||$2||'%' OR m.content ILIKE '%'||$2||'%') ORDER BY m.updated_at DESC,m.external_content_id LIMIT $3 OFFSET $4").bind(s).bind(page.search.unwrap_or_default()).bind(limit).bind(offset).fetch_all(&self.0).await?)
    }

    pub async fn templates(&self, s: Uuid) -> Result<Vec<Value>> {
        Ok(sqlx::query_scalar("SELECT to_jsonb(t)||jsonb_build_object('versions',coalesce((SELECT jsonb_agg(to_jsonb(v)||jsonb_build_object('publisher_name',(SELECT a.display_name FROM accounts a WHERE a.id=v.published_by)) ORDER BY v.created_at DESC) FROM template_versions v WHERE v.template_id=t.id),'[]'::jsonb)) FROM templates t WHERE school_id=$1 ORDER BY created_at DESC").bind(s).fetch_all(&self.0).await?)
    }
}
impl crate::distribution::south::port::Store for Repository {
    async fn materials(&self, s: Uuid, page: Page) -> Result<Vec<Value>> {
        Repository::materials(self, s, page).await
    }

    async fn templates(&self, s: Uuid) -> Result<Vec<Value>> {
        Repository::templates(self, s).await
    }
}

pub mod broadcast;

pub mod device_registry;

pub mod device_control;

pub mod shared_mobile_resources;

pub mod content;

pub mod senders;

pub mod resource_policy;

pub mod shared_send_v2;

pub(crate) mod product_preview;

pub mod workflows;

pub mod mobile_workspaces;

pub mod wecom_workspaces;

pub mod groups;
