use crate::bootstrap::Application;
use axum::Router;
mod scoped_materials;
pub fn router() -> Router<Application> {
    scoped_materials::router()
}
