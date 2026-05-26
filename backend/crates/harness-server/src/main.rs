use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use harness_core::{AgentsRegistry, Scheduler, TaskStore};
use tracing_subscriber::EnvFilter;

mod error;
mod routes;
mod state;

use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let home = harness_home();
    std::fs::create_dir_all(&home)?;
    tracing::info!(?home, "harness home");

    let task_store = TaskStore::new(&home)?;
    let agents = AgentsRegistry::new(&home)?;
    // Scheduler is kept alive by the AppState so its drop runs on shutdown.
    let scheduler = Scheduler::spawn(task_store.clone());

    let state = AppState {
        tasks: Arc::new(task_store),
        agents: Arc::new(agents),
        scheduler: Arc::new(scheduler),
    };

    let app = Router::new()
        .merge(routes::router())
        .with_state(state)
        .layer(tower_http::trace::TraceLayer::new_for_http());

    let port: u16 = std::env::var("HARNESS_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(7878);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    tracing::info!(%addr, "listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn harness_home() -> PathBuf {
    if let Ok(s) = std::env::var("HARNESS_HOME") {
        return PathBuf::from(s);
    }
    let home = dirs_home().unwrap_or_else(|| PathBuf::from("."));
    home.join(".harness")
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}
