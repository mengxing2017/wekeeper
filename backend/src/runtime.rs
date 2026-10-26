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
    let directory = application.integrations.directory();
    let interval = std::env::var("WEKEEPER_WECOM_DIRECTORY_INTERVAL_SECONDS")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(1800);
    if interval != 0 && interval < 60 {
        return Err("directory interval must be 0 or at least 60 seconds".into());
    }
    tracing::info!(
        server_configured = directory.server_configured,
        interval_seconds = interval,
        poll_seconds = 5,
        "wecom directory worker initialized; interval 0 disables scheduled sync only"
    );
    let worker = tokio::spawn(async move {
        loop {
            if directory.server_configured
                && let Err(error) = directory.run_once(interval).await
            {
                tracing::warn!(
                    error_code = crate::integrations::north::directory::error_code(&error),
                    "wecom directory worker failed; inspect task logs and directory configuration"
                );
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });
    let assets = application.integrations.assets();
    let assets_interval = std::env::var("WEKEEPER_WECOM_ASSET_SYNC_INTERVAL_SECONDS")
        .ok()
        .map(|v| v.parse::<i64>())
        .transpose()?
        .unwrap_or(1800);
    if assets_interval != 0 && assets_interval < 60 {
        worker.abort();
        return Err("asset interval must be 0 or at least 60 seconds".into());
    }
    tracing::info!(
        interval_seconds = assets_interval,
        "wecom asset worker initialized"
    );
    let assets_worker = tokio::spawn(async move {
        loop {
            if assets.integration.server_configured()
                && let Err(e) = assets.run_once(assets_interval).await
            {
                tracing::warn!(
                    error_code = crate::integrations::north::directory::error_code(&e),
                    "wecom asset worker failed"
                );
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });
    let broadcast = application.broadcast();
    let broadcast_worker = tokio::spawn(async move {
        loop {
            if broadcast.run_once().await.is_err() {
                tracing::warn!("broadcast worker failed; inspect persisted task state");
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });
    let mobile = application.mobile_delivery();
    let mobile_worker = tokio::spawn(async move {
        loop {
            if mobile.recover().await.is_err() {
                tracing::warn!("mobile delivery recovery failed; inspect persisted state");
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });
    let result = axum::serve(listener, http::app_router(application))
        .with_graceful_shutdown(shutdown_signal())
        .await;
    mobile_worker.abort();
    let _ = mobile_worker.await;
    broadcast_worker.abort();
    let _ = broadcast_worker.await;
    assets_worker.abort();
    let _ = assets_worker.await;
    worker.abort();
    let _ = worker.await;
    result?;
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
