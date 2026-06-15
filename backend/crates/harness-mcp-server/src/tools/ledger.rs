//! Derived agent ledger rails. Canonical state remains session meta JSON,
//! task handoff JSONL, and context events; SQLite is a compact rebuildable
//! index for orchestrators.

use std::path::{Path, PathBuf};
use std::time::Duration;

use harness_core::{Handoff, TaskStore};
use harness_session::SessionMeta;
use rusqlite::{params, Connection};
use serde_json::{json, Value};

use super::session::{harness_request, opt_str};

pub fn list(
    store: &TaskStore,
    harness_home: &Path,
    profile: &str,
    args: &Value,
) -> Result<Value, String> {
    let conn = refresh_ledger(store, harness_home, profile)?;
    let mut sql = String::from("SELECT payload_json FROM agent_ledger WHERE 1=1");
    let mut params_v = Vec::<rusqlite::types::Value>::new();
    for key in ["root_session_id", "thread_id", "status"] {
        if let Some(value) = opt_str(args, key).filter(|value| !value.trim().is_empty()) {
            sql.push_str(&format!(" AND {key} = ?"));
            params_v.push(rusqlite::types::Value::Text(value.to_string()));
        }
    }
    sql.push_str(" ORDER BY started_at DESC LIMIT ?");
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(50)
        .clamp(1, 200) as i64;
    params_v.push(rusqlite::types::Value::Integer(limit));
    let entries = query_payloads(&conn, &sql, params_v)?;
    Ok(json!({
        "profile": profile,
        "count": entries.len(),
        "entries": entries,
    }))
}

pub fn get(
    store: &TaskStore,
    harness_home: &Path,
    profile: &str,
    args: &Value,
) -> Result<Value, String> {
    let session_id = opt_str(args, "session_id")
        .ok_or_else(|| "agent_ledger_get requires session_id".to_string())?;
    let conn = refresh_ledger(store, harness_home, profile)?;
    let payload: String = conn
        .query_row(
            "SELECT payload_json FROM agent_ledger WHERE session_id = ?1",
            params![session_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("agent_ledger_get: {e}"))?;
    serde_json::from_str(&payload).map_err(|e| format!("agent_ledger_get decode: {e}"))
}

pub fn handoff_latest(
    store: &TaskStore,
    harness_home: &Path,
    profile: &str,
    current_thread_id: &str,
    args: &Value,
) -> Result<Value, String> {
    let (thread_id, task_id) = resolve_thread_task(harness_home, profile, current_thread_id, args)?;
    let handoff = latest_handoff(store, &thread_id, &task_id);
    Ok(json!({
        "thread_id": thread_id,
        "task_id": task_id,
        "handoff": handoff,
    }))
}

pub fn submit_handoff(
    current_session_id: Option<&str>,
    harness_home: &Path,
    profile: &str,
    current_thread_id: &str,
    server_url: Option<&str>,
    api_token: Option<&str>,
    args: &Value,
) -> Result<Value, String> {
    let session_id = opt_str(args, "session_id").or(current_session_id);
    let (thread_id, task_id) = resolve_thread_task_with_default_session(
        harness_home,
        profile,
        current_thread_id,
        args,
        session_id,
    )?;
    let server =
        server_url.ok_or_else(|| "session_handoff_submit needs --server-url".to_string())?;
    let from = opt_str(args, "from")
        .or(session_id)
        .ok_or_else(|| "session_handoff_submit requires from or --session-id".to_string())?;
    let body = json!({
        "from": from,
        "to_role": required_str(args, "to_role")?,
        "status": required_str(args, "status")?,
        "goal": required_str(args, "goal")?,
        "assumptions": string_vec(args, "assumptions"),
        "files_changed": string_vec(args, "files_changed"),
        "commands_run": string_vec(args, "commands_run"),
        "verification_passed": string_vec(args, "verification_passed"),
        "verification_not_run": string_vec(args, "verification_not_run"),
        "blocked_on": string_vec(args, "blocked_on"),
        "next_agent_action": required_str(args, "next_agent_action")?,
    });
    let url = format!(
        "{}/api/threads/{}/tasks/{}/handoffs",
        server.trim_end_matches('/'),
        super::session::encode_query(&thread_id),
        super::session::encode_query(&task_id)
    );
    let req = harness_request(ureq::post(&url).timeout(Duration::from_secs(5)), api_token);
    req.send_json(&body)
        .map_err(|e| e.to_string())?
        .into_json::<Value>()
        .map_err(|e| e.to_string())
}

fn refresh_ledger(
    store: &TaskStore,
    harness_home: &Path,
    profile: &str,
) -> Result<Connection, String> {
    let conn = open_ledger(harness_home, profile)?;
    conn.execute("DELETE FROM agent_ledger", [])
        .map_err(|e| format!("agent_ledger clear: {e}"))?;
    for meta in read_session_metas(harness_home, profile)? {
        let entry = ledger_entry(store, harness_home, profile, &meta);
        upsert_entry(&conn, &entry)?;
    }
    Ok(conn)
}

fn open_ledger(harness_home: &Path, profile: &str) -> Result<Connection, String> {
    let path = harness_home
        .join("profiles")
        .join(profile)
        .join("agent_ledger.sqlite");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("agent_ledger mkdir: {e}"))?;
    }
    let conn = Connection::open(path).map_err(|e| format!("agent_ledger open: {e}"))?;
    conn.execute_batch(
        r#"
        PRAGMA journal_mode=WAL;
        CREATE TABLE IF NOT EXISTS agent_ledger (
            session_id TEXT PRIMARY KEY,
            parent_session_id TEXT,
            root_session_id TEXT NOT NULL,
            thread_id TEXT NOT NULL,
            task_id TEXT,
            role TEXT,
            status TEXT NOT NULL,
            detected_state TEXT,
            objective TEXT NOT NULL,
            latest_pressure REAL,
            latest_checkpoint_seq INTEGER,
            latest_handoff_at INTEGER,
            latest_handoff_status TEXT,
            next_action TEXT,
            payload_json TEXT NOT NULL,
            started_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_agent_ledger_root ON agent_ledger(root_session_id);
        CREATE INDEX IF NOT EXISTS idx_agent_ledger_thread ON agent_ledger(thread_id);
        CREATE INDEX IF NOT EXISTS idx_agent_ledger_status ON agent_ledger(status);
        "#,
    )
    .map_err(|e| format!("agent_ledger schema: {e}"))?;
    Ok(conn)
}

fn ledger_entry(
    store: &TaskStore,
    harness_home: &Path,
    profile: &str,
    meta: &SessionMeta,
) -> Value {
    let task = meta
        .task_id
        .as_deref()
        .and_then(|task_id| store.get(&meta.thread_id, task_id).ok());
    let handoff = meta
        .task_id
        .as_deref()
        .and_then(|task_id| latest_handoff(store, &meta.thread_id, task_id));
    let context = context_summary(harness_home, profile, &meta.id);
    let objective = task
        .as_ref()
        .and_then(|task| {
            task.brief
                .as_ref()
                .map(|brief| brief.objective.trim())
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
        .or_else(|| task.as_ref().map(|task| task.title.clone()))
        .unwrap_or_else(|| {
            format!(
                "{} session in {}",
                meta.role.as_deref().unwrap_or(meta.kind.as_str()),
                meta.cwd
            )
        });
    let next_action = handoff
        .as_ref()
        .and_then(|handoff| handoff.get("next_agent_action"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string);
    json!({
        "session_id": meta.id,
        "parent_session_id": meta.parent_session_id,
        "root_session_id": meta.root_session_id,
        "thread_id": meta.thread_id,
        "task_id": meta.task_id,
        "role": meta.role,
        "scopes": meta.scopes,
        "objective": objective,
        "status": meta.status,
        "detected_state": meta.detected_state,
        "loaded_capabilities": meta.loaded_capabilities,
        "latest_pressure": context.get("latest_pressure").cloned().unwrap_or(Value::Null),
        "latest_checkpoint_seq": context.get("latest_checkpoint_seq").cloned().unwrap_or(Value::Null),
        "latest_handoff": handoff,
        "latest_handoff_at": handoff_field_i64(&handoff, "at"),
        "latest_handoff_status": handoff_field_str(&handoff, "status"),
        "next_action": next_action,
        "blocked_on": handoff.as_ref().and_then(|h| h.get("blocked_on")).cloned().unwrap_or_else(|| json!([])),
        "files_changed": handoff.as_ref().and_then(|h| h.get("files_changed")).cloned().unwrap_or_else(|| json!([])),
        "commands_run": handoff.as_ref().and_then(|h| h.get("commands_run")).cloned().unwrap_or_else(|| json!([])),
        "started_at": meta.started_at,
        "cwd": meta.cwd,
        "has_transcript": meta.has_transcript,
    })
}

fn upsert_entry(conn: &Connection, entry: &Value) -> Result<(), String> {
    let payload = serde_json::to_string(entry).map_err(|e| format!("agent_ledger encode: {e}"))?;
    conn.execute(
        r#"
        INSERT INTO agent_ledger(
            session_id, parent_session_id, root_session_id, thread_id, task_id, role,
            status, detected_state, objective, latest_pressure, latest_checkpoint_seq,
            latest_handoff_at, latest_handoff_status, next_action, payload_json, started_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
        ON CONFLICT(session_id) DO UPDATE SET
            parent_session_id=excluded.parent_session_id,
            root_session_id=excluded.root_session_id,
            thread_id=excluded.thread_id,
            task_id=excluded.task_id,
            role=excluded.role,
            status=excluded.status,
            detected_state=excluded.detected_state,
            objective=excluded.objective,
            latest_pressure=excluded.latest_pressure,
            latest_checkpoint_seq=excluded.latest_checkpoint_seq,
            latest_handoff_at=excluded.latest_handoff_at,
            latest_handoff_status=excluded.latest_handoff_status,
            next_action=excluded.next_action,
            payload_json=excluded.payload_json,
            started_at=excluded.started_at
        "#,
        params![
            str_field(entry, "session_id"),
            opt_str_field(entry, "parent_session_id"),
            str_field(entry, "root_session_id"),
            str_field(entry, "thread_id"),
            opt_str_field(entry, "task_id"),
            opt_str_field(entry, "role"),
            str_field(entry, "status"),
            opt_str_field(entry, "detected_state"),
            str_field(entry, "objective"),
            entry.get("latest_pressure").and_then(Value::as_f64),
            entry.get("latest_checkpoint_seq").and_then(Value::as_i64),
            entry.get("latest_handoff_at").and_then(Value::as_i64),
            opt_str_field(entry, "latest_handoff_status"),
            opt_str_field(entry, "next_action"),
            payload.as_str(),
            entry.get("started_at").and_then(Value::as_i64).unwrap_or(0),
        ],
    )
    .map_err(|e| format!("agent_ledger upsert: {e}"))?;
    Ok(())
}

fn query_payloads(
    conn: &Connection,
    sql: &str,
    args: Vec<rusqlite::types::Value>,
) -> Result<Vec<Value>, String> {
    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| format!("agent_ledger query: {e}"))?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(args.iter()), |row| {
            row.get::<_, String>(0)
        })
        .map_err(|e| format!("agent_ledger query: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        let raw = row.map_err(|e| format!("agent_ledger row: {e}"))?;
        out.push(serde_json::from_str(&raw).map_err(|e| format!("agent_ledger decode: {e}"))?);
    }
    Ok(out)
}

fn latest_handoff(store: &TaskStore, thread_id: &str, task_id: &str) -> Option<Value> {
    store
        .read_handoffs(thread_id, task_id)
        .ok()?
        .into_iter()
        .max_by_key(|handoff| handoff.at)
        .map(handoff_json)
}

fn handoff_json(handoff: Handoff) -> Value {
    json!({
        "at": handoff.at,
        "from": handoff.from,
        "to_role": handoff.to_role,
        "task_id": handoff.task_id,
        "status": handoff.status,
        "goal": handoff.goal,
        "assumptions": handoff.assumptions,
        "blocked_on": handoff.blocked_on,
        "files_changed": handoff.files_changed,
        "commands_run": handoff.commands_run,
        "verification_passed": handoff.verification_passed,
        "verification_not_run": handoff.verification_not_run,
        "next_agent_action": handoff.next_agent_action,
    })
}

fn context_summary(harness_home: &Path, profile: &str, session_id: &str) -> Value {
    let path = harness_home
        .join("profiles")
        .join(profile)
        .join("context.sqlite");
    let Ok(conn) = Connection::open(path) else {
        return json!({});
    };
    let latest_pressure = conn
        .query_row(
            "SELECT pressure FROM context_events WHERE session_id = ?1 AND pressure IS NOT NULL ORDER BY seq DESC LIMIT 1",
            params![session_id],
            |row| row.get::<_, f64>(0),
        )
        .ok();
    let latest_checkpoint_seq = conn
        .query_row(
            "SELECT seq FROM context_events WHERE session_id = ?1 AND event_type LIKE '%checkpoint%' ORDER BY seq DESC LIMIT 1",
            params![session_id],
            |row| row.get::<_, i64>(0),
        )
        .ok();
    json!({
        "latest_pressure": latest_pressure,
        "latest_checkpoint_seq": latest_checkpoint_seq,
    })
}

fn resolve_thread_task(
    harness_home: &Path,
    profile: &str,
    current_thread_id: &str,
    args: &Value,
) -> Result<(String, String), String> {
    resolve_thread_task_with_default_session(harness_home, profile, current_thread_id, args, None)
}

fn resolve_thread_task_with_default_session(
    harness_home: &Path,
    profile: &str,
    current_thread_id: &str,
    args: &Value,
    default_session_id: Option<&str>,
) -> Result<(String, String), String> {
    let mut thread_id = opt_str(args, "thread_id")
        .map(str::to_string)
        .unwrap_or_else(|| current_thread_id.to_string());
    let mut task_id = opt_str(args, "task_id").map(str::to_string);
    if task_id.is_none() {
        if let Some(session_id) = opt_str(args, "session_id").or(default_session_id) {
            let meta = read_session_meta(harness_home, profile, session_id)?;
            thread_id = meta.thread_id;
            task_id = meta.task_id;
        }
    }
    let task_id =
        task_id.ok_or_else(|| "task_id or session_id with task_id is required".to_string())?;
    Ok((thread_id, task_id))
}

fn read_session_meta(
    harness_home: &Path,
    profile: &str,
    session_id: &str,
) -> Result<SessionMeta, String> {
    let path = sessions_dir(harness_home, profile)
        .join(session_id)
        .join("meta.json");
    let bytes = std::fs::read(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("parse {}: {e}", path.display()))
}

fn read_session_metas(harness_home: &Path, profile: &str) -> Result<Vec<SessionMeta>, String> {
    let dir = sessions_dir(harness_home, profile);
    let read = match std::fs::read_dir(&dir) {
        Ok(read) => read,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("read_dir {}: {e}", dir.display())),
    };
    let mut out = Vec::new();
    for entry in read.filter_map(Result::ok) {
        let path = entry.path().join("meta.json");
        if !path.exists() {
            continue;
        }
        match std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<SessionMeta>(&bytes).ok())
        {
            Some(meta) => out.push(meta),
            None => tracing::warn!(path = %path.display(), "skipping unreadable session meta"),
        }
    }
    Ok(out)
}

fn sessions_dir(harness_home: &Path, profile: &str) -> PathBuf {
    harness_home.join("profiles").join(profile).join("sessions")
}

fn required_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    opt_str(args, key)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("session_handoff_submit requires {key}"))
}

fn string_vec(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn str_field<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn opt_str_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn handoff_field_i64(handoff: &Option<Value>, key: &str) -> Value {
    handoff
        .as_ref()
        .and_then(|handoff| handoff.get(key))
        .and_then(Value::as_i64)
        .map(Value::from)
        .unwrap_or(Value::Null)
}

fn handoff_field_str(handoff: &Option<Value>, key: &str) -> Value {
    handoff
        .as_ref()
        .and_then(|handoff| handoff.get(key))
        .and_then(Value::as_str)
        .map(Value::from)
        .unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_core::{Handoff, TaskDraft};
    use std::io::Read;
    use std::io::Write;
    use std::net::TcpListener;
    use std::sync::mpsc;

    fn write_meta(home: &Path, session_id: &str, parent: Option<&str>, task_id: Option<&str>) {
        let dir = home.join("profiles/default/sessions").join(session_id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("meta.json"),
            serde_json::to_vec(&json!({
                "id": session_id,
                "kind": "codex",
                "thread_id": "thr-1",
                "cwd": "/tmp/work",
                "pid": 0,
                "status": "running",
                "started_at": 10,
                "role": "generator",
                "task_id": task_id,
                "parent_session_id": parent,
                "root_session_id": parent.unwrap_or(session_id),
                "loaded_capabilities": {
                    "mcp_servers": ["harness"],
                    "skills": [],
                    "tool_groups": ["context"]
                },
                "has_transcript": true
            }))
            .unwrap(),
        )
        .unwrap();
    }

    fn mk_task(title: &str) -> TaskDraft {
        TaskDraft {
            title: title.into(),
            parent: None,
            depends_on: vec![],
            brief: None,
            acceptance: vec![],
            labels: vec![],
            spec_refs: vec![],
            write_paths: vec![],
            forbidden_paths: vec![],
            created_by: "test".into(),
        }
    }

    fn append_handoff(home: &Path) {
        let path = home.join("profiles/default/threads/thr-1/handoffs/T-0001.jsonl");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let handoff = Handoff {
            at: 123,
            from: "sid-child".into(),
            to_role: "evaluator".into(),
            task_id: "T-0001".into(),
            status: "ready_for_verification".into(),
            goal: "Verify indexed ledger".into(),
            assumptions: vec![],
            files_changed: vec!["src/lib.rs".into()],
            commands_run: vec!["cargo test".into()],
            verification_passed: vec![],
            verification_not_run: vec![],
            blocked_on: vec![],
            next_agent_action: "Review handoff".into(),
        };
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        writeln!(file, "{}", serde_json::to_string(&handoff).unwrap()).unwrap();
    }

    fn write_context_index(home: &Path) {
        let path = home.join("profiles/default/context.sqlite");
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE context_events (
                session_id TEXT NOT NULL,
                seq INTEGER NOT NULL,
                event_type TEXT NOT NULL,
                pressure REAL
            );
            INSERT INTO context_events(session_id, seq, event_type, pressure)
            VALUES ('sid-child', 7, 'session.context.pressure', 0.42);
            INSERT INTO context_events(session_id, seq, event_type, pressure)
            VALUES ('sid-child', 9, 'session.context.checkpoint', NULL);
            "#,
        )
        .unwrap();
    }

    #[test]
    fn agent_ledger_rebuilds_from_meta_and_handoff() {
        let home = tempfile::tempdir().unwrap();
        let store = TaskStore::with_profile(home.path(), "default").unwrap();
        store.create("thr-1", mk_task("Ledger task")).unwrap();
        write_meta(home.path(), "sid-root", None, None);
        write_meta(home.path(), "sid-child", Some("sid-root"), Some("T-0001"));
        append_handoff(home.path());
        write_context_index(home.path());

        let out = list(
            &store,
            home.path(),
            "default",
            &json!({ "root_session_id": "sid-root" }),
        )
        .unwrap();
        let entries = out["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 2);
        let child = entries
            .iter()
            .find(|entry| entry["session_id"] == "sid-child")
            .expect("child ledger entry");
        assert_eq!(child["objective"], "Ledger task");
        assert_eq!(child["next_action"], "Review handoff");
        assert_eq!(child["commands_run"][0], "cargo test");
        assert_eq!(child["latest_pressure"], 0.42);
        assert_eq!(child["latest_checkpoint_seq"], 9);

        let db_path = home.path().join("profiles/default/agent_ledger.sqlite");
        assert!(db_path.exists());
    }

    #[test]
    fn handoff_latest_resolves_task_from_session_meta() {
        let home = tempfile::tempdir().unwrap();
        let store = TaskStore::with_profile(home.path(), "default").unwrap();
        store.create("thr-1", mk_task("Ledger task")).unwrap();
        write_meta(home.path(), "sid-child", None, Some("T-0001"));
        append_handoff(home.path());

        let out = handoff_latest(
            &store,
            home.path(),
            "default",
            "unused-thread",
            &json!({ "session_id": "sid-child" }),
        )
        .unwrap();
        assert_eq!(out["thread_id"], "thr-1");
        assert_eq!(out["task_id"], "T-0001");
        assert_eq!(out["handoff"]["next_agent_action"], "Review handoff");
    }

    #[test]
    fn session_handoff_submit_posts_protocol_header_and_body() {
        let Some((server_url, rx)) = spawn_http_capture_server() else {
            return;
        };
        let home = tempfile::tempdir().unwrap();
        write_meta(home.path(), "sid-child", None, Some("T-0001"));

        let out = submit_handoff(
            Some("sid-child"),
            home.path(),
            "default",
            "thr-1",
            Some(&server_url),
            None,
            &json!({
                "to_role": "evaluator",
                "status": "ready_for_verification",
                "goal": "Verify handoff",
                "next_agent_action": "Review it",
                "commands_run": ["cargo test"]
            }),
        )
        .unwrap();
        assert_eq!(out["task_id"], "T-0001");
        let captured = rx.recv().expect("captured request");
        assert!(captured.starts_with("POST /api/threads/thr-1/tasks/T-0001/handoffs HTTP/1.1"));
        assert!(captured
            .to_ascii_lowercase()
            .contains("x-protocol-version: 1.0"));
        let body = captured.split("\r\n\r\n").nth(1).expect("body");
        let body: Value = serde_json::from_str(body).unwrap();
        assert_eq!(body["from"], "sid-child");
        assert_eq!(body["commands_run"][0], "cargo test");
    }

    fn spawn_http_capture_server() -> Option<(String, mpsc::Receiver<String>)> {
        let listener = match TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => listener,
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => return None,
            Err(e) => panic!("bind test server: {e}"),
        };
        let addr = listener.local_addr().expect("local addr");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut buf = Vec::new();
            let mut tmp = [0u8; 1024];
            loop {
                let n = stream.read(&mut tmp).expect("read request");
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&tmp[..n]);
                if let Some(header_end) = find_header_end(&buf) {
                    let headers = String::from_utf8_lossy(&buf[..header_end]).to_lowercase();
                    let content_len = headers
                        .lines()
                        .find_map(|line| {
                            line.strip_prefix("content-length:")
                                .and_then(|value| value.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    let total = header_end + 4 + content_len;
                    while buf.len() < total {
                        let n = stream.read(&mut tmp).expect("read body");
                        if n == 0 {
                            break;
                        }
                        buf.extend_from_slice(&tmp[..n]);
                    }
                    break;
                }
            }
            tx.send(String::from_utf8_lossy(&buf).to_string())
                .expect("send captured request");
            let response = concat!(
                "HTTP/1.1 201 Created\r\n",
                "Content-Type: application/json\r\n",
                "Content-Length: 59\r\n",
                "\r\n",
                "{\"task_id\":\"T-0001\",\"next_agent_action\":\"Review it\",\"at\":1}"
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
        });
        Some((format!("http://{addr}"), rx))
    }

    fn find_header_end(buf: &[u8]) -> Option<usize> {
        buf.windows(4).position(|window| window == b"\r\n\r\n")
    }
}
