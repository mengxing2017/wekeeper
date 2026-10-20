use std::{net::SocketAddr, time::Duration};

use clap::Parser;
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions, PgSslMode},
};

#[derive(Parser)]
#[command(name = "wekeeper-api", version, about = "WeKeeper HTTP API")]
pub struct Config {
    /// Read a new administrator password from stdin; exits without starting HTTP.
    #[arg(long)]
    pub create_admin: Option<String>,

    #[arg(long, env = "WEKEEPER_BIND_ADDR", default_value = "0.0.0.0:8080")]
    pub bind_addr: SocketAddr,
    #[arg(long, env = "WEKEEPER_DATABASE_URL", hide_env_values = true)]
    database_url: Option<String>,
    #[arg(long, env = "WEKEEPER_DATABASE_HOST", default_value = "127.0.0.1")]
    database_host: String,
    #[arg(long, env = "WEKEEPER_DATABASE_PORT", default_value_t = 5432)]
    database_port: u16,
    #[arg(long, env = "WEKEEPER_DATABASE_NAME", default_value = "wekeeper")]
    database_name: String,
    #[arg(long, env = "WEKEEPER_DATABASE_USER", default_value = "wekeeper")]
    database_user: String,
    #[arg(
        long,
        env = "WEKEEPER_DATABASE_PASSWORD",
        hide_env_values = true,
        required_unless_present = "database_url"
    )]
    database_password: Option<String>,
    #[arg(long, env = "WEKEEPER_DATABASE_SSL_MODE", default_value = "prefer")]
    database_ssl_mode: PgSslMode,
    #[arg(long, env = "WEKEEPER_DATABASE_MAX_CONNECTIONS", default_value_t = 10, value_parser = clap::value_parser!(u32).range(1..=100))]
    database_max_connections: u32,
}

impl Config {
    pub async fn connect(&self) -> Result<PgPool, sqlx::Error> {
        let options = match &self.database_url {
            Some(url) => url.parse::<PgConnectOptions>()?,
            None => PgConnectOptions::new()
                .host(&self.database_host)
                .port(self.database_port)
                .database(&self.database_name)
                .username(&self.database_user)
                .password(self.database_password.as_deref().unwrap_or_default())
                .ssl_mode(self.database_ssl_mode),
        };
        PgPoolOptions::new()
            .max_connections(self.database_max_connections)
            .acquire_timeout(Duration::from_secs(5))
            .connect_with(options)
            .await
    }
}
