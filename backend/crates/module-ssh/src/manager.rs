use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, SystemTime};

use dashmap::DashMap;
use harness_sandbox::{SandboxCommand, SandboxProfile};
use sha2::{Digest, Sha256};
use tokio::process::Command;
use tokio::time;
use uuid::Uuid;

use crate::error::{SshError, SshResult};
use crate::storage::Storage;
use crate::types::{
    AuthMethod, Host, HostInput, HostKeyPolicy, HostTestResult, RemoteEntry, RemoteEntryKind,
    SftpListResult, SftpTransfer, SftpTransferStatus, SshExecResult, SshIdentity, SshIdentityInput,
    SshKnownHost, SshSession, SshSessionStatus,
};

const SSH_TIMEOUT: Duration = Duration::from_secs(20);
const SSH_CONTEXT_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(600);
const CONTROL_PERSIST: &str = "10m";
const CONTROL_SOCKET_TTL: Duration = Duration::from_secs(12 * 60);
const DEFAULT_CONTEXT_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const REMOTE_CONTEXT_BRIEF_MAX_BYTES: usize = 12_000;
const REMOTE_CONTEXT_TRUNCATED_NOTE: &str = "\n[truncated: brief too large]\n";

const REMOTE_CONTEXT_COMMANDS: &[RemoteContextCommand] = &[
    RemoteContextCommand {
        key: "uname",
        title: "Kernel",
        command: "uname -a",
    },
    RemoteContextCommand {
        key: "os_release",
        title: "OS Release",
        command: "cat /etc/os-release 2>/dev/null || true",
    },
    RemoteContextCommand {
        key: "hostname",
        title: "Hostname",
        command: "hostname 2>/dev/null || uname -n",
    },
    RemoteContextCommand {
        key: "uptime",
        title: "Uptime",
        command: "uptime 2>/dev/null || true",
    },
    RemoteContextCommand {
        key: "interfaces",
        title: "Interfaces",
        command: "ip -brief addr 2>/dev/null || ifconfig 2>/dev/null || true",
    },
    RemoteContextCommand {
        key: "systemd_services",
        title: "Running systemd services",
        command: "command -v systemctl >/dev/null 2>&1 && systemctl list-units --type=service --state=running --no-pager --no-legend 2>/dev/null | head -40 || true",
    },
    RemoteContextCommand {
        key: "docker_ps",
        title: "Docker containers",
        command: "command -v docker >/dev/null 2>&1 && docker ps --format 'table {{.Names}}\\t{{.Image}}\\t{{.Status}}\\t{{.Ports}}' 2>/dev/null || true",
    },
    RemoteContextCommand {
        key: "package_manager",
        title: "Package manager",
        command: "for c in apt dnf yum pacman apk zypper brew; do command -v \"$c\" >/dev/null 2>&1 && printf '%s\\n' \"$c\"; done | head -5",
    },
    RemoteContextCommand {
        key: "relevant_dirs",
        title: "Relevant directories",
        command: "for d in \"$HOME\" /var/www /opt /srv; do [ -d \"$d\" ] && ls -ld \"$d\"; done",
    },
    RemoteContextCommand {
        key: "top_processes",
        title: "Top processes",
        command: "ps -eo pid,ppid,user,comm,%cpu,%mem --sort=-%cpu 2>/dev/null | head -16 || true",
    },
];

#[derive(Debug, Clone, Copy)]
struct RemoteContextCommand {
    key: &'static str,
    title: &'static str,
    command: &'static str,
}

#[derive(Debug, Clone)]
struct RemoteContextOutput {
    key: &'static str,
    title: &'static str,
    ok: bool,
    stdout: String,
    stderr: String,
}

#[derive(Debug)]
pub struct Manager {
    storage: Storage,
    sessions: DashMap<String, SshSession>,
}

impl Manager {
    pub fn new(harness_home: &Path, profile: &str) -> SshResult<Self> {
        let storage = Storage::new(
            harness_home
                .join("profiles")
                .join(profile)
                .join("modules")
                .join("ssh"),
        )?;
        Ok(Self {
            storage,
            sessions: DashMap::new(),
        })
    }

    pub fn list_hosts(&self) -> SshResult<Vec<Host>> {
        Ok(self
            .storage
            .list_hosts()?
            .into_iter()
            .map(redact_host_secret)
            .collect())
    }

    pub fn add_host(&self, input: HostInput) -> SshResult<Host> {
        self.storage.add_host(input).map(redact_host_secret)
    }

    pub fn remove_host(&self, id: &str) -> SshResult<bool> {
        self.storage.remove_host(id)
    }

    pub fn list_identities(&self) -> SshResult<Vec<SshIdentity>> {
        self.storage.list_identities()
    }

    pub fn add_identity(&self, input: SshIdentityInput) -> SshResult<SshIdentity> {
        self.storage.add_identity(input)
    }

    pub fn list_known_hosts(&self) -> SshResult<Vec<SshKnownHost>> {
        self.storage.list_known_hosts()
    }

    pub fn record_known_host(
        &self,
        host: &str,
        port: u16,
        fingerprint: &str,
        key_type: Option<String>,
    ) -> SshResult<SshKnownHost> {
        self.storage
            .record_known_host(host, port, fingerprint, key_type)
    }

    pub async fn test_host(&self, id: &str) -> SshResult<HostTestResult> {
        let host = self.storage.get_host(id)?;
        let result = self
            .run_ssh_command(
                &host,
                "printf 'ssh-ok:%s\\n' \"$(hostname 2>/dev/null || uname -n)\"",
                SSH_TIMEOUT,
            )
            .await?;
        let ok = result.status.success();
        let stdout = String::from_utf8_lossy(&result.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&result.stderr).trim().to_string();
        Ok(HostTestResult {
            ok,
            message: if ok {
                if stdout.is_empty() {
                    format!("Connected to {}@{}:{}", host.username, host.host, host.port)
                } else {
                    stdout
                }
            } else if stderr.is_empty() {
                format!("ssh exited with status {}", exit_code(&result))
            } else {
                stderr
            },
            fingerprint: None,
        })
    }

    pub async fn exec(&self, host_id: &str, cmd: &str) -> SshResult<SshExecResult> {
        let host = self.storage.get_host(host_id)?;
        let result = self.run_ssh_command(&host, cmd, SSH_TIMEOUT).await?;
        Ok(SshExecResult {
            ok: result.status.success(),
            exit_code: exit_code(&result),
            stdout: String::from_utf8_lossy(&result.stdout).to_string(),
            stderr: String::from_utf8_lossy(&result.stderr).to_string(),
        })
    }

    pub async fn sftp_list(&self, host_id: &str, path: &str) -> SshResult<SftpListResult> {
        let quoted = shell_quote(path);
        let cmd = format!("find {quoted} -maxdepth 1 -mindepth 1 -printf '%y\\t%s\\t%p\\n'");
        let exec = self.exec(host_id, &cmd).await?;
        let entries = if exec.ok {
            parse_find_entries(&exec.stdout)
        } else {
            Vec::new()
        };
        Ok(SftpListResult {
            host_id: host_id.to_string(),
            path: path.to_string(),
            entries,
            error: if exec.ok {
                None
            } else {
                Some(nonempty(exec.stderr, exec.stdout))
            },
        })
    }

    pub async fn sftp_get(
        &self,
        host_id: &str,
        remote_path: &str,
        local_path: &Path,
    ) -> SshResult<SftpTransfer> {
        validate_transfer_paths(Some(remote_path), Some(local_path))?;
        let host = self.storage.get_host(host_id)?;
        let remote = format!("{}@{}:{}", host.username, host.host, remote_path);
        let output = self
            .run_scp_command(&host, &[remote, local_path.display().to_string()])
            .await?;
        let completed = output.status.success();
        let bytes_done = if completed {
            std::fs::metadata(local_path).map(|m| m.len()).unwrap_or(0)
        } else {
            0
        };
        Ok(SftpTransfer {
            id: Uuid::new_v4().to_string(),
            host_id: host_id.to_string(),
            local_path: local_path.display().to_string(),
            remote_path: remote_path.to_string(),
            bytes_total: bytes_done,
            bytes_done,
            status: if completed {
                SftpTransferStatus::Completed
            } else {
                SftpTransferStatus::Failed
            },
            error: transfer_error(&output),
        })
    }

    pub async fn sftp_put(
        &self,
        host_id: &str,
        local_path: &Path,
        remote_path: &str,
    ) -> SshResult<SftpTransfer> {
        validate_transfer_paths(Some(remote_path), Some(local_path))?;
        let host = self.storage.get_host(host_id)?;
        let bytes_total = std::fs::metadata(local_path).map(|m| m.len()).unwrap_or(0);
        let remote = format!("{}@{}:{}", host.username, host.host, remote_path);
        let output = self
            .run_scp_command(&host, &[local_path.display().to_string(), remote])
            .await?;
        let completed = output.status.success();
        Ok(SftpTransfer {
            id: Uuid::new_v4().to_string(),
            host_id: host_id.to_string(),
            local_path: local_path.display().to_string(),
            remote_path: remote_path.to_string(),
            bytes_total,
            bytes_done: if completed { bytes_total } else { 0 },
            status: if completed {
                SftpTransferStatus::Completed
            } else {
                SftpTransferStatus::Failed
            },
            error: transfer_error(&output),
        })
    }

    pub async fn sftp_mkdir(&self, host_id: &str, path: &str) -> SshResult<SshExecResult> {
        validate_remote_path(path)?;
        self.exec(host_id, &format!("mkdir -- {}", shell_quote(path)))
            .await
    }

    pub async fn sftp_rmdir(&self, host_id: &str, path: &str) -> SshResult<SshExecResult> {
        validate_remote_path(path)?;
        self.exec(host_id, &format!("rmdir -- {}", shell_quote(path)))
            .await
    }

    pub async fn sftp_unlink(&self, host_id: &str, path: &str) -> SshResult<SshExecResult> {
        validate_remote_path(path)?;
        self.exec(host_id, &format!("rm -f -- {}", shell_quote(path)))
            .await
    }

    pub async fn sftp_rename(
        &self,
        host_id: &str,
        from_path: &str,
        to_path: &str,
    ) -> SshResult<SshExecResult> {
        validate_remote_path(from_path)?;
        validate_remote_path(to_path)?;
        self.exec(
            host_id,
            &format!("mv -- {} {}", shell_quote(from_path), shell_quote(to_path)),
        )
        .await
    }

    pub async fn open_session(&self, host_id: &str) -> SshResult<SshSession> {
        let probe = self.test_host(host_id).await?;
        if !probe.ok {
            return Err(SshError::Command(probe.message));
        }

        let session = SshSession {
            id: Uuid::new_v4().to_string(),
            host_id: host_id.to_string(),
            status: SshSessionStatus::Open,
            opened_at: chrono::Utc::now(),
        };
        self.sessions.insert(session.id.clone(), session.clone());
        Ok(session)
    }

    pub async fn close_session(&self, session_id: &str) -> SshResult<bool> {
        let Some((_id, session)) = self.sessions.remove(session_id) else {
            return Ok(false);
        };
        let _ = self.close_master_connection(&session.host_id).await;
        Ok(true)
    }

    pub async fn context_refresh(&self, host_id: &str) -> SshResult<String> {
        let host = self.storage.get_host(host_id)?;
        let mut outputs = Vec::new();
        for probe in REMOTE_CONTEXT_COMMANDS {
            let result = self
                .run_ssh_command(&host, probe.command, SSH_CONTEXT_COMMAND_TIMEOUT)
                .await;
            outputs.push(match result {
                Ok(output) => RemoteContextOutput {
                    key: probe.key,
                    title: probe.title,
                    ok: output.status.success(),
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                },
                Err(err) => RemoteContextOutput {
                    key: probe.key,
                    title: probe.title,
                    ok: false,
                    stdout: String::new(),
                    stderr: err.to_string(),
                },
            });
        }
        let brief = build_remote_context_brief(host_id, &host, &outputs);
        self.write_context_cache(host_id, &brief)?;
        Ok(brief)
    }

    pub async fn context(&self, host_id: &str, max_age_hours: Option<u64>) -> SshResult<String> {
        let max_age = max_age_hours
            .map(|hours| Duration::from_secs(hours.saturating_mul(60 * 60)))
            .unwrap_or(DEFAULT_CONTEXT_MAX_AGE);
        if let Some(cached) = self.cached_context_if_fresh(host_id, max_age)? {
            return Ok(cached);
        }
        self.context_refresh(host_id).await
    }

    pub fn cached_context_if_fresh(
        &self,
        host_id: &str,
        max_age: Duration,
    ) -> SshResult<Option<String>> {
        let path = self.context_cache_path(host_id);
        if !path.exists() || context_cache_is_stale(&path, max_age)? {
            return Ok(None);
        }
        Ok(Some(std::fs::read_to_string(path)?))
    }

    async fn run_ssh_command(
        &self,
        host: &Host,
        remote_cmd: &str,
        timeout: Duration,
    ) -> SshResult<std::process::Output> {
        let askpass = Askpass::new(self.storage.root(), host.password.as_deref())?;
        let known_hosts = self.storage.openssh_known_hosts_path()?;
        self.cleanup_expired_control_sockets();
        let mut command = self.sandbox_command("ssh")?;
        command.args(self.common_ssh_args(host, &known_hosts, false)?);

        match host.auth_method {
            AuthMethod::Password => {
                command
                    .arg("-o")
                    .arg("PreferredAuthentications=password")
                    .arg("-o")
                    .arg("PubkeyAuthentication=no");
                if let Some(askpass) = askpass.as_ref() {
                    command
                        .env("SSH_ASKPASS", &askpass.path)
                        .env("SSH_ASKPASS_REQUIRE", "force")
                        .env("DISPLAY", "none")
                        .env(
                            "HARNESS_SSH_PASSWORD",
                            host.password.as_deref().unwrap_or(""),
                        );
                }
            }
            AuthMethod::KeyFile => {
                let key_path = host
                    .key_path
                    .as_deref()
                    .ok_or_else(|| SshError::Validation("key_path is required".into()))?;
                command
                    .arg("-i")
                    .arg(key_path)
                    .arg("-o")
                    .arg("BatchMode=yes");
            }
            AuthMethod::Agent => {
                command.arg("-o").arg("BatchMode=yes");
            }
        }

        command
            .arg(format!("{}@{}", host.username, host.host))
            .arg(remote_cmd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = time::timeout(timeout, command.output())
            .await
            .map_err(|_| SshError::Timeout)??;
        drop(askpass);
        Ok(output)
    }

    async fn run_scp_command(
        &self,
        host: &Host,
        args: &[String],
    ) -> SshResult<std::process::Output> {
        let askpass = Askpass::new(self.storage.root(), host.password.as_deref())?;
        let known_hosts = self.storage.openssh_known_hosts_path()?;
        self.cleanup_expired_control_sockets();
        let mut command = self.sandbox_command("scp")?;
        command.args(self.common_ssh_args(host, &known_hosts, true)?);

        match host.auth_method {
            AuthMethod::Password => {
                command
                    .arg("-o")
                    .arg("PreferredAuthentications=password")
                    .arg("-o")
                    .arg("PubkeyAuthentication=no");
                if let Some(askpass) = askpass.as_ref() {
                    command
                        .env("SSH_ASKPASS", &askpass.path)
                        .env("SSH_ASKPASS_REQUIRE", "force")
                        .env("DISPLAY", "none")
                        .env(
                            "HARNESS_SSH_PASSWORD",
                            host.password.as_deref().unwrap_or(""),
                        );
                }
            }
            AuthMethod::KeyFile => {
                let key_path = host
                    .key_path
                    .as_deref()
                    .ok_or_else(|| SshError::Validation("key_path is required".into()))?;
                command
                    .arg("-i")
                    .arg(key_path)
                    .arg("-o")
                    .arg("BatchMode=yes");
            }
            AuthMethod::Agent => {
                command.arg("-o").arg("BatchMode=yes");
            }
        }

        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = time::timeout(TRANSFER_TIMEOUT, command.output())
            .await
            .map_err(|_| SshError::Timeout)??;
        drop(askpass);
        Ok(output)
    }

    fn sandbox_command(&self, program: &str) -> SshResult<Command> {
        let (command, _plan) =
            SandboxCommand::new(program, ssh_command_sandbox_profile(self.storage.root()))
                .into_tokio_command()?;
        Ok(command)
    }

    fn common_ssh_args(
        &self,
        host: &Host,
        known_hosts: &Path,
        scp: bool,
    ) -> SshResult<Vec<String>> {
        common_ssh_args(host, known_hosts, &self.control_path(host)?, scp, "auto")
    }

    fn control_dir(&self) -> PathBuf {
        self.storage.root().join(".runtime").join("control")
    }

    fn control_path(&self, host: &Host) -> SshResult<PathBuf> {
        let dir = self.control_dir();
        create_private_dir_all(&dir)?;
        Ok(dir.join(format!("cm-{}", short_host_hash(host))))
    }

    fn context_dir(&self) -> PathBuf {
        self.storage.root().join("context")
    }

    pub fn context_cache_path(&self, host_id: &str) -> PathBuf {
        self.context_dir()
            .join(format!("{}.md", safe_cache_key(host_id)))
    }

    fn write_context_cache(&self, host_id: &str, brief: &str) -> SshResult<()> {
        let path = self.context_cache_path(host_id);
        if let Some(parent) = path.parent() {
            create_private_dir_all(parent)?;
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, brief)?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }

    fn cleanup_expired_control_sockets(&self) {
        let dir = self.control_dir();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let now = SystemTime::now();
        for entry in entries.flatten() {
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            let Ok(modified) = metadata.modified() else {
                continue;
            };
            if now
                .duration_since(modified)
                .is_ok_and(|age| age > CONTROL_SOCKET_TTL)
            {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    async fn close_master_connection(&self, host_id: &str) -> SshResult<()> {
        let host = self.storage.get_host(host_id)?;
        let known_hosts = self.storage.openssh_known_hosts_path()?;
        let mut command = self.sandbox_command("ssh")?;
        command
            .args(common_ssh_args(
                &host,
                &known_hosts,
                &self.control_path(&host)?,
                false,
                "no",
            )?)
            .arg("-O")
            .arg("exit")
            .arg(format!("{}@{}", host.username, host.host))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let _ = time::timeout(SSH_CONTEXT_COMMAND_TIMEOUT, command.status()).await;
        Ok(())
    }
}

fn common_ssh_args(
    host: &Host,
    known_hosts: &Path,
    control_path: &Path,
    scp: bool,
    control_master: &str,
) -> SshResult<Vec<String>> {
    let mut args = Vec::new();
    if scp {
        args.push("-P".to_string());
    } else {
        args.push("-p".to_string());
    }
    args.push(host.port.to_string());
    push_ssh_option(&mut args, "ConnectTimeout=10");
    push_ssh_option(&mut args, "ServerAliveInterval=5");
    push_ssh_option(&mut args, "ServerAliveCountMax=1");
    push_ssh_option(
        &mut args,
        format!("UserKnownHostsFile={}", known_hosts.display()),
    );
    push_ssh_option(&mut args, "GlobalKnownHostsFile=/dev/null");
    push_ssh_option(&mut args, "HashKnownHosts=no");
    push_ssh_option(
        &mut args,
        match host.host_key_policy {
            HostKeyPolicy::Tofu => "StrictHostKeyChecking=accept-new",
            HostKeyPolicy::Strict => "StrictHostKeyChecking=yes",
        },
    );
    push_ssh_option(&mut args, format!("ControlMaster={control_master}"));
    push_ssh_option(&mut args, format!("ControlPath={}", control_path.display()));
    push_ssh_option(&mut args, format!("ControlPersist={CONTROL_PERSIST}"));
    Ok(args)
}

fn push_ssh_option(args: &mut Vec<String>, option: impl Into<String>) {
    args.push("-o".to_string());
    args.push(option.into());
}

fn create_private_dir_all(path: &Path) -> SshResult<()> {
    std::fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn short_host_hash(host: &Host) -> String {
    let mut hasher = Sha256::new();
    hasher.update(host.id.as_bytes());
    hasher.update(b"\0");
    hasher.update(host.username.as_bytes());
    hasher.update(b"\0");
    hasher.update(host.host.as_bytes());
    hasher.update(b"\0");
    hasher.update(host.port.to_be_bytes());
    let digest = hasher.finalize();
    digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn safe_cache_key(value: &str) -> String {
    let stem: String = value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let stem = if stem.is_empty() { "host" } else { &stem };
    format!("{}-{}", stem, short_value_hash(value))
}

fn short_value_hash(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();
    digest[..4]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn context_cache_is_stale(path: &Path, max_age: Duration) -> SshResult<bool> {
    let modified = std::fs::metadata(path)?.modified()?;
    Ok(SystemTime::now()
        .duration_since(modified)
        .map(|age| age > max_age)
        .unwrap_or(false))
}

fn build_remote_context_brief(
    host_id: &str,
    host: &Host,
    outputs: &[RemoteContextOutput],
) -> String {
    let mut brief = String::new();
    brief.push_str("# SSH Remote Context\n\n");
    brief.push_str(&format!("- Host id: `{}`\n", host_id));
    brief.push_str(&format!(
        "- Target: `{}@{}:{}`\n",
        host.username, host.host, host.port
    ));
    brief.push_str(&format!(
        "- Refreshed at: `{}`\n",
        chrono::Utc::now().to_rfc3339()
    ));
    brief.push_str("- Source: fixed read-only SSH probe commands, best effort.\n\n");

    for (idx, output) in outputs.iter().enumerate() {
        let stdout = output.stdout.trim();
        let stderr = output.stderr.trim();
        if stdout.is_empty() && (output.ok || optional_context_section(output.key)) {
            continue;
        }
        let section_marker = format!(
            "harness-remote-context:{}:{}:{}",
            short_value_hash(host_id),
            idx,
            short_value_hash(output.key)
        );
        let mut section = String::new();
        section.push_str(&format!("<!-- BEGIN {section_marker} -->\n"));
        section.push_str(&format!("## {}\n\n", output.title));
        if !stdout.is_empty() {
            section.push_str("```text\n");
            section.push_str(&truncate_context_text(&sanitize_context_text(stdout)));
            section.push_str("\n```\n");
        } else if !stderr.is_empty() {
            section.push_str("_Unavailable: ");
            section.push_str(&truncate_inline(stderr));
            section.push_str("_\n");
        } else {
            section.push_str("_Unavailable._\n");
        }
        section.push_str(&format!("<!-- END {section_marker} -->\n\n"));
        if brief.len() + section.len() > REMOTE_CONTEXT_BRIEF_MAX_BYTES {
            append_brief_truncation_note(&mut brief);
            break;
        }
        brief.push_str(&section);
    }
    brief
}

fn sanitize_context_text(value: &str) -> String {
    let mut sanitized = String::with_capacity(value.len());
    let mut backticks = 0usize;
    for ch in value.chars() {
        if ch == '`' {
            backticks += 1;
            continue;
        }
        push_sanitized_backticks(&mut sanitized, backticks);
        backticks = 0;
        sanitized.push(ch);
    }
    push_sanitized_backticks(&mut sanitized, backticks);
    sanitized
}

fn push_sanitized_backticks(output: &mut String, count: usize) {
    if count < 3 {
        for _ in 0..count {
            output.push('`');
        }
        return;
    }
    for idx in 0..count {
        if idx > 0 {
            output.push(' ');
        }
        output.push('`');
    }
}

fn append_brief_truncation_note(brief: &mut String) {
    if brief.len() + REMOTE_CONTEXT_TRUNCATED_NOTE.len() <= REMOTE_CONTEXT_BRIEF_MAX_BYTES {
        brief.push_str(REMOTE_CONTEXT_TRUNCATED_NOTE);
        return;
    }
    let keep = REMOTE_CONTEXT_BRIEF_MAX_BYTES.saturating_sub(REMOTE_CONTEXT_TRUNCATED_NOTE.len());
    let end = brief
        .char_indices()
        .map(|(idx, _)| idx)
        .take_while(|idx| *idx <= keep)
        .last()
        .unwrap_or(0);
    brief.truncate(end);
    brief.push_str(REMOTE_CONTEXT_TRUNCATED_NOTE);
}

fn optional_context_section(key: &str) -> bool {
    matches!(key, "systemd_services" | "docker_ps")
}

fn truncate_context_text(value: &str) -> String {
    const MAX: usize = 4_000;
    if value.len() <= MAX {
        value.to_string()
    } else {
        let end = value
            .char_indices()
            .map(|(idx, _)| idx)
            .take_while(|idx| *idx <= MAX)
            .last()
            .unwrap_or(0);
        format!("{}\n...[truncated]", &value[..end])
    }
}

fn truncate_inline(value: &str) -> String {
    let trimmed = value.replace('\n', " ");
    if trimmed.len() <= 300 {
        trimmed
    } else {
        let end = trimmed
            .char_indices()
            .map(|(idx, _)| idx)
            .take_while(|idx| *idx <= 300)
            .last()
            .unwrap_or(0);
        format!("{}...", &trimmed[..end])
    }
}

fn ssh_command_sandbox_profile(root: &Path) -> SandboxProfile {
    SandboxProfile::workspace(root)
}

struct Askpass {
    path: PathBuf,
}

impl Askpass {
    fn new(root: &Path, password: Option<&str>) -> SshResult<Option<Self>> {
        if password.is_none() {
            return Ok(None);
        }
        std::fs::create_dir_all(root)?;
        let path = root.join(format!("askpass-{}.sh", Uuid::new_v4()));
        let script = "#!/bin/sh\nprintf '%s\\n' \"$HARNESS_SSH_PASSWORD\"\n";
        let mut options = std::fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o700);
        }
        std::io::Write::write_all(&mut options.open(&path)?, script.as_bytes())?;
        Ok(Some(Self { path }))
    }
}

impl Drop for Askpass {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn exit_code(output: &std::process::Output) -> i32 {
    output.status.code().unwrap_or(-1)
}

fn nonempty(primary: String, fallback: String) -> String {
    let primary = primary.trim();
    if primary.is_empty() {
        fallback.trim().to_string()
    } else {
        primary.to_string()
    }
}

fn parse_find_entries(stdout: &str) -> Vec<RemoteEntry> {
    stdout
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let kind = match parts.next()? {
                "f" => RemoteEntryKind::File,
                "d" => RemoteEntryKind::Directory,
                "l" => RemoteEntryKind::Symlink,
                _ => RemoteEntryKind::Other,
            };
            let size = parts.next()?.parse().unwrap_or(0);
            let path = parts.next()?.to_string();
            let name = Path::new(&path)
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or(&path)
                .to_string();
            Some(RemoteEntry {
                path,
                name,
                kind,
                size,
                modified_at: None,
            })
        })
        .collect()
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn redact_host_secret(mut host: Host) -> Host {
    host.password = None;
    host
}

fn validate_transfer_paths(remote_path: Option<&str>, local_path: Option<&Path>) -> SshResult<()> {
    if remote_path.is_some_and(|p| p.trim().is_empty()) {
        return Err(SshError::Validation("remote_path is required".into()));
    }
    if local_path.is_some_and(|p| p.as_os_str().is_empty()) {
        return Err(SshError::Validation("local_path is required".into()));
    }
    Ok(())
}

fn validate_remote_path(path: &str) -> SshResult<()> {
    if path.trim().is_empty() {
        return Err(SshError::Validation("path is required".into()));
    }
    Ok(())
}

fn transfer_error(output: &std::process::Output) -> Option<String> {
    if output.status.success() {
        None
    } else {
        Some(nonempty(
            String::from_utf8_lossy(&output.stderr).to_string(),
            String::from_utf8_lossy(&output.stdout).to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AuthMethod;
    use chrono::Utc;
    use harness_sandbox::{sandbox_level_str, SandboxLevel};

    #[test]
    fn ssh_commands_use_workspace_sandbox_profile() {
        let root = std::env::temp_dir();
        let profile = ssh_command_sandbox_profile(&root);

        assert_eq!(profile.level, SandboxLevel::Workspace);
        assert_eq!(sandbox_level_str(profile.level), "workspace");
        assert_eq!(profile.workspace_root.as_deref(), Some(root.as_path()));
        assert_eq!(profile.writable_roots, vec![root]);
    }

    #[test]
    fn openssh_args_enable_control_master_reuse() {
        let host = test_host();
        let known_hosts = Path::new("/tmp/known_hosts");
        let control_path = Path::new("/tmp/harness-ssh/cm-abcdef");
        let args = common_ssh_args(&host, known_hosts, control_path, false, "auto").expect("args");

        assert!(args.windows(2).any(|w| w == ["-p", "2222"]));
        assert!(args.windows(2).any(|w| w == ["-o", "ControlMaster=auto"]));
        assert!(args.windows(2).any(|w| w == ["-o", "ControlPersist=10m"]));
        assert!(args
            .windows(2)
            .any(|w| w == ["-o", "ControlPath=/tmp/harness-ssh/cm-abcdef"]));
    }

    #[test]
    fn scp_args_use_same_control_master_options() {
        let host = test_host();
        let args = common_ssh_args(
            &host,
            Path::new("/tmp/known_hosts"),
            Path::new("/tmp/harness-ssh/cm-abcdef"),
            true,
            "auto",
        )
        .expect("args");

        assert!(args.windows(2).any(|w| w == ["-P", "2222"]));
        assert!(args.windows(2).any(|w| w == ["-o", "ControlMaster=auto"]));
        assert!(args
            .windows(2)
            .any(|w| w == ["-o", "ControlPath=/tmp/harness-ssh/cm-abcdef"]));
    }

    #[test]
    fn close_master_args_disable_control_master_creation() {
        let host = test_host();
        let args = common_ssh_args(
            &host,
            Path::new("/tmp/known_hosts"),
            Path::new("/tmp/harness-ssh/cm-abcdef"),
            false,
            "no",
        )
        .expect("args");

        assert!(args.windows(2).any(|w| w == ["-o", "ControlMaster=no"]));
        assert!(!args.windows(2).any(|w| w == ["-o", "ControlMaster=auto"]));
    }

    #[test]
    fn remote_context_brief_omits_missing_docker_and_systemd() {
        let host = test_host();
        let outputs = vec![
            context_output("uname", "Kernel", true, "Linux test 6.1", ""),
            context_output("hostname", "Hostname", true, "remote-a", ""),
            context_output("systemd_services", "Running systemd services", true, "", ""),
            context_output("docker_ps", "Docker containers", true, "", ""),
            context_output(
                "top_processes",
                "Top processes",
                true,
                "PID COMMAND\n1 init",
                "",
            ),
        ];

        let brief = build_remote_context_brief("host-1", &host, &outputs);

        assert!(brief.contains("## Kernel"));
        assert!(brief.contains("Linux test 6.1"));
        assert!(brief.contains("## Hostname"));
        assert!(brief.contains("remote-a"));
        assert!(brief.contains("## Top processes"));
        assert!(!brief.contains("## Docker containers"));
        assert!(!brief.contains("## Running systemd services"));
    }

    #[test]
    fn remote_context_brief_sanitizes_stdout_fence_escape() {
        let host = test_host();
        let outputs = vec![context_output(
            "malicious",
            "Malicious output",
            true,
            "before\n```\n## Injected\nverbatim\n````\nafter",
            "",
        )];

        let brief = build_remote_context_brief("host-1", &host, &outputs);

        assert_eq!(brief.matches("```").count(), 2);
        assert!(!brief.contains("```\n## Injected"));
        assert!(!brief.contains("````"));
        assert!(brief.contains("` ` `"));
        assert!(brief.contains("<!-- BEGIN harness-remote-context:"));
        assert!(brief.contains("<!-- END harness-remote-context:"));
    }

    #[test]
    fn remote_context_brief_has_global_size_cap() {
        let host = test_host();
        let outputs: Vec<_> = (0..10)
            .map(|idx| {
                context_output(
                    "large",
                    "Large output",
                    true,
                    &format!("section {idx}\n{}", "x".repeat(4_000)),
                    "",
                )
            })
            .collect();

        let brief = build_remote_context_brief("host-1", &host, &outputs);

        assert!(brief.len() <= REMOTE_CONTEXT_BRIEF_MAX_BYTES);
        assert!(brief.contains("[truncated: brief too large]"));
    }

    #[test]
    fn remote_context_probe_list_is_fixed_and_short_timeout() {
        assert!(SSH_CONTEXT_COMMAND_TIMEOUT <= Duration::from_secs(5));
        assert_eq!(REMOTE_CONTEXT_COMMANDS.len(), 10);
        assert!(REMOTE_CONTEXT_COMMANDS
            .iter()
            .all(|probe| !probe.command.contains("{host_id}")));
        assert!(REMOTE_CONTEXT_COMMANDS
            .iter()
            .all(|probe| !probe.command.contains("$HARNESS_USER_INPUT")));
    }

    #[test]
    fn context_cache_staleness_respects_max_age() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("context.md");
        std::fs::write(&path, "cached").expect("write");

        assert!(!context_cache_is_stale(&path, Duration::from_secs(60)).expect("fresh"));
        std::thread::sleep(Duration::from_millis(2));
        assert!(context_cache_is_stale(&path, Duration::ZERO).expect("stale"));
    }

    #[test]
    fn safe_cache_key_uses_raw_hash_to_avoid_collisions() {
        let dotted = safe_cache_key("host.1");
        let underscored = safe_cache_key("host_1");

        assert_ne!(dotted, underscored);
        assert!(dotted.starts_with("host_1-"));
        assert!(underscored.starts_with("host_1-"));
    }

    fn test_host() -> Host {
        Host {
            id: "host-1".to_string(),
            name: "Test host".to_string(),
            host: "example.test".to_string(),
            port: 2222,
            username: "alice".to_string(),
            auth_method: AuthMethod::Agent,
            key_path: None,
            password: None,
            host_key_policy: HostKeyPolicy::Tofu,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn context_output(
        key: &'static str,
        title: &'static str,
        ok: bool,
        stdout: &str,
        stderr: &str,
    ) -> RemoteContextOutput {
        RemoteContextOutput {
            key,
            title,
            ok,
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
        }
    }
}
