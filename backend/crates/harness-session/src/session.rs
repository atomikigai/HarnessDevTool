use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use tokio::sync::{broadcast, Mutex as AsyncMutex};

use crate::errors::SessionError;
use crate::kind::AgentKind;
use crate::manager::SessionEvent;
use crate::meta::{SessionMeta, SessionStatus};
use crate::output::OutputWriter;

const PTY_FLUSH_INTERVAL_MS: u64 = 16;
const PTY_CHUNK_TARGET: usize = 4096;

/// Handle to a running agent process.
pub struct AgentSession {
    meta: AsyncMutex<SessionMeta>,
    dir: PathBuf,
    writer: Arc<OutputWriter>,
    /// PTY master writer (input side). `Box<dyn Write + Send>`.
    pty_writer: AsyncMutex<Box<dyn Write + Send>>,
    pty_master: AsyncMutex<Box<dyn portable_pty::MasterPty + Send>>,
    /// Killer handle for the child process.
    killer: AsyncMutex<Box<dyn portable_pty::ChildKiller + Send + Sync>>,
    child_pid: u32,
    seq: AtomicU64,
    /// Immutable identity, cached outside the async meta lock so non-async
    /// callers (e.g. the scheduler's budget pass) can read them without
    /// awaiting.
    id_static: String,
    kind_static: AgentKind,
    thread_id_static: String,
    cwd_static: PathBuf,
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
        bus: broadcast::Sender<SessionEvent>,
    ) -> Result<Arc<Self>, SessionError> {
        std::fs::create_dir_all(&dir)?;

        let pty_system = native_pty_system();
        let pty_pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
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
            role,
        };
        persist_meta(&dir, &meta)?;

        let session = Arc::new(Self {
            meta: AsyncMutex::new(meta),
            dir: dir.clone(),
            writer: output_writer.clone(),
            pty_writer: AsyncMutex::new(writer),
            pty_master: AsyncMutex::new(pty_pair.master),
            killer: AsyncMutex::new(killer),
            child_pid,
            seq: AtomicU64::new(0),
            id_static: id.clone(),
            kind_static: kind,
            thread_id_static: thread_id.clone(),
            cwd_static: cwd.clone(),
        });

        // Emit started.
        let _ = bus.send(SessionEvent::Started {
            session_id: id.clone(),
            pid: child_pid,
        });

        // PTY reader task: blocking reads in a dedicated thread, forwarded
        // through a channel into the async runtime.
        let (tx_bytes, mut rx_bytes) = tokio::sync::mpsc::channel::<Vec<u8>>(64);
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
                        tracing::debug!(error = %e, "pty reader closed");
                        break;
                    }
                }
            }
        });

        // Async forwarder: batch incoming bytes, append to log, broadcast.
        let bus_for_output = bus.clone();
        let session_for_output = session.clone();
        let id_for_output = id.clone();
        tokio::spawn(async move {
            use base64::engine::general_purpose::STANDARD as B64;

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
                                &B64,
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
                                &B64,
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
                                &B64,
                            );
                        }
                    }
                }
            }
        });

        // Wait-for-exit task. `child.wait()` is blocking.
        let bus_for_exit = bus.clone();
        let session_for_exit = session.clone();
        let id_for_exit = id.clone();
        tokio::task::spawn_blocking(move || {
            let exit_status = match child.wait() {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(error = %e, "child wait failed");
                    return;
                }
            };
            let code = exit_status.exit_code() as i32;
            let signal: Option<String> = None; // portable-pty 0.8 doesn't expose signal info portably.

            // Update meta on disk + in memory.
            let dir = session_for_exit.dir.clone();
            tokio::spawn(async move {
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

        Ok(session)
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
        let master = self.pty_master.lock().await;
        master
            .resize(PtySize {
                cols,
                rows,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| SessionError::Pty(e.to_string()))?;
        Ok(())
    }

    /// Send SIGTERM, wait up to 3s, then SIGKILL. Marks status as `Killed`.
    pub async fn kill(&self) -> Result<(), SessionError> {
        {
            let mut m = self.meta.lock().await;
            m.status = SessionStatus::Killed;
            let _ = persist_meta(&self.dir, &m);
        }

        let pid = self.child_pid as i32;
        #[cfg(unix)]
        unsafe {
            // SIGTERM = 15
            libc::kill(pid, libc::SIGTERM);
        }
        #[cfg(not(unix))]
        {
            let _ = pid;
        }

        // Give the child up to 3s to exit gracefully.
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            if !pid_alive(pid) {
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

#[cfg(unix)]
fn pid_alive(pid: i32) -> bool {
    // kill(pid, 0) returns 0 if process exists and we have permission,
    // -1 with ESRCH if it doesn't exist.
    unsafe { libc::kill(pid, 0) == 0 }
}

#[cfg(not(unix))]
fn pid_alive(_pid: i32) -> bool {
    true
}

fn flush_chunk(
    pending: &mut Vec<u8>,
    session: &Arc<AgentSession>,
    bus: &broadcast::Sender<SessionEvent>,
    id: &str,
    b64: &base64::engine::GeneralPurpose,
) {
    use base64::Engine;
    if let Err(e) = session.writer.append(pending) {
        tracing::warn!(error = %e, "output writer append failed");
    }
    let seq = session.seq.fetch_add(1, Ordering::SeqCst);
    let encoded = b64.encode(&pending);
    pending.clear();
    let _ = bus.send(SessionEvent::Output {
        session_id: id.to_string(),
        seq,
        b64: encoded,
    });
}

fn persist_meta(dir: &std::path::Path, meta: &SessionMeta) -> Result<(), SessionError> {
    let path = dir.join("meta.json");
    let tmp = dir.join("meta.json.tmp");
    let bytes = serde_json::to_vec_pretty(meta)?;
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}
