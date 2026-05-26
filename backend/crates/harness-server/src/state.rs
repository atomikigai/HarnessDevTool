use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use harness_core::Store;
use tokio::sync::broadcast;

use crate::config::Config;

/// Shared application state.
#[derive(Debug)]
pub struct AppState {
    pub store: Arc<Store>,
    pub start_time: Instant,
    pub version: &'static str,
    pub tick_tx: broadcast::Sender<String>,
}

impl AppState {
    pub fn new(cfg: &Config) -> Result<Self> {
        let store = Arc::new(Store::new(&cfg.home)?);
        let (tick_tx, _) = broadcast::channel(64);
        Ok(Self {
            store,
            start_time: Instant::now(),
            version: env!("CARGO_PKG_VERSION"),
            tick_tx,
        })
    }
}
