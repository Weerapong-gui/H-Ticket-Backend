use h_ticket_backend::{AppState, build_app, config::Config, error::set_contact_info};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=info".into()),
        )
        .init();

    let config = Config::from_env()?;
    set_contact_info(config.contact_info.clone());

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    tokio::fs::create_dir_all(&config.slip_dir).await?;

    let addr = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("listening on {addr}");
    let app = build_app(AppState { pool, config: Arc::new(config) });
    axum::serve(listener, app).await?;
    Ok(())
}
