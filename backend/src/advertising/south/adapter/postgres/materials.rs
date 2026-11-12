//! Persistence for independent advertising materials.
use crate::{
    advertising::{
        domain::{
            AdvertisingError, Content, Result,
            targeting::{
                ArchiveMaterial, CreateMaterial, MaterialVersionView, MaterialView, PublishMaterial,
            },
        },
        south::{adapter::postgres::Repository, port::MaterialStore},
    },
    business::Actor,
};
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

fn unavailable(_: impl std::fmt::Display) -> AdvertisingError {
    tracing::error!("independent advertising material operation failed");
    AdvertisingError("temporarily_unavailable")
}

async fn authorize(tx: &mut sqlx::PgConnection, actor: &Actor) -> Result<()> {
    let id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM accounts WHERE id=$1 AND active AND platform_admin FOR SHARE",
    )
    .bind(actor.id)
    .fetch_optional(tx)
    .await
    .map_err(unavailable)?;
    if id.is_some() {
        Ok(())
    } else {
        Err(AdvertisingError("forbidden"))
    }
}

impl MaterialStore for Repository {
    async fn create_material(&self, actor: &Actor, input: CreateMaterial) -> Result<MaterialView> {
        let id = Uuid::now_v7();
        let version = Uuid::now_v7();
        let mut tx = self.0.begin().await.map_err(unavailable)?;
        authorize(&mut tx, actor).await?;
        sqlx::query("INSERT INTO ad_materials(id,created_by) VALUES($1,$2)")
            .bind(id)
            .bind(actor.id)
            .execute(&mut *tx)
            .await
            .map_err(unavailable)?;
        sqlx::query("INSERT INTO ad_material_versions(id,material_id,version,name,body,link,created_by) VALUES($1,$2,1,$3,$4,$5,$6)")
            .bind(version).bind(id).bind(&input.content.name).bind(&input.content.body).bind(&input.content.link).bind(actor.id)
            .execute(&mut *tx).await.map_err(unavailable)?;
        sqlx::query("UPDATE ad_materials SET current_version_id=$2 WHERE id=$1")
            .bind(id)
            .bind(version)
            .execute(&mut *tx)
            .await
            .map_err(unavailable)?;
        crate::audit::record(
            &mut tx,
            actor,
            Uuid::nil(),
            "advertising.material.create",
            id,
            json!({"revision":1,"content_version":version,"reason":input.reason}),
        )
        .await
        .map_err(unavailable)?;
        tx.commit().await.map_err(unavailable)?;
        Ok(MaterialView {
            id,
            revision: 1,
            archived: false,
            content_version: version,
            content: input.content,
        })
    }

    async fn material(&self, actor: &Actor, id: Uuid) -> Result<Option<MaterialView>> {
        let mut tx = self.0.begin().await.map_err(unavailable)?;
        authorize(&mut tx, actor).await?;
        let row = sqlx::query("SELECT m.id,m.revision,m.archived,v.id AS version_id,v.name,v.body,v.link FROM ad_materials m JOIN ad_material_versions v ON v.material_id=m.id AND v.id=m.current_version_id WHERE m.id=$1")
            .bind(id).fetch_optional(&mut *tx).await.map_err(unavailable)?;
        tx.commit().await.map_err(unavailable)?;
        Ok(row.map(|r| MaterialView {
            id: r.get("id"),
            revision: r.get("revision"),
            archived: r.get("archived"),
            content_version: r.get("version_id"),
            content: Content {
                name: r.get("name"),
                body: r.get("body"),
                link: r.get("link"),
            },
        }))
    }

    async fn materials(&self, actor: &Actor, offset: i64) -> Result<Vec<MaterialView>> {
        let mut tx = self.0.begin().await.map_err(unavailable)?;
        authorize(&mut tx, actor).await?;
        let rows = sqlx::query("SELECT m.id,m.revision,m.archived,v.id AS version_id,v.name,v.body,v.link FROM ad_materials m JOIN ad_material_versions v ON v.material_id=m.id AND v.id=m.current_version_id ORDER BY m.id LIMIT 31 OFFSET $1")
            .bind(offset).fetch_all(&mut *tx).await.map_err(unavailable)?;
        let items = rows
            .into_iter()
            .map(|r| MaterialView {
                id: r.get("id"),
                revision: r.get("revision"),
                archived: r.get("archived"),
                content_version: r.get("version_id"),
                content: Content {
                    name: r.get("name"),
                    body: r.get("body"),
                    link: r.get("link"),
                },
            })
            .collect();
        tx.commit().await.map_err(unavailable)?;
        Ok(items)
    }

    async fn material_versions(
        &self,
        actor: &Actor,
        id: Uuid,
        offset: i64,
    ) -> Result<Option<Vec<MaterialVersionView>>> {
        let mut tx = self.0.begin().await.map_err(unavailable)?;
        authorize(&mut tx, actor).await?;
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM ad_materials WHERE id=$1)")
                .bind(id)
                .fetch_one(&mut *tx)
                .await
                .map_err(unavailable)?;
        if !exists {
            return Ok(None);
        }
        let rows = sqlx::query("SELECT id,material_id,version,name,body,link,created_by,created_at FROM ad_material_versions WHERE material_id=$1 ORDER BY version ASC LIMIT 31 OFFSET $2")
            .bind(id).bind(offset).fetch_all(&mut *tx).await.map_err(unavailable)?;
        let versions = rows
            .into_iter()
            .map(|r| MaterialVersionView {
                id: r.get("id"),
                material_id: r.get("material_id"),
                version: r.get("version"),
                content: Content {
                    name: r.get("name"),
                    body: r.get("body"),
                    link: r.get("link"),
                },
                created_by: r.get("created_by"),
                created_at: r.get("created_at"),
            })
            .collect();
        tx.commit().await.map_err(unavailable)?;
        Ok(Some(versions))
    }

    async fn archive_material(
        &self,
        actor: &Actor,
        id: Uuid,
        input: ArchiveMaterial,
    ) -> Result<MaterialView> {
        let mut tx = self.0.begin().await.map_err(unavailable)?;
        authorize(&mut tx, actor).await?;
        let current =
            sqlx::query("SELECT revision,archived FROM ad_materials WHERE id=$1 FOR UPDATE")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(unavailable)?
                .ok_or(AdvertisingError("not_found"))?;
        let revision: i64 = current.get("revision");
        if revision != input.expected_revision {
            return Err(AdvertisingError("revision_conflict"));
        }
        if current.get::<bool, _>("archived") {
            return Err(AdvertisingError("material_archived"));
        }
        let next = revision
            .checked_add(1)
            .ok_or(AdvertisingError("temporarily_unavailable"))?;
        let row = sqlx::query("UPDATE ad_materials SET archived=true,revision=$2 WHERE id=$1 RETURNING current_version_id")
            .bind(id).bind(next).fetch_one(&mut *tx).await.map_err(unavailable)?;
        let version: Uuid = row.get("current_version_id");
        let content = sqlx::query(
            "SELECT name,body,link FROM ad_material_versions WHERE id=$1 AND material_id=$2",
        )
        .bind(version)
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .map_err(unavailable)?;
        crate::audit::record(
            &mut tx,
            actor,
            Uuid::nil(),
            "advertising.material.archive",
            id,
            json!({"revision":next,"content_version":version,"reason":input.reason}),
        )
        .await
        .map_err(unavailable)?;
        let result = MaterialView {
            id,
            revision: next,
            archived: true,
            content_version: version,
            content: Content {
                name: content.get("name"),
                body: content.get("body"),
                link: content.get("link"),
            },
        };
        tx.commit().await.map_err(unavailable)?;
        Ok(result)
    }

    async fn publish_material(
        &self,
        actor: &Actor,
        id: Uuid,
        input: PublishMaterial,
    ) -> Result<MaterialView> {
        let mut tx = self.0.begin().await.map_err(unavailable)?;
        authorize(&mut tx, actor).await?;
        let row = sqlx::query("SELECT revision,archived FROM ad_materials WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(unavailable)?
            .ok_or(AdvertisingError("not_found"))?;
        if row.get::<bool, _>("archived") {
            return Err(AdvertisingError("material_archived"));
        }
        let current: i64 = row.get("revision");
        if current != input.expected_revision {
            return Err(AdvertisingError("revision_conflict"));
        }
        let revision = current
            .checked_add(1)
            .ok_or(AdvertisingError("temporarily_unavailable"))?;
        let version = Uuid::now_v7();
        sqlx::query("INSERT INTO ad_material_versions(id,material_id,version,name,body,link,created_by) VALUES($1,$2,$3,$4,$5,$6,$7)")
            .bind(version).bind(id).bind(revision).bind(&input.content.name).bind(&input.content.body).bind(&input.content.link).bind(actor.id)
            .execute(&mut *tx).await.map_err(unavailable)?;
        sqlx::query("UPDATE ad_materials SET current_version_id=$2,revision=$3 WHERE id=$1")
            .bind(id)
            .bind(version)
            .bind(revision)
            .execute(&mut *tx)
            .await
            .map_err(unavailable)?;
        crate::audit::record(
            &mut tx,
            actor,
            Uuid::nil(),
            "advertising.material.publish",
            id,
            json!({"revision":revision,"content_version":version,"reason":input.reason}),
        )
        .await
        .map_err(unavailable)?;
        tx.commit().await.map_err(unavailable)?;
        Ok(MaterialView {
            id,
            revision,
            archived: false,
            content_version: version,
            content: input.content,
        })
    }
}

fn scope_parts(
    scope: crate::advertising::domain::targeting::TargetScope,
) -> (&'static str, Option<Uuid>, Option<Uuid>) {
    use crate::advertising::domain::targeting::TargetScope;
    match scope {
        TargetScope::Global {} => ("global", None, None),
        TargetScope::Platform { platform_id } => ("platform", Some(platform_id), None),
        TargetScope::Unit {
            platform_id,
            unit_id,
        } => ("unit", Some(platform_id), Some(unit_id)),
    }
}

fn decode_scope(
    kind: &str,
    platform: Option<Uuid>,
    unit: Option<Uuid>,
) -> Result<crate::advertising::domain::targeting::TargetScope> {
    use crate::advertising::domain::targeting::TargetScope;
    match (kind, platform, unit) {
        ("global", None, None) => Ok(TargetScope::Global {}),
        ("platform", Some(platform_id), None) => Ok(TargetScope::Platform { platform_id }),
        ("unit", Some(platform_id), Some(unit_id)) => Ok(TargetScope::Unit {
            platform_id,
            unit_id,
        }),
        _ => Err(AdvertisingError("temporarily_unavailable")),
    }
}

impl crate::advertising::south::port::PlacementRuleStore for Repository {
    async fn create_rule(
        &self,
        actor: &Actor,
        input: crate::advertising::domain::targeting::CreatePlacementRule,
    ) -> Result<crate::advertising::domain::targeting::PlacementRuleView> {
        use crate::advertising::domain::targeting::PlacementRuleView;
        let mut tx = self.0.begin().await.map_err(unavailable)?;
        authorize(&mut tx, actor).await?;
        let material = sqlx::query(
            "SELECT archived,current_version_id FROM ad_materials WHERE id=$1 FOR SHARE",
        )
        .bind(input.material_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(unavailable)?
        .ok_or(AdvertisingError("not_found"))?;
        if material.get::<bool, _>("archived") {
            return Err(AdvertisingError("material_archived"));
        }
        if material
            .get::<Option<Uuid>, _>("current_version_id")
            .is_none()
        {
            return Err(AdvertisingError("material_unpublished"));
        }
        let id = Uuid::now_v7();
        let (kind, platform, unit) = scope_parts(input.scope);
        let name = input.name.trim().to_owned();
        sqlx::query("INSERT INTO ad_placement_rules(id,name,material_id,scope_kind,platform_id,unit_id,enabled,valid_from,valid_until,created_by,updated_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$10)")
            .bind(id).bind(&name).bind(input.material_id).bind(kind).bind(platform).bind(unit).bind(input.enabled).bind(input.valid_from).bind(input.valid_until).bind(actor.id)
            .execute(&mut *tx).await.map_err(|error| {
                if error.as_database_error().is_some_and(|db| db.code().as_deref() == Some("23503")) { AdvertisingError("invalid_ad_scope") } else { unavailable(error) }
            })?;
        crate::audit::record(&mut tx, actor, Uuid::nil(), "advertising.placement.create", id,
            json!({"material_id":input.material_id,"scope":input.scope,"enabled":input.enabled,"reason":input.reason})).await.map_err(unavailable)?;
        tx.commit().await.map_err(unavailable)?;
        Ok(PlacementRuleView {
            id,
            name,
            material_id: input.material_id,
            scope: input.scope,
            enabled: input.enabled,
            valid_from: input.valid_from,
            valid_until: input.valid_until,
            revision: 1,
        })
    }
    async fn rule(
        &self,
        actor: &Actor,
        id: Uuid,
    ) -> Result<Option<crate::advertising::domain::targeting::PlacementRuleView>> {
        use crate::advertising::domain::targeting::PlacementRuleView;
        let mut tx = self.0.begin().await.map_err(unavailable)?;
        authorize(&mut tx, actor).await?;
        let row = sqlx::query("SELECT id,name,material_id,scope_kind,platform_id,unit_id,enabled,valid_from,valid_until,revision FROM ad_placement_rules WHERE id=$1")
            .bind(id).fetch_optional(&mut *tx).await.map_err(unavailable)?;
        let result = row
            .map(|r| -> Result<PlacementRuleView> {
                Ok(PlacementRuleView {
                    id: r.get("id"),
                    name: r.get("name"),
                    material_id: r.get("material_id"),
                    scope: decode_scope(
                        r.get::<String, _>("scope_kind").as_str(),
                        r.get("platform_id"),
                        r.get("unit_id"),
                    )?,
                    enabled: r.get("enabled"),
                    valid_from: r.get("valid_from"),
                    valid_until: r.get("valid_until"),
                    revision: r.get("revision"),
                })
            })
            .transpose()?;
        tx.commit().await.map_err(unavailable)?;
        Ok(result)
    }

    async fn rules(
        &self,
        actor: &Actor,
        offset: i64,
    ) -> Result<Vec<crate::advertising::domain::targeting::PlacementRuleView>> {
        use crate::advertising::domain::targeting::PlacementRuleView;
        let mut tx = self.0.begin().await.map_err(unavailable)?;
        authorize(&mut tx, actor).await?;
        let rows = sqlx::query("SELECT id,name,material_id,scope_kind,platform_id,unit_id,enabled,valid_from,valid_until,revision FROM ad_placement_rules ORDER BY id LIMIT 31 OFFSET $1")
            .bind(offset).fetch_all(&mut *tx).await.map_err(unavailable)?;
        let items = rows
            .into_iter()
            .map(|r| -> Result<PlacementRuleView> {
                Ok(PlacementRuleView {
                    id: r.get("id"),
                    name: r.get("name"),
                    material_id: r.get("material_id"),
                    scope: decode_scope(
                        r.get::<String, _>("scope_kind").as_str(),
                        r.get("platform_id"),
                        r.get("unit_id"),
                    )?,
                    enabled: r.get("enabled"),
                    valid_from: r.get("valid_from"),
                    valid_until: r.get("valid_until"),
                    revision: r.get("revision"),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        tx.commit().await.map_err(unavailable)?;
        Ok(items)
    }

    async fn set_rule_state(
        &self,
        actor: &Actor,
        id: Uuid,
        input: crate::advertising::domain::targeting::SetPlacementState,
    ) -> Result<crate::advertising::domain::targeting::PlacementRuleView> {
        use crate::advertising::domain::targeting::PlacementRuleView;
        let mut tx = self.0.begin().await.map_err(unavailable)?;
        authorize(&mut tx, actor).await?;
        let current = sqlx::query(
            "SELECT revision,enabled,material_id FROM ad_placement_rules WHERE id=$1 FOR UPDATE",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(unavailable)?
        .ok_or(AdvertisingError("not_found"))?;
        let revision: i64 = current.get("revision");
        if revision != input.expected_revision {
            return Err(AdvertisingError("revision_conflict"));
        }
        if input.enabled {
            let material = sqlx::query(
                "SELECT archived,current_version_id FROM ad_materials WHERE id=$1 FOR SHARE",
            )
            .bind(current.get::<Uuid, _>("material_id"))
            .fetch_one(&mut *tx)
            .await
            .map_err(unavailable)?;
            if material.get::<bool, _>("archived") {
                return Err(AdvertisingError("material_archived"));
            }
            if material
                .get::<Option<Uuid>, _>("current_version_id")
                .is_none()
            {
                return Err(AdvertisingError("material_unpublished"));
            }
        }
        let next = revision
            .checked_add(1)
            .ok_or(AdvertisingError("temporarily_unavailable"))?;
        let row = sqlx::query("UPDATE ad_placement_rules SET enabled=$2,revision=$3,updated_by=$4,updated_at=now() WHERE id=$1 RETURNING id,name,material_id,scope_kind,platform_id,unit_id,enabled,valid_from,valid_until,revision")
            .bind(id).bind(input.enabled).bind(next).bind(actor.id).fetch_one(&mut *tx).await.map_err(unavailable)?;
        let result = PlacementRuleView {
            id: row.get("id"),
            name: row.get("name"),
            material_id: row.get("material_id"),
            scope: decode_scope(
                row.get::<String, _>("scope_kind").as_str(),
                row.get("platform_id"),
                row.get("unit_id"),
            )?,
            enabled: row.get("enabled"),
            valid_from: row.get("valid_from"),
            valid_until: row.get("valid_until"),
            revision: row.get("revision"),
        };
        crate::audit::record(&mut tx, actor, Uuid::nil(), "advertising.placement.state", id,
            json!({"previous_enabled":current.get::<bool,_>("enabled"),"enabled":input.enabled,"revision":next,"reason":input.reason})).await.map_err(unavailable)?;
        tx.commit().await.map_err(unavailable)?;
        Ok(result)
    }
}
