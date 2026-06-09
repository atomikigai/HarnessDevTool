use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use chrono::Utc;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use tokio::sync::{broadcast, Mutex as AsyncMutex};
use tokio::task::JoinHandle;

use crate::errors::SessionError;
use crate::kind::AgentKind;
use crate::manager::SessionEvent;
use crate::meta::{LoadedCapabilities, SessionMeta, SessionRepoContext, SessionStatus};
use crate::output::OutputWriter;

const PTY_FLUSH_INTERVAL_MS: u64 = 16;
const PTY_CHUNK_TARGET: usize = 16 * 1024;

#[derive(Default)]
struct SessionRuntime {
    output_forwarder: Option<JoinHandle<()>>,
    exit_waiter: Option<JoinHandle<()>>,
    state_detector: Option<JoinHandle<()>>,
    prompt_injector: Option<JoinHandle<()>>,
}

impl SessionRuntime {
    fn abort_interruptible(&mut self) {
        if let Some(handle) = self.state_detector.take() {
            handle.abort();
        }
        if let Some(handle) = self.prompt_injector.take() {
            handle.abort();
        }
    }
}

/// Handle to a running agent process.
pub struct AgentSession {
    meta: AsyncMutex<SessionMeta>,
    dir: PathBuf,
    writer: Arc<OutputWriter>,
    /// PTY master writer (input side). `Box<dyn Write + Send>`.
    pty_writer: AsyncMutex<Box<dyn Write + Send>>,
    pty_master: AsyncMutex<Box<dyn portable_pty::MasterPty + Send>>,
    pty_size: AsyncMutex<(u16, u16)>,
    /// Killer handle for the child process.
    killer: AsyncMutex<Box<dyn portable_pty::ChildKiller + Send + Sync>>,
    kill_lock: AsyncMutex<()>,
    shutdown_requested: AtomicBool,
    runtime: StdMutex<SessionRuntime>,
    child_pid: u32,
    seq: AtomicU64,
    /// Immutable identity, cached outside the async meta lock so non-async
    /// callers (e.g. the scheduler's budget pass) can read them without
    /// awaiting.
    id_static: String,
    kind_static: AgentKind,
    thread_id_static: String,
    cwd_static: PathBuf,
    /// Session-tree identity, cached for non-async readers (scheduler,
    /// audit emit) — matches `meta.parent_session_id` / `meta.root_session_id`.
    parent_session_id_static: Option<String>,
    root_session_id_static: String,
    role_static: Option<String>,
    owner_session_id_static: Option<String>,
    task_id_static: Option<String>,
    scopes_static: Vec<String>,
}

impl AgentSession {
    pub fn id(&self) -> &str {
        &self.id_static
    }
    pub fn kind(&self) -> AgentKind {
        self.kind_static
    }
    pub fn thread_id(&self) -> &str {
        &self.thread_id_static
    }
    pub fn cwd(&self) -> &std::path::Path {
        &self.cwd_static
    }
    pub fn parent_session_id_static(&self) -> Option<&str> {
        self.parent_session_id_static.as_deref()
    }
    pub fn root_session_id_static(&self) -> &str {
        &self.root_session_id_static
    }
    pub fn role(&self) -> Option<String> {
        self.role_static.clone()
    }
    pub fn owner_session_id_static(&self) -> Option<&str> {
        self.owner_session_id_static.as_deref()
    }
    pub fn task_id_static(&self) -> Option<&str> {
        self.task_id_static.as_deref()
    }
    pub fn scopes(&self) -> &[String] {
        &self.scopes_static
    }
}

impl AgentSession {
    /// Spawn a new agent session.
    ///
    /// Returns a handle wrapped in `Arc`. Two background tasks are also spawned:
    /// one drains the PTY master into the output log + broadcast channel, and
    /// another waits for the child to exit to update the persisted status.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_with_id(
        id: String,
        kind: AgentKind,
        binary: PathBuf,
        thread_id: String,
        cwd: PathBuf,
        dir: PathBuf,
        extra_args: Vec<String>,
        role: Option<String>,
        owner_session_id: Option<String>,
        task_id: Option<String>,
        scopes: Vec<String>,
        repo: Option<SessionRepoContext>,
        loaded_capabilities: LoadedCapabilities,
        parent_session_id: Option<String>,
        root_session_id: String,
        initial_size: Option<(u16, u16)>,
        bus: broadcast::Sender<SessionEvent>,
    ) -> Result<Arc<Self>, SessionError> {
        std::fs::create_dir_all(&dir)?;

        // Default to 80x24 when the caller didn't pass a size — matches the
        // historical behaviour and what every TTY app expects. The frontend
        // measures its container at mount and passes the real dimensions so
        // the TUI never has to repaint after the first SIGWINCH, avoiding
        // the "ugly initial frame" most users would see otherwise.
        let (cols, rows) = initial_size.unwrap_or((80, 24));
        let pty_system = native_pty_system();
        let pty_pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| SessionError::Pty(e.to_string()))?;

        let mut cmd = CommandBuilder::new(&binary);
        cmd.cwd(&cwd);
        for a in &extra_args {
            cmd.arg(a);
        }
        // Inherit env: portable-pty inherits parent env when not overridden.
        // Ensure TERM is reasonable for TUIs.
        if std::env::var_os("TERM").is_none() {
            cmd.env("TERM", "xterm-256color");
        }
        // Force a UTF-8 locale so accented characters round-trip in the PTY.
        // C.UTF-8 is universally available without locale-gen. Only set if the
        // parent didn't already provide an explicit UTF-8 locale.
        let lang_ok = std::env::var("LANG")
            .map(|v| {
                v.to_ascii_uppercase().contains("UTF-8") || v.to_ascii_uppercase().contains("UTF8")
            })
            .unwrap_or(false);
        if !lang_ok {
            cmd.env("LANG", "C.UTF-8");
        }
        let lc_all_ok = std::env::var("LC_ALL")
            .map(|v| {
                v.to_ascii_uppercase().contains("UTF-8") || v.to_ascii_uppercase().contains("UTF8")
            })
            .unwrap_or(false);
        if !lc_all_ok {
            cmd.env("LC_ALL", "C.UTF-8");
        }
        if !extra_args.is_empty() {
            tracing::info!(
                spawn_id = %id,
                kind = %kind,
                args = ?extra_args,
                "spawning agent with extra args"
            );
        }

        let mut child = pty_pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| SessionError::Pty(e.to_string()))?;
        let child_pid = child.process_id().unwrap_or(0);
        let killer = child.clone_killer();

        let reader = pty_pair
            .master
            .try_clone_reader()
            .map_err(|e| SessionError::Pty(e.to_string()))?;
        let writer = pty_pair
            .master
            .take_writer()
            .map_err(|e| SessionError::Pty(e.to_string()))?;

        // Drop the slave so EOF on the master once the child exits.
        drop(pty_pair.slave);

        let output_writer = Arc::new(OutputWriter::open(&dir)?);

        let meta = SessionMeta {
            id: id.clone(),
            kind,
            thread_id: thread_id.clone(),
            cwd: cwd.to_string_lossy().to_string(),
            pid: child_pid,
            status: SessionStatus::Running,
            started_at: Utc::now().timestamp_millis(),
            exit_code: None,
            role: role.clone(),
            owner_session_id: owner_session_id.clone(),
            task_id: task_id.clone(),
            scopes: scopes.clone(),
            repo: repo.clone(),
            loaded_capabilities,
            parent_session_id: parent_session_id.clone(),
            root_session_id: root_session_id.clone(),
            detected_state: None,
            has_transcript: matches!(kind, AgentKind::Claude | AgentKind::Codex),
        };
        persist_meta(&dir, &meta)?;

        let session = Arc::new(Self {
            meta: AsyncMutex::new(meta),
            dir: dir.clone(),
            writer: output_writer.clone(),
            pty_writer: AsyncMutex::new(writer),
            pty_master: AsyncMutex::new(pty_pair.master),
            pty_size: AsyncMutex::new((cols, rows)),
            killer: AsyncMutex::new(killer),
            kill_lock: AsyncMutex::new(()),
            shutdown_requested: AtomicBool::new(false),
            runtime: StdMutex::new(SessionRuntime::default()),
            child_pid,
            seq: AtomicU64::new(0),
            id_static: id.clone(),
            kind_static: kind,
            thread_id_static: thread_id.clone(),
            cwd_static: cwd.clone(),
            parent_session_id_static: parent_session_id.clone(),
            root_session_id_static: root_session_id.clone(),
            role_static: role.clone(),
            owner_session_id_static: owner_session_id.clone(),
            task_id_static: task_id.clone(),
            scopes_static: scopes.clone(),
        });

        // Emit started.
        let _ = bus.send(SessionEvent::Started {
            session_id: id.clone(),
            pid: child_pid,
        });

        // PTY reader task: blocking reads in a dedicated thread, forwarded
        // through a channel into the async runtime.
        let (tx_bytes, mut rx_bytes) = tokio::sync::mpsc::channel::<Vec<u8>>(128);
        let id_for_reader = id.clone();
        std::thread::spawn(move || {
            let mut reader = reader;
            let mut buf = vec![0u8; PTY_CHUNK_TARGET];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if tx_bytes.blocking_send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::debug!(spawn_id = %id_for_reader, error = %e, "pty reader closed");
                        break;
                    }
                }
            }
        });

        // Async forwarder: batch incoming bytes, append to log, broadcast.
        let bus_for_output = bus.clone();
        let session_for_output = session.clone();
        let id_for_output = id.clone();
        let output_forwarder = tokio::spawn(async move {
            let mut pending: Vec<u8> = Vec::with_capacity(PTY_CHUNK_TARGET * 2);
            let flush_interval = Duration::from_millis(PTY_FLUSH_INTERVAL_MS);

            loop {
                let recv = tokio::time::timeout(flush_interval, rx_bytes.recv()).await;
                match recv {
                    Ok(Some(chunk)) => {
                        pending.extend_from_slice(&chunk);
                        if pending.len() >= PTY_CHUNK_TARGET {
                            flush_chunk(
                                &mut pending,
                                &session_for_output,
                                &bus_for_output,
                                &id_for_output,
                            );
                        }
                    }
                    Ok(None) => {
                        // Channel closed = PTY EOF.
                        if !pending.is_empty() {
                            flush_chunk(
                                &mut pending,
                                &session_for_output,
                                &bus_for_output,
                                &id_for_output,
                            );
                        }
                        break;
                    }
                    Err(_) => {
                        // Timer tick: flush whatever we have.
                        if !pending.is_empty() {
                            flush_chunk(
                                &mut pending,
                                &session_for_output,
                                &bus_for_output,
                                &id_for_output,
                            );
                        }
                        if session_for_output.shutdown_requested.load(Ordering::SeqCst) {
                            break;
                        }
                    }
                }
            }
        });

        // Wait-for-exit task. `child.wait()` is blocking.
        let bus_for_exit = bus.clone();
        let session_for_exit = session.clone();
        let id_for_exit = id.clone();
        let handle_for_exit = tokio::runtime::Handle::current();
        let exit_waiter = tokio::task::spawn_blocking(move || {
            let exit_status = match child.wait() {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(spawn_id = %id_for_exit, error = %e, "child wait failed");
                    return;
                }
            };
            let code = exit_status.exit_code() as i32;
            let signal: Option<String> = None; // portable-pty 0.8 doesn't expose signal info portably.
            session_for_exit
                .shutdown_requested
                .store(true, Ordering::SeqCst);

            // Update meta on disk + in memory.
            let dir = session_for_exit.dir.clone();
            handle_for_exit.block_on(async move {
                let mut m = session_for_exit.meta.lock().await;
                // Heuristic: nonzero with killed flag set elsewhere? We update
                // status to Exited unless a prior explicit kill set Killed.
                if m.status == SessionStatus::Running {
                    m.status = SessionStatus::Exited;
                }
                m.exit_code = Some(code);
                let _ = persist_meta(&dir, &m);
            });

            let _ = bus_for_exit.send(SessionEvent::Exit {
                session_id: id_for_exit,
                code: Some(code),
                signal,
            });
        });

        // Heuristic state detector — tails the output log every 600ms and
        // classifies the CLI's interaction phase. Emits StateChanged on
        // transitions; persists the latest detection on `meta.detected_state`.
        let bus_for_state = bus.clone();
        let session_for_state = session.clone();
        let id_for_state = id.clone();
        let kind_for_state = kind;
        let dir_for_state = dir.clone();
        let state_detector = tokio::spawn(async move {
            use crate::detect::{detect as detect_fn, AgentState, TAIL_WINDOW_BYTES};
            let mut prev: AgentState = AgentState::Unknown;
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(600)).await;
                if session_for_state.shutdown_requested.load(Ordering::SeqCst) {
                    break;
                }

                // Stop polling once the child exited.
                let still_running = {
                    let m = session_for_state.meta.lock().await;
                    matches!(m.status, SessionStatus::Running)
                };
                if !still_running {
                    break;
                }

                let tail = match read_output_tail(&dir_for_state, TAIL_WINDOW_BYTES).await {
                    Ok(t) => t,
                    Err(_) => continue,
                };
                let next = detect_fn(kind_for_state, &tail);
                if next == prev {
                    continue;
                }
                {
                    let mut m = session_for_state.meta.lock().await;
                    m.detected_state = Some(next);
                    let _ = persist_meta(&dir_for_state, &m);
                }
                let _ = bus_for_state.send(SessionEvent::StateChanged {
                    session_id: id_for_state.clone(),
                    prev,
                    next,
                });
                prev = next;
            }
        });
        session.set_runtime_handles(output_forwarder, exit_waiter, state_detector);

        Ok(session)
    }

    fn set_runtime_handles(
        &self,
        output_forwarder: JoinHandle<()>,
        exit_waiter: JoinHandle<()>,
        state_detector: JoinHandle<()>,
    ) {
        let mut runtime = self.runtime.lock().expect("session runtime lock poisoned");
        runtime.output_forwarder = Some(output_forwarder);
        runtime.exit_waiter = Some(exit_waiter);
        runtime.state_detector = Some(state_detector);
    }

    pub fn set_prompt_injector(&self, handle: JoinHandle<()>) {
        let mut runtime = self.runtime.lock().expect("session runtime lock poisoned");
        runtime.prompt_injector = Some(handle);
    }

    #[cfg(test)]
    pub(crate) fn interruptible_abort_handles_for_test(&self) -> InterruptibleAbortHandles {
        let runtime = self.runtime.lock().expect("session runtime lock poisoned");
        InterruptibleAbortHandles {
            state_detector: runtime
                .state_detector
                .as_ref()
                .map(|handle| handle.abort_handle()),
            prompt_injector: runtime
                .prompt_injector
                .as_ref()
                .map(|handle| handle.abort_handle()),
        }
    }

    fn abort_interruptible_tasks(&self) {
        let mut runtime = self.runtime.lock().expect("session runtime lock poisoned");
        runtime.abort_interruptible();
    }

    pub async fn meta(&self) -> SessionMeta {
        self.meta.lock().await.clone()
    }

    pub fn pid(&self) -> u32 {
        self.child_pid
    }

    pub fn writer(&self) -> Arc<OutputWriter> {
        self.writer.clone()
    }

    /// Write raw bytes to the PTY master (forwarded to the child's stdin).
    pub async fn write_input(&self, bytes: &[u8]) -> Result<(), SessionError> {
        let mut w = self.pty_writer.lock().await;
        w.write_all(bytes)?;
        w.flush()?;
        Ok(())
    }

    pub async fn resize(&self, cols: u16, rows: u16) -> Result<(), SessionError> {
        let mut current = self.pty_size.lock().await;
        if *current == (cols, rows) {
            return Ok(());
        }
        let master = self.pty_master.lock().await;
        master
            .resize(PtySize {
                cols,
                rows,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| SessionError::Pty(e.to_string()))?;
        *current = (cols, rows);
        Ok(())
    }

    /// Send SIGTERM, wait up to 3s, then SIGKILL. Marks status as `Killed`.
    pub async fn kill(&self) -> Result<(), SessionError> {
        let _guard = self.kill_lock.lock().await;
        self.shutdown_requested.store(true, Ordering::SeqCst);
        self.abort_interruptible_tasks();
        {
            let mut m = self.meta.lock().await;
            if m.status != SessionStatus::Running {
                return Ok(());
            }
            m.status = SessionStatus::Killed;
            let _ = persist_meta(&self.dir, &m);
        }

        let pid = self.child_pid as i32;
        #[cfg(unix)]
        {
            if pid > 0 {
                unsafe {
                    // SIGTERM = 15
                    libc::kill(pid, libc::SIGTERM);
                }
            }
        }
        #[cfg(not(unix))]
        {
            let _ = pid;
        }

        // Give the child up to 3s to exit gracefully.
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            if pid <= 0 || !pid_alive(pid) {
                return Ok(());
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // Force kill.
        let mut killer = self.killer.lock().await;
        let _ = killer.kill();
        Ok(())
    }
}

#[cfg(test)]
pub(crate) struct InterruptibleAbortHandles {
    pub state_detector: Option<tokio::task::AbortHandle>,
    pub prompt_injector: Option<tokio::task::AbortHandle>,
}

#[cfg(unix)]
pub(crate) fn pid_alive(pid: i32) -> bool {
    if pid <= 0 {
        return false;
    }
    // kill(pid, 0) returns 0 if process exists and we have permission,
    // -1 with ESRCH if it doesn't exist.
    unsafe { libc::kill(pid, 0) == 0 }
}

#[cfg(not(unix))]
pub(crate) fn pid_alive(pid: i32) -> bool {
    pid > 0
}

/// Read up to `max` bytes from the end of the session's active `output.log`.
/// Cheap seek+read; falls back to reading the whole file if it's smaller
/// than `max`. The detector uses this on a 600ms tick so we keep it
/// allocation-light.
async fn read_output_tail(dir: &std::path::Path, max: usize) -> Result<Vec<u8>, std::io::Error> {
    let path = dir.join("output.log");
    let mut file = tokio::fs::File::open(&path).await?;
    let len = file.metadata().await?.len();
    let start = len.saturating_sub(max as u64);
    if start > 0 {
        use tokio::io::AsyncSeekExt;
        file.seek(std::io::SeekFrom::Start(start)).await?;
    }
    let mut buf = Vec::with_capacity(max);
    use tokio::io::AsyncReadExt;
    file.read_to_end(&mut buf).await?;
    Ok(buf)
}

fn flush_chunk(
    pending: &mut Vec<u8>,
    session: &Arc<AgentSession>,
    bus: &broadcast::Sender<SessionEvent>,
    id: &str,
) {
    if let Err(e) = session.writer.append(pending) {
        tracing::warn!(spawn_id = %id, error = %e, "output writer append failed");
    }
    let seq = session.seq.fetch_add(1, Ordering::SeqCst);
    let bytes = pending.clone();
    pending.clear();
    let _ = bus.send(SessionEvent::Output {
        session_id: id.to_string(),
        seq,
        bytes,
    });
}

pub(crate) fn persist_meta(dir: &std::path::Path, meta: &SessionMeta) -> Result<(), SessionError> {
    let path = dir.join("meta.json");
    let tmp = dir.join("meta.json.tmp");
    let bytes = serde_json::to_vec_pretty(meta)?;
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::pid_alive;

    #[test]
    fn non_positive_pid_is_never_alive() {
        assert!(!pid_alive(0));
        assert!(!pid_alive(-1));
    }
}
