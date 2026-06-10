use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Datelike, TimeZone, Utc};
use harness_core::SkillStore;
use tokio::task::JoinHandle;

use crate::config::EvolutionConfig;
use crate::state::AppState;

const PANAMA_UTC_OFFSET_HOURS: u32 = 5;

pub struct EvolutionHandle {
    join: Option<JoinHandle<()>>,
}

impl EvolutionHandle {
    pub fn disabled() -> Self {
        Self { join: None }
    }

    pub fn stop(mut self) {
        if let Some(join) = self.join.take() {
            join.abort();
        }
    }
}

impl Drop for EvolutionHandle {
    fn drop(&mut self) {
        if let Some(join) = &self.join {
            join.abort();
        }
    }
}

pub fn spawn_daily_evolution(state: Arc<AppState>, cfg: EvolutionConfig) -> EvolutionHandle {
    if !cfg.enabled {
        tracing::info!("daily evolution scheduler disabled");
        return EvolutionHandle::disabled();
    }
    tracing::info!(
        hour = cfg.panama_hour,
        minute = cfg.panama_minute,
        idle_only = cfg.idle_only,
        curator_dry_run = cfg.curator_dry_run,
        "daily evolution scheduler enabled"
    );
    let join = tokio::spawn(async move {
        loop {
            let delay =
                duration_until_next_panama_time(Utc::now(), cfg.panama_hour, cfg.panama_minute);
            tracing::info!(
                delay_secs = delay.as_secs(),
                "sleeping until next daily evolution window"
            );
            tokio::time::sleep(delay).await;
            run_once(state.clone(), cfg.clone()).await;
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
    EvolutionHandle { join: Some(join) }
}

async fn run_once(state: Arc<AppState>, cfg: EvolutionConfig) {
    if cfg.idle_only && !state.manager.all().is_empty() {
        tracing::info!(
            live_sessions = state.manager.all().len(),
            "skipping daily evolution because sessions are active"
        );
        return;
    }

    let home = state.harness_home.clone();
    let profile = state.profile.clone();
    let observation_limit = cfg.observation_limit;
    let curator_dry_run = cfg.curator_dry_run;
    let result = tokio::task::spawn_blocking(move || {
        let store = SkillStore::new(&home, &profile)?;
        let learner = store.evolve_run(observation_limit)?;
        let curator = store.curator_run(curator_dry_run)?;
        Ok::<_, harness_core::Error>((learner, curator))
    })
    .await;

    match result {
        Ok(Ok((learner, curator))) => {
            tracing::info!(
                observations_read = learner.observations_read,
                proposals_created = learner.proposals_created.len(),
                stale_candidates = curator.stale_candidates.len(),
                archived = curator.archived.len(),
                curator_dry_run = curator.dry_run,
                "daily evolution completed"
            );
        }
        Ok(Err(e)) => tracing::warn!(error = %e, "daily evolution failed"),
        Err(e) => tracing::warn!(error = %e, "daily evolution task join failed"),
    }
}

fn duration_until_next_panama_time(
    now: DateTime<Utc>,
    panama_hour: u32,
    panama_minute: u32,
) -> Duration {
    let utc_hour = (panama_hour + PANAMA_UTC_OFFSET_HOURS) % 24;
    let today_target = Utc
        .with_ymd_and_hms(
            now.year(),
            now.month(),
            now.day(),
            utc_hour,
            panama_minute,
            0,
        )
        .single()
        .expect("valid UTC daily target");
    let target = if now < today_target {
        today_target
    } else {
        today_target + chrono::Duration::days(1)
    };
    let secs = (target - now).num_seconds().max(1) as u64;
    Duration::from_secs(secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panama_7am_maps_to_noon_utc_same_day() {
        let now = Utc.with_ymd_and_hms(2026, 6, 10, 11, 0, 0).unwrap();
        let delay = duration_until_next_panama_time(now, 7, 0);
        assert_eq!(delay, Duration::from_secs(60 * 60));
    }

    #[test]
    fn panama_7am_rolls_to_next_day_after_noon_utc() {
        let now = Utc.with_ymd_and_hms(2026, 6, 10, 12, 1, 0).unwrap();
        let delay = duration_until_next_panama_time(now, 7, 0);
        assert_eq!(delay, Duration::from_secs(23 * 60 * 60 + 59 * 60));
    }
}
