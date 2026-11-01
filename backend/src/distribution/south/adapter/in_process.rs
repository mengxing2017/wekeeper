//! The only production forum dependency in the distribution adapter layer.
use crate::{
    business::{Actor, Error, Result, require},
    distribution::{
        domain::content::{Material, SourceReference},
        south::port::content_source::ContentSource,
    },
};
use sqlx::PgPool;
use uuid::Uuid;
#[derive(Clone)]
pub struct Sources(pub PgPool);
impl ContentSource for Sources {
    async fn check(&self, school: Uuid, references: &[SourceReference]) -> Result<()> {
        for reference in references {
            reference.validate()?;
            require(
                reference.namespace == "builtin.forum"
                    && reference.external_organization_id == school.to_string(),
                "source_entry_not_authorized",
            )?;
            let id = reference
                .external_content_id
                .parse()
                .map_err(|_| Error::NotFound)?;
            crate::community::north::local::Service(
                crate::community::south::adapter::postgres::Repository(self.0.clone()),
            )
            .distribution_eligibility(school, id, &reference.version)
            .await?;
        }
        Ok(())
    }
    async fn read(
        &self,
        actor: &Actor,
        school: Uuid,
        references: &[SourceReference],
    ) -> Result<Vec<Material>> {
        let mut out = Vec::new();
        for reference in references {
            reference.validate()?;
            if reference.namespace == "builtin.forum" {
                require(
                    reference.external_organization_id == school.to_string(),
                    "来源学校不匹配",
                )?;
                let id = reference
                    .external_content_id
                    .parse()
                    .map_err(|_| Error::NotFound)?;
                let material = crate::community::north::local::Service(
                    crate::community::south::adapter::postgres::Repository(self.0.clone()),
                )
                .distribution_material(actor, school, id)
                .await?;
                if material.version != reference.version {
                    return Err(Error::Policy("source_version_conflict"));
                }
                out.push(Material {
                    reference: reference.clone(),
                    title: material.title,
                    content: material.content,
                    detail_url: format!("/community/{school}/posts/{id}"),
                });
            } else {
                // The browser cannot select another product's identity. Future
                // sources need a separately authenticated adapter/entry point.
                return Err(Error::Policy("source_entry_not_authorized"));
            }
        }
        Ok(out)
    }
}
