use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use dashmap::DashMap;
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
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(600);

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
        let result = self.run_ssh_command(&host, cmd).await?;
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
        let _host = self.storage.get_host(host_id)?;
        let session = SshSession {
            id: Uuid::new_v4().to_string(),
            host_id: host_id.to_string(),
            status: SshSessionStatus::Failed,
            opened_at: chrono::Utc::now(),
        };
        self.sessions.insert(session.id.clone(), session.clone());
        Err(SshError::NotImplemented("ssh session.open"))
    }

    pub fn close_session(&self, session_id: &str) -> SshResult<bool> {
        Ok(self.sessions.remove(session_id).is_some())
    }

    async fn run_ssh_command(
        &self,
        host: &Host,
        remote_cmd: &str,
    ) -> SshResult<std::process::Output> {
        let askpass = Askpass::new(self.storage.root(), host.password.as_deref())?;
        let mut command = Command::new("ssh");
        command
            .arg("-p")
            .arg(host.port.to_string())
            .arg("-o")
            .arg("ConnectTimeout=10")
            .arg("-o")
            .arg("ServerAliveInterval=5")
            .arg("-o")
            .arg("ServerAliveCountMax=1")
            .arg("-o")
            .arg(match host.host_key_policy {
                HostKeyPolicy::Tofu => "StrictHostKeyChecking=accept-new",
                HostKeyPolicy::Strict => "StrictHostKeyChecking=yes",
            });

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

        let output = time::timeout(SSH_TIMEOUT, command.output())
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
        let mut command = Command::new("scp");
        command
            .arg("-P")
            .arg(host.port.to_string())
            .arg("-o")
            .arg("ConnectTimeout=10")
            .arg("-o")
            .arg(match host.host_key_policy {
                HostKeyPolicy::Tofu => "StrictHostKeyChecking=accept-new",
                HostKeyPolicy::Strict => "StrictHostKeyChecking=yes",
            });

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
