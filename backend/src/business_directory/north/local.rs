use crate::business_directory::{
    domain::{BusinessPlatform, BusinessUnit, DirectoryError, DirectoryPage, Result},
    south::port::UnitStore,
};
use uuid::Uuid;

/// Internal directory use case; the caller must already be authorized for the platform.
pub struct UnitDirectory<S>(pub S);

impl<S: UnitStore> UnitDirectory<S> {
    pub async fn find(&self, platform_id: Uuid, unit_id: Uuid) -> Result<Option<BusinessUnit>> {
        self.0.find(platform_id, unit_id).await
    }

    pub async fn platforms(
        &self,
        actor: &crate::business::Actor,
        page: i64,
    ) -> Result<DirectoryPage<BusinessPlatform>> {
        let offset = management_offset(actor, page)?;
        let mut items = self.0.list_platforms(offset).await?;
        let has_more = items.len() > 30;
        items.truncate(30);
        Ok(DirectoryPage { items, has_more })
    }

    pub async fn units(
        &self,
        actor: &crate::business::Actor,
        platform_id: Uuid,
        page: i64,
    ) -> Result<DirectoryPage<BusinessUnit>> {
        let offset = management_offset(actor, page)?;
        let mut items = self.0.list_units(platform_id, offset).await?;
        let has_more = items.len() > 30;
        items.truncate(30);
        Ok(DirectoryPage { items, has_more })
    }
}

fn management_offset(actor: &crate::business::Actor, page: i64) -> Result<i64> {
    if !actor.platform_admin {
        return Err(DirectoryError::Forbidden);
    }
    if !(1..=100_000).contains(&page) {
        return Err(DirectoryError::InvalidPage);
    }
    Ok((page - 1) * 30)
}
