use crate::{
    business::{Actor, Result},
    distribution::domain::content::{Material, SourceReference},
};
use std::future::Future;
/// A source grants material use, never platform authorization or human confirmation.
pub trait ContentSource: Clone + Send + Sync + 'static {
    fn check(&self, references: &[SourceReference]) -> impl Future<Output = Result<()>> + Send;
    fn read(
        &self,
        actor: &Actor,
        references: &[SourceReference],
    ) -> impl Future<Output = Result<Vec<Material>>> + Send;
}
