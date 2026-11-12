use crate::business_directory::{
    domain::{BusinessPlatform, BusinessUnit, DirectoryError, Result},
    south::port::UnitStore,
};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Clone)]
pub struct Repository(pub PgPool);

impl UnitStore for Repository {
    async fn find(&self, platform_id: Uuid, unit_id: Uuid) -> Result<Option<BusinessUnit>> {
        let row = sqlx::query(
            "SELECT id,platform_id,name,active,revision FROM business_units WHERE platform_id=$1 AND id=$2",
        )
        .bind(platform_id)
        .bind(unit_id)
        .fetch_optional(&self.0)
        .await
        .map_err(|error| {
            tracing::error!(error = %error, "business unit lookup failed");
            DirectoryError::Unavailable
        })?;
        Ok(row.map(|r| BusinessUnit {
            id: r.get("id"),
            platform_id: r.get("platform_id"),
            name: r.get("name"),
            active: r.get("active"),
            revision: r.get("revision"),
        }))
    }

    async fn list_platforms(&self, offset: i64) -> Result<Vec<BusinessPlatform>> {
        let rows = sqlx::query("SELECT source_id,unit_name,status FROM source_credentials ORDER BY source_id LIMIT 31 OFFSET $1")
            .bind(offset).fetch_all(&self.0).await.map_err(|error| {
                tracing::error!(error = %error, "business platform listing failed");
                DirectoryError::Unavailable
            })?;
        Ok(rows
            .into_iter()
            .map(|r| BusinessPlatform {
                id: r.get("source_id"),
                name: r.get("unit_name"),
                status: r.get("status"),
            })
            .collect())
    }

    async fn list_units(&self, platform_id: Uuid, offset: i64) -> Result<Vec<BusinessUnit>> {
        let rows = sqlx::query("SELECT id,platform_id,name,active,revision FROM business_units WHERE platform_id=$1 ORDER BY id LIMIT 31 OFFSET $2")
            .bind(platform_id).bind(offset).fetch_all(&self.0).await.map_err(|error| {
                tracing::error!(error = %error, "business unit listing failed");
                DirectoryError::Unavailable
            })?;
        Ok(rows
            .into_iter()
            .map(|r| BusinessUnit {
                id: r.get("id"),
                platform_id: r.get("platform_id"),
                name: r.get("name"),
                active: r.get("active"),
                revision: r.get("revision"),
            })
            .collect())
    }
}
