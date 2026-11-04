use crate::business::Result;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;
#[derive(Clone)]
pub struct Repository(pub PgPool);
impl Repository {
    pub async fn templates(&self, s: Uuid) -> Result<Vec<Value>> {
        Ok(sqlx::query_scalar("SELECT to_jsonb(t)||jsonb_build_object('versions',coalesce((SELECT jsonb_agg(to_jsonb(v)||jsonb_build_object('publisher_name',(SELECT a.display_name FROM accounts a WHERE a.id=v.published_by)) ORDER BY v.created_at DESC) FROM template_versions v WHERE v.template_id=t.id),'[]'::jsonb)) FROM templates t WHERE school_id=$1 ORDER BY created_at DESC").bind(s).fetch_all(&self.0).await?)
    }
}
impl crate::distribution::south::port::Store for Repository {
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
