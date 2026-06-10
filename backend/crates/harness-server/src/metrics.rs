use std::sync::Arc;

use axum::extract::State;
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use harness_core::TaskStatus;
use harness_session::SessionStatus;
use sha2::{Digest, Sha256};

use crate::state::AppState;

const CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";
const MAX_SESSION_PRESSURE_SERIES: usize = 100;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/metrics", get(metrics))
}

async fn metrics(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, CONTENT_TYPE)],
        render_metrics(&state).await,
    )
}

async fn render_metrics(state: &AppState) -> String {
    let mut out = String::new();
    append_help(
        &mut out,
        "harness_sessions_live",
        "Live agent sessions currently owned by this process.",
        "gauge",
    );
    append_sample(
        &mut out,
        "harness_sessions_live",
        &[],
        state.manager.all().len(),
    );

    append_help(
        &mut out,
        "harness_sessions_total_by_status",
        "Known sessions by persisted lifecycle status.",
        "gauge",
    );
    let mut session_counts = vec![0usize; known_session_statuses().len()];
    for meta in state.manager.list_metas().await {
        session_counts[session_status_index(meta.status)] += 1;
    }
    for (idx, status) in known_session_statuses().into_iter().enumerate() {
        append_sample(
            &mut out,
            "harness_sessions_total_by_status",
            &[("status", session_status_label(status))],
            session_counts[idx],
        );
    }

    append_help(
        &mut out,
        "harness_tasks_by_state",
        "Scheduler in-memory task snapshot by lifecycle state.",
        "gauge",
    );
    let mut task_counts = vec![0usize; known_task_statuses().len()];
    match state.tasks.scheduler_threads() {
        Ok(threads) => {
            for tid in threads {
                match state.tasks.scheduler_snapshot(&tid) {
                    Ok(tasks) => {
                        for task in tasks {
                            task_counts[task_status_index(task.status)] += 1;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(thread_id = %tid, error = %e, "metrics task snapshot failed")
                    }
                }
            }
        }
        Err(e) => tracing::warn!(error = %e, "metrics scheduler thread snapshot failed"),
    }
    for (idx, status) in known_task_statuses().into_iter().enumerate() {
        append_sample(
            &mut out,
            "harness_tasks_by_state",
            &[("state", status.as_str())],
            task_counts[idx],
        );
    }

    append_context_pressure_metrics(&mut out, state);

    append_help(
        &mut out,
        "harness_sse_lagged_total",
        "Lagged SSE or PTY lag frames emitted by this process.",
        "counter",
    );
    append_sample(
        &mut out,
        "harness_sse_lagged_total",
        &[],
        state.sse_lagged_total(),
    );

    append_help(
        &mut out,
        "harness_build_info",
        "Build information for this harness-server process.",
        "gauge",
    );
    append_sample(
        &mut out,
        "harness_build_info",
        &[("version", state.version)],
        1,
    );

    out
}

fn append_context_pressure_metrics(out: &mut String, state: &AppState) {
    let live_sessions = state.manager.all();
    let mut pressures = Vec::new();
    for session in &live_sessions {
        if let Some(pressure) = crate::context_governor::latest_context_pressure(session.id()) {
            pressures.push((session.id().to_string(), pressure));
        }
    }
    if live_sessions.len() <= MAX_SESSION_PRESSURE_SERIES {
        if pressures.is_empty() {
            return;
        }
        append_help(
            out,
            "harness_context_pressure",
            "Latest known context pressure ratio for live sessions.",
            "gauge",
        );
        for (session_id, pressure) in &pressures {
            // /metrics is public, so expose a stable opaque session label
            // instead of the real persisted session id.
            let session_hash = opaque_session_label(session_id);
            append_sample(
                out,
                "harness_context_pressure",
                &[("session_hash", session_hash.as_str())],
                *pressure,
            );
        }
        return;
    }

    append_help(
        out,
        "harness_context_pressure_avg",
        "Average latest context pressure ratio across live sessions.",
        "gauge",
    );
    append_help(
        out,
        "harness_context_pressure_max",
        "Maximum latest context pressure ratio across live sessions.",
        "gauge",
    );
    if pressures.is_empty() {
        append_sample(out, "harness_context_pressure_avg", &[], 0);
        append_sample(out, "harness_context_pressure_max", &[], 0);
        return;
    }
    let sum = pressures.iter().map(|(_, pressure)| *pressure).sum::<f64>();
    let max = pressures
        .iter()
        .map(|(_, pressure)| *pressure)
        .fold(f64::NEG_INFINITY, f64::max);
    append_sample(
        out,
        "harness_context_pressure_avg",
        &[],
        sum / pressures.len() as f64,
    );
    append_sample(out, "harness_context_pressure_max", &[], max);
}

fn opaque_session_label(session_id: &str) -> String {
    let digest = Sha256::digest(session_id.as_bytes());
    digest[..4]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn append_help(out: &mut String, name: &str, help: &str, metric_type: &str) {
    out.push_str("# HELP ");
    out.push_str(name);
    out.push(' ');
    out.push_str(help);
    out.push('\n');
    out.push_str("# TYPE ");
    out.push_str(name);
    out.push(' ');
    out.push_str(metric_type);
    out.push('\n');
}

fn append_sample<T: MetricValue>(out: &mut String, name: &str, labels: &[(&str, &str)], value: T) {
    out.push_str(name);
    if !labels.is_empty() {
        out.push('{');
        for (idx, (key, value)) in labels.iter().enumerate() {
            if idx > 0 {
                out.push(',');
            }
            out.push_str(key);
            out.push_str("=\"");
            push_escaped_label(out, value);
            out.push('"');
        }
        out.push('}');
    }
    out.push(' ');
    value.push_metric_value(out);
    out.push('\n');
}

trait MetricValue {
    fn push_metric_value(self, out: &mut String);
}

impl MetricValue for usize {
    fn push_metric_value(self, out: &mut String) {
        out.push_str(&self.to_string());
    }
}

impl MetricValue for u64 {
    fn push_metric_value(self, out: &mut String) {
        out.push_str(&self.to_string());
    }
}

impl MetricValue for i32 {
    fn push_metric_value(self, out: &mut String) {
        out.push_str(&self.to_string());
    }
}

impl MetricValue for f64 {
    fn push_metric_value(self, out: &mut String) {
        if self.is_finite() {
            out.push_str(&format!("{self:.6}"));
        } else {
            out.push('0');
        }
    }
}

fn push_escaped_label(out: &mut String, value: &str) {
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            _ => out.push(ch),
        }
    }
}

fn known_session_statuses() -> [SessionStatus; 3] {
    [
        SessionStatus::Running,
        SessionStatus::Exited,
        SessionStatus::Killed,
    ]
}

fn session_status_label(status: SessionStatus) -> &'static str {
    match status {
        SessionStatus::Running => "running",
        SessionStatus::Exited => "exited",
        SessionStatus::Killed => "killed",
    }
}

fn session_status_index(status: SessionStatus) -> usize {
    match status {
        SessionStatus::Running => 0,
        SessionStatus::Exited => 1,
        SessionStatus::Killed => 2,
    }
}

fn known_task_statuses() -> [TaskStatus; 8] {
    [
        TaskStatus::Proposed,
        TaskStatus::Queued,
        TaskStatus::InProgress,
        TaskStatus::PendingVerify,
        TaskStatus::Done,
        TaskStatus::Paused,
        TaskStatus::Blocked,
        TaskStatus::Abandoned,
    ]
}

fn task_status_index(status: TaskStatus) -> usize {
    match status {
        TaskStatus::Proposed => 0,
        TaskStatus::Queued => 1,
        TaskStatus::InProgress => 2,
        TaskStatus::PendingVerify => 3,
        TaskStatus::Done => 4,
        TaskStatus::Paused => 5,
        TaskStatus::Blocked => 6,
        TaskStatus::Abandoned => 7,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app;
    use crate::config::Config;
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    fn state(home: std::path::PathBuf) -> Arc<AppState> {
        Arc::new(
            AppState::new(&Config {
                bind: "127.0.0.1:7777".parse().unwrap(),
                home,
                cors_origin: "http://localhost:8080".to_string(),
                profile: "default".to_string(),
                autonomy_profile: harness_core::AutonomyProfile::Assisted,
                api_token: Some("secret".to_string()),
                evolution: Default::default(),
            })
            .unwrap(),
        )
    }

    #[tokio::test]
    async fn metrics_route_is_public_text_exposition() {
        let dir = tempfile::tempdir().unwrap();
        let state = state(dir.path().to_path_buf());
        state.record_sse_lagged();
        let cfg = Config {
            bind: "127.0.0.1:7777".parse().unwrap(),
            home: dir.path().to_path_buf(),
            cors_origin: "http://localhost:8080".to_string(),
            profile: "default".to_string(),
            autonomy_profile: harness_core::AutonomyProfile::Assisted,
            api_token: Some("secret".to_string()),
            evolution: Default::default(),
        };
        let app = app::build_router(state, &cfg);

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(content_type.starts_with("text/plain; version=0.0.4"));
        let body = String::from_utf8(
            to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(body.contains("harness_sessions_live 0\n"));
        assert!(body.contains("harness_tasks_by_state{state=\"queued\"} 0\n"));
        assert!(body.contains("harness_sse_lagged_total 1\n"));
        assert!(body.contains("harness_build_info{version=\""));
        assert_samples_are_parseable(&body);
    }

    #[test]
    fn label_values_are_escaped() {
        let mut out = String::new();
        append_sample(
            &mut out,
            "harness_build_info",
            &[("version", "a\"b\\c\nd")],
            1,
        );

        assert_eq!(out, "harness_build_info{version=\"a\\\"b\\\\c\\nd\"} 1\n");
    }

    #[tokio::test]
    async fn sse_lagged_counter_increments() {
        let dir = tempfile::tempdir().unwrap();
        let state = state(dir.path().to_path_buf());

        state.record_sse_lagged();
        state.record_sse_lagged();

        assert_eq!(state.sse_lagged_total(), 2);
    }

    fn assert_samples_are_parseable(body: &str) {
        for line in body.lines() {
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            let (metric, value) = line
                .rsplit_once(' ')
                .unwrap_or_else(|| panic!("sample has no value: {line}"));
            assert!(
                sample_name(metric)
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == ':'),
                "bad metric name: {line}"
            );
            assert!(
                value.parse::<f64>().is_ok(),
                "bad metric value in sample: {line}"
            );
            if let Some(labels) = metric
                .split_once('{')
                .and_then(|(_, rest)| rest.strip_suffix('}'))
            {
                for label in labels.split(',') {
                    let (key, raw_value) = label
                        .split_once('=')
                        .unwrap_or_else(|| panic!("bad label: {line}"));
                    assert!(!key.is_empty(), "empty label key: {line}");
                    assert!(
                        raw_value.starts_with('"') && raw_value.ends_with('"'),
                        "unquoted label value: {line}"
                    );
                }
            }
        }
    }

    fn sample_name(metric: &str) -> &str {
        metric
            .split_once('{')
            .map(|(name, _)| name)
            .unwrap_or(metric)
    }
}
