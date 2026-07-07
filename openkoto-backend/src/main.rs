use std::sync::Arc;

use chrono::Utc;
use openkoto_backend::{
    config::AppConfig,
    database::connect_and_migrate,
    error::AppError,
    routes::{build_router, AppState},
};
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> Result<(), AppError> {
    init_tracing();
    load_dotenv();

    let config = AppConfig::from_env()?;
    let pool = connect_and_migrate(&config).await?;
    let bind_addr = config.bind_addr;
    let state = AppState {
        pool,
        config: Arc::new(config),
        started_at: Utc::now(),
    };
    let app = build_router(state).layer(TraceLayer::new_for_http());
    let listener = TcpListener::bind(bind_addr).await?;

    tracing::info!("OpenKoto backend listening on http://{bind_addr}");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

fn load_dotenv() {
    if dotenvy::dotenv().is_err() {
        let _ = dotenvy::from_filename("openkoto-backend/.env");
    }
}

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("openkoto_backend=info,tower_http=info"));
    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
