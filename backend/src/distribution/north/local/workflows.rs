use crate::{
    business::{Actor, Page, Result},
    distribution::{
        domain::workflows::Command,
        south::{adapter::postgres::workflows::Repository, port::workflows::Store},
    },
};
use serde_json::Value;
use uuid::Uuid;
#[derive(Clone)]
pub struct Service<R: Store = Repository>(pub R);
impl<R: Store> Service<R> {
    pub async fn command(&self, a: &Actor, s: Option<Uuid>, v: Command) -> Result<Value> {
        v.validate()?;
        self.0.command(a, s, v).await
    }
    pub async fn options(&self, a: &Actor, s: Option<Uuid>) -> Result<Value> {
        self.0.options(a, s).await
    }
    pub async fn requests(&self, a: &Actor, s: Option<Uuid>, p: Page) -> Result<Vec<Value>> {
        self.0.requests(a, s, p).await
    }
    pub async fn channel(
        &self,
        a: &Actor,
        s: Uuid,
        configured: bool,
        enabled: bool,
        scope: &str,
    ) -> Result<Value> {
        self.0.channel(a, s, configured, enabled, scope).await
    }
}

impl<R: Store> Service<R> {
    pub async fn deliver(
        &self,
        actor: Uuid,
        integration: crate::integrations::north::local::Service,
        available: bool,
        source: String,
        valid_seconds: i64,
    ) -> Result<Uuid> {
        use crate::distribution::south::port::broadcast::Client;
        self.0.service_identity(actor).await?;
        let scope = integration.credential_scope();
        let client = crate::integrations::south::adapter::broadcast_http::Client(integration);
        let (corp, generation) = client.generation().await?;
        if available {
            let end = chrono::Utc::now().timestamp();
            client.candidates(end - 3600, end).await?;
        }
        self.0
            .record_delivery(
                actor,
                crate::distribution::domain::workflows::ChannelDelivery {
                    corp,
                    generation,
                    scope,
                    available,
                    source,
                    valid_seconds,
                },
            )
            .await
    }
}
