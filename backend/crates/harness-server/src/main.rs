mod app;
mod approvals;
mod audit;
mod auth;
mod config;
mod error;
mod routes;
mod sse;
mod state;
mod transcript;

use std::sync::{Arc, OnceLock};

use anyhow::Context;
use tokio::net::TcpListener;
use tokio::sync::Notify;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::config::Config;
use crate::state::AppState;

/// Process-wide reload signal. Routes (`POST /api/profiles/:id/activate`)
/// fire this to trigger a graceful shutdown + full AppState rebuild without
/// dropping the OS process. Lets the user hot-swap workspaces from the UI
/// without `kill && cargo run` cycles.
static RELOAD_NOTIFY: OnceLock<Arc<Notify>> = OnceLock::new();

pub fn reload_notify() -> Arc<Notify> {
    RELOAD_NOTIFY
        .get_or_init(|| Arc::new(Notify::new()))
        .clone()
}

pub fn trigger_reload() {
    reload_notify().notify_one();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    // Outer loop: each iteration builds a fresh AppState from current env /
    // active_profile pointer. Hot-swap is "reload" → drop state → loop.
    loop {
        let cfg = Config::from_env()?;
        tracing::info!(
            bind = %cfg.bind,
            home = %cfg.home.display(),
            profile = %cfg.profile,
            "starting harness-server"
        );

        let state = Arc::new(AppState::new(&cfg).context("initializing app state")?);
        let router = app::build_router(state.clone(), &cfg);

        // Kick off the periodic tick broadcaster.
        let ticker = sse::hub::spawn_ticker(state.clone());

        let listener = TcpListener::bind(cfg.bind)
            .await
            .with_context(|| format!("binding {}", cfg.bind))?;

        // Shutdown driver — distinguishes ctrl-c (true terminate) from a
        // reload notify (drop state and re-loop). We use a small flag set
        // before the shutdown future completes; the outer loop reads it.
        let reload = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reload_for_signal = reload.clone();
        let notify = reload_notify();

        axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {
                        tracing::info!("ctrl-c received, terminating");
                    }
                    _ = notify.notified() => {
                        reload_for_signal.store(true, std::sync::atomic::Ordering::SeqCst);
                        tracing::info!("reload signal received, will rebuild app state");
                    }
                }
            })
            .await?;

        ticker.stop();

        // Kill all live sessions before tearing down state. This explicit
        // path is the lifecycle owner for subprocess shutdown; dropping
        // AppState alone is not enough to reap PTY children.
        for sid in state.manager.shutdown_all().await {
            state.cleanup_session_resources(&sid);
        }

        if !reload.load(std::sync::atomic::Ordering::SeqCst) {
            return Ok(());
        }
        // Tiny pause so in-flight HTTP responses to the activate endpoint can
        // flush before we rebuild — otherwise the client sees a connection
        // reset instead of the JSON ack.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        tracing::info!("rebuilding harness-server state");
        drop(state);
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,harness_server=info,harness_core=info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().json().with_writer(std::io::stderr))
        .init();
}
