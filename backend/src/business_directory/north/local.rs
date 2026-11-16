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
        page: &crate::business::Page,
    ) -> Result<DirectoryPage<BusinessPlatform>> {
        let (limit, offset) = management_bounds(actor, page)?;
        let mut items = self.0.list_platforms(limit, offset).await?;
        let has_more = items.len() >= limit as usize;
        items.truncate(limit as usize - 1);
        Ok(DirectoryPage { items, has_more })
    }

    pub async fn units(
        &self,
        actor: &crate::business::Actor,
        platform_id: Uuid,
        page: &crate::business::Page,
    ) -> Result<DirectoryPage<BusinessUnit>> {
        let (limit, offset) = management_bounds(actor, page)?;
        let mut items = self.0.list_units(platform_id, limit, offset).await?;
        let has_more = items.len() >= limit as usize;
        items.truncate(limit as usize - 1);
        Ok(DirectoryPage { items, has_more })
    }
}

fn management_bounds(
    actor: &crate::business::Actor,
    page: &crate::business::Page,
) -> Result<(i64, i64)> {
    if !actor.platform_admin {
        return Err(DirectoryError::Forbidden);
    }
    page.fetch_bounds().map_err(|_| DirectoryError::InvalidPage)
}
