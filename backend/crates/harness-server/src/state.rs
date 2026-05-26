use std::sync::Arc;

use harness_core::{AgentsRegistry, Scheduler, TaskStore};

#[derive(Clone)]
pub struct AppState {
    pub tasks: Arc<TaskStore>,
    pub agents: Arc<AgentsRegistry>,
    #[allow(dead_code)]
    pub scheduler: Arc<Scheduler>,
}
