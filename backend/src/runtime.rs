use std::net::SocketAddr;

use sqlx::PgPool;
use tracing::info;

use crate::{bootstrap, database, http};

/// Shared startup path for the production binary and local embedded-PG example.
/// The caller owns the pool and closes it, including when startup fails.
pub async fn serve(pool: PgPool, bind_addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    serve_inner(pool, bind_addr, false).await
}

/// Local temporary-database entry; automatically provisions a development admin.
pub async fn serve_embedded(
    pool: PgPool,
    bind_addr: SocketAddr,
) -> Result<(), Box<dyn std::error::Error>> {
    if !bind_addr.ip().is_loopback() {
        return Err("embedded development API must bind to a loopback address".into());
    }
    serve_inner(pool, bind_addr, true).await
}

async fn serve_inner(
    pool: PgPool,
    bind_addr: SocketAddr,
    embedded: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    database::migrate(&pool).await?;
    let mut application = bootstrap::build_application(pool.clone());
    let configured_password = std::env::var("WEKEEPER_DEV_ADMIN_PASSWORD").ok();
    let generated_password = (embedded && configured_password.is_none())
        .then(|| uuid::Uuid::new_v4().simple().to_string());
    if embedded {
        application.options.insecure_local_auth = true;
    }
    if let Some(password) = configured_password.as_ref().or(generated_password.as_ref()) {
        if !application.options.insecure_local_auth {
            return Err("development bootstrap requires WEKEEPER_INSECURE_LOCAL_AUTH=true".into());
        }
        if application.identity.0.account("admin").await?.is_none() {
            application
                .identity
                .bootstrap("admin", "本地管理员", password, true)
                .await?;
        }
    }
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    if embedded {
        eprintln!("本地管理后台：{}/admin", application.options.public_origin);
        eprintln!("管理员用户名：admin");
        if let Some(password) = generated_password {
            eprintln!("本次临时管理员密码：{password}");
        } else {
            eprintln!("管理员密码：使用 WEKEEPER_DEV_ADMIN_PASSWORD 设置的值");
        }
    }
    info!(address = %listener.local_addr()?, "WeKeeper API listening");
    axum::serve(listener, http::app_router(application))
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
