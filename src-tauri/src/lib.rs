use pulldown_cmark::{html, Options, Parser};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::ipc::Channel;
use tauri::{Manager, State};
use tauri_plugin_shell::process::CommandChild;
use tauri_plugin_shell::ShellExt;

// ── Markdown commands ────────────────────────────────────────────────────────

fn md_opts() -> Options {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_SMART_PUNCTUATION);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts
}

fn render_one(text: &str) -> String {
    let parser = Parser::new_ext(text, md_opts());
    let mut out = String::with_capacity(text.len() * 2);
    html::push_html(&mut out, parser);
    out
}

#[tauri::command]
fn parse_markdown(text: &str) -> String {
    render_one(text)
}

#[tauri::command]
fn parse_markdown_batch(texts: Vec<String>) -> Vec<String> {
    texts.par_iter().map(|t| render_one(t)).collect()
}

// ── Native PTY output streaming ──────────────────────────────────────────────

const PROTOCOL_VERSION: &str = "1.0";
const PROTOCOL_VERSION_HEADER: &str = "X-Protocol-Version";

#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
enum PtyStreamEvent {
    Started,
    Exit {
        code: Option<i32>,
        signal: Option<String>,
    },
    Lagged {
        skipped: u64,
        resync: Option<String>,
    },
    Error {
        message: String,
    },
}

#[derive(Deserialize)]
struct SessionExitPayload {
    code: Option<i32>,
    signal: Option<String>,
}

#[derive(Deserialize)]
struct LaggedPayload {
    skipped: Option<u64>,
    resync: Option<String>,
}

#[derive(Clone, Default)]
struct PtyStreamRegistry(Arc<PtyStreamRegistryInner>);

#[derive(Default)]
struct PtyStreamRegistryInner {
    next_id: AtomicU64,
    cancels: Mutex<HashMap<u64, tokio::sync::oneshot::Sender<()>>>,
}

#[tauri::command]
async fn stream_pty_output(
    session_id: String,
    on_output: Channel<Vec<u8>>,
    on_event: Channel<PtyStreamEvent>,
    registry: State<'_, PtyStreamRegistry>,
) -> Result<u64, String> {
    let stream_id = registry.0.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
    registry
        .0
        .cancels
        .lock()
        .map_err(|_| "PTY stream registry lock poisoned".to_string())?
        .insert(stream_id, cancel_tx);

    let registry_for_task = registry.inner().clone();
    tauri::async_runtime::spawn(async move {
        if let Err(err) =
            stream_pty_output_inner(session_id, on_output, on_event.clone(), cancel_rx).await
        {
            let _ = on_event.send(PtyStreamEvent::Error {
                message: err.to_string(),
            });
        }
        if let Ok(mut cancels) = registry_for_task.0.cancels.lock() {
            cancels.remove(&stream_id);
        }
    });
    Ok(stream_id)
}

#[tauri::command]
fn stop_pty_output_stream(
    stream_id: u64,
    registry: State<'_, PtyStreamRegistry>,
) -> Result<(), String> {
    if let Some(cancel) = registry
        .0
        .cancels
        .lock()
        .map_err(|_| "PTY stream registry lock poisoned".to_string())?
        .remove(&stream_id)
    {
        let _ = cancel.send(());
    }
    Ok(())
}

async fn stream_pty_output_inner(
    session_id: String,
    on_output: Channel<Vec<u8>>,
    on_event: Channel<PtyStreamEvent>,
    mut cancel_rx: tokio::sync::oneshot::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use futures_util::StreamExt;

    let api_base =
        std::env::var("HARNESS_API_BASE").unwrap_or_else(|_| "http://127.0.0.1:43177/api".into());
    let url = format!(
        "{}/events/pty?session={}",
        api_base.trim_end_matches('/'),
        url_encode_component(&session_id)
    );

    let response = reqwest::Client::new()
        .get(url)
        .header(PROTOCOL_VERSION_HEADER, PROTOCOL_VERSION)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(format!("native PTY stream failed: {}", response.status()).into());
    }

    let _ = on_event.send(PtyStreamEvent::Started);

    let mut stream = response.bytes_stream();
    let mut pending = Vec::<u8>::new();
    loop {
        tokio::select! {
            _ = &mut cancel_rx => break,
            chunk = stream.next() => {
                let Some(chunk) = chunk else { break };
                let chunk = chunk?;
                pending.extend_from_slice(&chunk);
                drain_pty_frames(&mut pending, &on_output, &on_event)?;
            }
        }
    }

    Ok(())
}

fn drain_pty_frames(
    pending: &mut Vec<u8>,
    on_output: &Channel<Vec<u8>>,
    on_event: &Channel<PtyStreamEvent>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    while pending.len() >= 5 {
        let kind = pending[0];
        let len = u32::from_be_bytes([pending[1], pending[2], pending[3], pending[4]]) as usize;
        if pending.len() < 5 + len {
            break;
        }
        let payload = pending[5..5 + len].to_vec();
        pending.drain(..5 + len);
        match kind {
            1 => on_output.send(payload)?,
            2 => {
                let payload: SessionExitPayload = serde_json::from_slice(&payload)?;
                on_event.send(PtyStreamEvent::Exit {
                    code: payload.code,
                    signal: payload.signal,
                })?;
            }
            3 => {
                let payload: LaggedPayload = serde_json::from_slice(&payload)?;
                on_event.send(PtyStreamEvent::Lagged {
                    skipped: payload.skipped.unwrap_or(0),
                    resync: payload.resync,
                })?;
            }
            _ => {}
        }
    }

    Ok(())
}

fn url_encode_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(b));
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// ── Sidecar management ───────────────────────────────────────────────────────

/// Holds the harness-server child process.
/// Dropping this kills the backend — Tauri drops managed state on app exit.
struct SidecarProcess(Mutex<Option<CommandChild>>);

impl Drop for SidecarProcess {
    fn drop(&mut self) {
        if let Ok(mut child) = self.0.lock() {
            if let Some(child) = child.take() {
                let _ = child.kill();
            }
        }
    }
}

fn spawn_backend(app: &tauri::App) {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let harness_home = format!("{home}/.harness");

    let result = app
        .shell()
        .sidecar("harness-server")
        .and_then(|cmd| {
            cmd.env("HARNESS_BIND", "127.0.0.1:43177")
                .env("HARNESS_HOME", &harness_home)
                // loopback origins are already allowed by harness-server CORS policy
                .spawn()
        });

    match result {
        Ok((_rx, child)) => {
            eprintln!("[harness] backend sidecar started on 127.0.0.1:43177 (home: {harness_home})");
            app.manage(SidecarProcess(Mutex::new(Some(child))));
        }
        Err(e) => {
            // Binary not bundled (dev mode without build-sidecar) or already running.
            eprintln!("[harness] sidecar not started — expecting external backend: {e}");
        }
    }
}

// ── App entry point ──────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(PtyStreamRegistry::default())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            spawn_backend(app);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            parse_markdown,
            parse_markdown_batch,
            stream_pty_output,
            stop_pty_output_stream
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
