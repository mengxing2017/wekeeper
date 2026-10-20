use std::net::SocketAddr;

use sqlx::PgPool;
use tracing::info;

use crate::{bootstrap, database, http};

/// Shared startup path for the production binary and local embedded-PG example.
/// The caller owns the pool and closes it, including when startup fails.
pub async fn serve(pool: PgPool, bind_addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    database::migrate(&pool).await?;
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    info!(address = %listener.local_addr()?, "WeKeeper API listening");
    axum::serve(
        listener,
        http::app_router(bootstrap::build_application(pool)),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl-C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    info!("shutdown requested");
}
