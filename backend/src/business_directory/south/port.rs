use crate::business_directory::domain::{BusinessPlatform, BusinessUnit, Result};
use uuid::Uuid;

pub trait UnitStore: Send + Sync {
    fn find(
        &self,
        platform_id: Uuid,
        unit_id: Uuid,
    ) -> impl std::future::Future<Output = Result<Option<BusinessUnit>>> + Send;

    fn list_platforms(
        &self,
        offset: i64,
    ) -> impl std::future::Future<Output = Result<Vec<BusinessPlatform>>> + Send;

    fn list_units(
        &self,
        platform_id: Uuid,
        offset: i64,
    ) -> impl std::future::Future<Output = Result<Vec<BusinessUnit>>> + Send;
}
