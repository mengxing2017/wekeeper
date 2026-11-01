use crate::{
    business::{Actor, Result},
    distribution::domain::content::{Material, SourceReference},
};
use std::future::Future;
use uuid::Uuid;
/// A source grants material use, never platform authorization or human confirmation.
pub trait ContentSource: Clone + Send + Sync + 'static {
    fn check(
        &self,
        school: Uuid,
        references: &[SourceReference],
    ) -> impl Future<Output = Result<()>> + Send;
    fn read(
        &self,
        actor: &Actor,
        school: Uuid,
        references: &[SourceReference],
    ) -> impl Future<Output = Result<Vec<Material>>> + Send;
}
