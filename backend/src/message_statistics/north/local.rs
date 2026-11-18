use crate::{
    business::{Actor, Result},
    message_statistics::{domain::*, south::port::StatisticsStore},
};
use uuid::Uuid;
pub struct Statistics<S>(pub S);
impl<S: StatisticsStore> Statistics<S> {
    pub async fn create(&self, actor: &Actor, filter: Filter) -> Result<Overview> {
        actor.platform()?;
        self.0
            .create(actor, filter.normalize(chrono::Utc::now())?)
            .await
    }
    pub async fn overview(&self, actor: &Actor, id: Uuid) -> Result<Overview> {
        actor.platform()?;
        self.0.overview(actor, id).await
    }
    pub async fn details(
        &self,
        actor: &Actor,
        id: Uuid,
        filter: DetailFilter,
    ) -> Result<DetailPage> {
        actor.platform()?;
        filter.validate()?;
        self.0.details(actor, id, filter).await
    }
    pub async fn export(&self, actor: &Actor, id: Uuid) -> Result<Export> {
        actor.platform()?;
        self.0.export(actor, id).await
    }
}
