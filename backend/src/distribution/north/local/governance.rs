//! Explicit platform/school governance use cases, separate from send confirmation.
use crate::{
    business::{Actor, Page, Result},
    distribution::{
        domain::{
            content::{OfficialSender, SenderQualificationInput},
            resource_policy::{ResourcePolicy, SourcePolicy},
        },
        south::adapter::postgres::{resource_policy, senders},
    },
};
use uuid::Uuid;
#[derive(Clone)]
pub struct Service(pub sqlx::PgPool);
impl Service {
    pub async fn senders(
        &self,
        a: &Actor,
        s: Option<Uuid>,
        page: Page,
    ) -> Result<Vec<OfficialSender>> {
        senders::Repository(self.0.clone()).list(a, s, page).await
    }
    pub async fn qualify(
        &self,
        a: &Actor,
        s: Uuid,
        id: Uuid,
        input: SenderQualificationInput,
    ) -> Result<()> {
        a.platform()?;
        senders::Repository(self.0.clone())
            .qualify(a, s, id, input)
            .await
    }
    pub async fn policies(&self, a: &Actor) -> Result<serde_json::Value> {
        a.platform()?;
        resource_policy::Repository(self.0.clone())
            .policies(a)
            .await
    }
    pub async fn source_policy(&self, a: &Actor, input: SourcePolicy) -> Result<()> {
        a.platform()?;
        resource_policy::Repository(self.0.clone())
            .source(a, input)
            .await
    }
    pub async fn resource_policy(&self, a: &Actor, input: ResourcePolicy) -> Result<()> {
        a.platform()?;
        resource_policy::Repository(self.0.clone())
            .resource(a, input)
            .await
    }
}
