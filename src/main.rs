use std::net::SocketAddr;

use tracing_subscriber::EnvFilter;
use wasmer_sqlx_demo::{build_router, connect, ensure_schema_and_seed, load_settings};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let settings = load_settings()?;
    tracing::info!(
        host = %settings.db.host,
        port = settings.db.port,
        database = %settings.db.database,
        ssl_mode = ?settings.db.ssl_mode,
        "opening MySQL pool with SQLx"
    );

    let state = connect(&settings.db).await?;
    ensure_schema_and_seed(&state).await?;

    let app = build_router(state);
    let addr = SocketAddr::new(settings.bind_ip, settings.port);
    tracing::info!("listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
