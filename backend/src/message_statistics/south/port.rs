use crate::{
    business::{Actor, Result},
    message_statistics::domain::*,
};
use uuid::Uuid;
pub trait StatisticsStore: Send + Sync {
    fn create(
        &self,
        actor: &Actor,
        filter: Filter,
    ) -> impl std::future::Future<Output = Result<Overview>> + Send;
    fn overview(
        &self,
        actor: &Actor,
        id: Uuid,
    ) -> impl std::future::Future<Output = Result<Overview>> + Send;
    fn details(
        &self,
        actor: &Actor,
        id: Uuid,
        filter: DetailFilter,
    ) -> impl std::future::Future<Output = Result<DetailPage>> + Send;
    fn export(
        &self,
        actor: &Actor,
        id: Uuid,
    ) -> impl std::future::Future<Output = Result<Export>> + Send;
}
