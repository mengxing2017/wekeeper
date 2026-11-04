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
    pub async fn check_health(
        &self,
        integration: crate::integrations::north::local::Service,
    ) -> Result<()> {
        if !integration.server_configured() {
            return Ok(());
        }
        let scope = integration.credential_scope();
        if !self.0.claim_health_probe(&scope).await? {
            return Ok(());
        }
        use crate::distribution::south::port::broadcast::Client;
        let client = crate::integrations::south::adapter::broadcast_http::Client(integration);
        let mut identity = None;
        let result = tokio::time::timeout(std::time::Duration::from_secs(20), async {
            identity = Some(client.generation().await.map_err(|error| {
                crate::integrations::north::directory::error_code(&error).to_owned()
            })?);
            client.probe_read_access().await
        })
        .await;
        let (available, error_code) = match result {
            Ok(Ok(())) => (true, None),
            Ok(Err(error)) => (false, Some(error)),
            Err(_) => (false, Some("probe_timeout".into())),
        };
        let (corp, generation) = identity
            .map(|(c, g)| (Some(c), Some(g)))
            .unwrap_or((None, None));
        self.0
            .record_health(crate::distribution::domain::workflows::ChannelHealth {
                corp,
                generation,
                scope,
                available,
                error_code,
            })
            .await
    }
    pub async fn channel_control_read(&self, a: &Actor, scope: &str) -> Result<Value> {
        self.0.channel_control_read(a, scope).await
    }
    pub async fn channel_control(
        &self,
        a: &Actor,
        scope: &str,
        input: crate::distribution::domain::workflows::ChannelControl,
    ) -> Result<Value> {
        crate::business::text(&input.reason, 1, 1000)?;
        crate::business::require(input.expected_revision >= 0, "修订无效")?;
        self.0.channel_control(a, scope, input).await
    }
}
