use serde::Deserialize;
use slint::{ModelRc, SharedString, VecModel};
use std::rc::Rc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

slint::include_modules!();

const PROTOCOL_VERSION: &str = "1.0";

#[derive(Debug, Clone, Deserialize)]
struct ThreadWithSessions {
    id: String,
    title: Option<String>,
    sessions: Vec<SessionMeta>,
}

#[derive(Debug, Clone, Deserialize)]
struct SessionMeta {
    id: String,
    kind: String,
    status: String,
    pid: i64,
    started_at: i64,
    role: Option<String>,
    task_id: Option<String>,
    scopes: Option<Vec<String>>,
    parent_session_id: Option<String>,
    root_session_id: Option<String>,
    detected_state: Option<String>,
}

#[derive(Debug, Clone)]
struct Config {
    base_url: String,
    session_id_filter: Option<String>,
    poll_ms: u64,
}

fn main() -> Result<(), slint::PlatformError> {
    let config = parse_args();
    let ui = AgentsWindow::new()?;
    ui.set_backend_url(config.base_url.clone().into());
    ui.set_scope_label(
        config
            .session_id_filter
            .as_deref()
            .map(|id| format!("root/session filter {id}"))
            .unwrap_or_else(|| "all threads".to_string())
            .into(),
    );
    ui.set_last_updated("not loaded".into());

    let ui_weak = ui.as_weak();
    let refresh_config = config.clone();
    ui.on_refresh(move || {
        let ui_weak = ui_weak.clone();
        let config = refresh_config.clone();
        std::thread::spawn(move || {
            let result = fetch_agents(&config);
            let _ = ui_weak.upgrade_in_event_loop(move |ui| apply_result(&ui, result));
        });
    });

    let ui_weak = ui.as_weak();
    ui.on_open_agent(move |id| {
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_selected_agent(id);
        }
    });

    spawn_poll_loop(ui.as_weak(), config);
    ui.run()
}

fn parse_args() -> Config {
    let mut base_url =
        std::env::var("HARNESS_URL").unwrap_or_else(|_| "http://127.0.0.1:7777".to_string());
    let mut session_id_filter = std::env::var("HARNESS_SESSION_ID")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let mut poll_ms = 1500;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--base-url" => {
                if let Some(value) = args.next() {
                    base_url = value;
                }
            }
            "--session-id" => {
                if let Some(value) = args.next() {
                    session_id_filter = Some(value);
                }
            }
            "--poll-ms" => {
                if let Some(value) = args.next() {
                    poll_ms = value.parse().unwrap_or(poll_ms);
                }
            }
            "--help" | "-h" => {
                eprintln!(
                    "Usage: harness-slint-agents [--base-url http://127.0.0.1:7777] [--session-id <root-or-parent-sid>]"
                );
                std::process::exit(0);
            }
            _ => {}
        }
    }
    Config {
        base_url,
        session_id_filter,
        poll_ms,
    }
}

fn spawn_poll_loop(ui_weak: slint::Weak<AgentsWindow>, config: Config) {
    std::thread::spawn(move || loop {
        let result = fetch_agents(&config);
        let _ = ui_weak.upgrade_in_event_loop(move |ui| apply_result(&ui, result));
        std::thread::sleep(Duration::from_millis(config.poll_ms));
    });
}

fn fetch_agents(config: &Config) -> Result<Vec<AgentRow>, String> {
    let url = format!("{}/api/threads", config.base_url.trim_end_matches('/'));
    let threads = ureq::get(&url)
        .set("X-Protocol-Version", PROTOCOL_VERSION)
        .call()
        .map_err(|err| err.to_string())?
        .into_json::<Vec<ThreadWithSessions>>()
        .map_err(|err| err.to_string())?;

    let mut rows = Vec::new();
    for thread in threads {
        let title = thread
            .title
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| thread.id.clone());
        for session in thread.sessions {
            if let Some(filter) = &config.session_id_filter {
                let root = session.root_session_id.as_deref().unwrap_or(&session.id);
                let parent = session.parent_session_id.as_deref().unwrap_or("");
                if session.id != *filter && root != filter && parent != filter {
                    continue;
                }
            }
            rows.push((session.started_at, to_agent_row(&title, session)));
        }
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(rows.into_iter().map(|(_, row)| row).collect())
}

fn apply_result(ui: &AgentsWindow, result: Result<Vec<AgentRow>, String>) {
    match result {
        Ok(rows) => {
            let running = rows.iter().filter(|row| row.status == "running").count();
            ui.set_running_count(running as i32);
            ui.set_total_count(rows.len() as i32);
            ui.set_agents(ModelRc::from(Rc::new(VecModel::from(rows))));
            ui.set_last_updated(format!("updated {}", now_label()).into());
        }
        Err(err) => {
            ui.set_last_updated(format!("error: {err}").into());
        }
    }
}

fn to_agent_row(thread_title: &str, session: SessionMeta) -> AgentRow {
    let scopes = session
        .scopes
        .filter(|values| !values.is_empty())
        .map(|values| format!(" · {}", values.join(", ")))
        .unwrap_or_default();
    let task_id = session
        .task_id
        .filter(|value| !value.is_empty())
        .map(|value| format!("task {value}"))
        .unwrap_or_default();
    let parent = session
        .parent_session_id
        .as_ref()
        .map(|id| format!("child of {}", short_id(id)))
        .unwrap_or_else(|| "root session".to_string());
    let state = session
        .detected_state
        .filter(|value| value != "unknown")
        .unwrap_or_default();
    AgentRow {
        id: SharedString::from(session.id.clone()),
        short_id: SharedString::from(short_id(&session.id)),
        role: SharedString::from(session.role.unwrap_or_else(|| "(no role)".to_string())),
        kind: SharedString::from(session.kind),
        status: SharedString::from(session.status),
        pid: SharedString::from(session.pid.to_string()),
        task_id: SharedString::from(task_id),
        scopes: SharedString::from(scopes),
        started: SharedString::from(started_label(session.started_at)),
        thread: SharedString::from(thread_title.to_string()),
        parent: SharedString::from(parent),
        state: SharedString::from(state),
    }
}

fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

fn started_label(started_at_ms: i64) -> String {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(started_at_ms);
    let diff_secs = ((now_ms - started_at_ms).max(0) / 1000) as u64;
    if diff_secs < 60 {
        format!("{diff_secs}s ago")
    } else if diff_secs < 3600 {
        format!("{}m ago", diff_secs / 60)
    } else {
        format!("{}h ago", diff_secs / 3600)
    }
}

fn now_label() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    format!("{secs}")
}
