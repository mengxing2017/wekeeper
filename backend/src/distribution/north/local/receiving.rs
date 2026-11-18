use crate::{
    bootstrap::Application,
    business::{Actor, Page, Result, require},
    distribution::{
        domain::receiving::{
            Channel, Configuration, ConfigurationInput, TargetInput, TargetResource,
        },
        south::{adapter::postgres::receiving::Repository, port::receiving::Store},
    },
};
use uuid::Uuid;

#[derive(Clone)]
pub struct Service<S = Repository>(pub S);
impl<S: Store> Service<S> {
    pub async fn phone_progress(
        &self,
        actor: &Actor,
        receipt: Uuid,
        page: Page,
    ) -> Result<serde_json::Value> {
        actor.platform()?;
        page.fetch_bounds()?;
        self.0.phone_progress(actor, receipt, page).await
    }
    pub async fn stop_phone(
        &self,
        actor: &Actor,
        receipt: Uuid,
        target: Option<Uuid>,
    ) -> Result<serde_json::Value> {
        actor.platform()?;
        self.0.stop_phone(actor, receipt, target).await
    }
    pub async fn target(
        &self,
        actor: &Actor,
        channel: Channel,
        unit: Uuid,
        id: Uuid,
        mut input: TargetInput,
    ) -> Result<TargetResource> {
        input.name = crate::business::text(&input.name, 1, 256)?;
        require(!id.is_nil() && !unit.is_nil(), "接收群标识无效")?;
        require(
            match channel {
                Channel::Mobile => input.directory_group_id.is_none(),
                Channel::Wecom => input.directory_group_id.is_some_and(|v| !v.is_nil()),
            },
            "接收群身份无效",
        )?;
        self.0.target(actor, channel, unit, id, input).await
    }
    pub async fn resources(
        &self,
        actor: &Actor,
        channel: Channel,
        unit: Uuid,
    ) -> Result<serde_json::Value> {
        self.0.resources(actor, channel, unit).await
    }
    pub async fn save(
        &self,
        actor: &Actor,
        channel: Channel,
        unit: Uuid,
        id: Uuid,
        mut input: ConfigurationInput,
    ) -> Result<Configuration> {
        require(!id.is_nil() && !unit.is_nil(), "接收配置标识无效")?;
        input.validate(channel)?;
        self.0.save(actor, channel, unit, id, input).await
    }
    pub async fn list(
        &self,
        actor: &Actor,
        channel: Channel,
        unit: Uuid,
        page: Page,
    ) -> Result<Vec<Configuration>> {
        page.fetch_bounds()?;
        self.0.list(actor, channel, unit, page).await
    }
    pub async fn get(
        &self,
        actor: &Actor,
        channel: Channel,
        unit: Uuid,
        id: Uuid,
    ) -> Result<Configuration> {
        self.0.get(actor, channel, unit, id).await
    }
}
impl Application {
    pub fn receiving_configurations(&self) -> Service {
        Service(Repository(self.pool.clone()))
    }
}
