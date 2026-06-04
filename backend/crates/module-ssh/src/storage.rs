use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::error::{SshError, SshResult};
use crate::types::{Host, HostInput};

#[derive(Debug, Default, Serialize, Deserialize)]
struct HostsFile {
    #[serde(default)]
    hosts: BTreeMap<String, Host>,
}

#[derive(Debug, Clone)]
pub struct Storage {
    root: PathBuf,
}

impl Storage {
    pub fn new(root: impl Into<PathBuf>) -> SshResult<Self> {
        let root = root.into();
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    pub fn list_hosts(&self) -> SshResult<Vec<Host>> {
        let mut hosts = self.read_hosts()?.hosts.into_values().collect::<Vec<_>>();
        hosts.sort_by(|a, b| a.name.cmp(&b.name).then(a.host.cmp(&b.host)));
        Ok(hosts)
    }

    pub fn get_host(&self, id: &str) -> SshResult<Host> {
        self.read_hosts()?
            .hosts
            .remove(id)
            .ok_or_else(|| SshError::HostNotFound(id.to_string()))
    }

    pub fn add_host(&self, input: HostInput) -> SshResult<Host> {
        validate_input(&input)?;
        let mut file = self.read_hosts()?;
        let now = Utc::now();
        let host = Host {
            id: uuid::Uuid::new_v4().to_string(),
            name: input.name,
            host: input.host,
            port: input.port,
            username: input.username,
            auth_method: input.auth_method,
            key_path: input.key_path,
            password: input.password,
            host_key_policy: input.host_key_policy,
            created_at: now,
            updated_at: now,
        };
        file.hosts.insert(host.id.clone(), host.clone());
        self.write_hosts(&file)?;
        Ok(host)
    }

    pub fn remove_host(&self, id: &str) -> SshResult<bool> {
        let mut file = self.read_hosts()?;
        let removed = file.hosts.remove(id).is_some();
        if removed {
            self.write_hosts(&file)?;
        }
        Ok(removed)
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    fn path(&self) -> PathBuf {
        self.root.join("hosts.toml")
    }

    fn read_hosts(&self) -> SshResult<HostsFile> {
        let path = self.path();
        if !path.exists() {
            return Ok(HostsFile::default());
        }
        let text = std::fs::read_to_string(path)?;
        toml_edit::de::from_str(&text).map_err(|e| SshError::Toml(e.to_string()))
    }

    fn write_hosts(&self, file: &HostsFile) -> SshResult<()> {
        write_private_toml(&self.path(), file)
    }
}

fn validate_input(input: &HostInput) -> SshResult<()> {
    if input.name.trim().is_empty() {
        return Err(SshError::Validation("name is required".into()));
    }
    if input.host.trim().is_empty() {
        return Err(SshError::Validation("host is required".into()));
    }
    if input.username.trim().is_empty() {
        return Err(SshError::Validation("username is required".into()));
    }
    if input.port == 0 {
        return Err(SshError::Validation("port must be > 0".into()));
    }
    match input.auth_method {
        crate::types::AuthMethod::KeyFile if input.key_path.as_deref().unwrap_or("").is_empty() => {
            Err(SshError::Validation(
                "key_path is required for key_file auth".into(),
            ))
        }
        crate::types::AuthMethod::Password
            if input.password.as_deref().unwrap_or("").is_empty() =>
        {
            Err(SshError::Validation(
                "password is required for password auth".into(),
            ))
        }
        _ => Ok(()),
    }
}

fn write_private_toml(path: &Path, value: &impl Serialize) -> SshResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text =
        toml_edit::ser::to_string_pretty(value).map_err(|e| SshError::Toml(e.to_string()))?;
    let tmp = path.with_extension("toml.tmp");
    let mut options = OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AuthMethod, HostKeyPolicy};

    #[test]
    fn host_crud_round_trips_private_storage() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(dir.path()).unwrap();
        let host = storage
            .add_host(HostInput {
                name: "prod".into(),
                host: "example.com".into(),
                port: 22,
                username: "deploy".into(),
                auth_method: AuthMethod::KeyFile,
                key_path: Some("/keys/prod".into()),
                password: None,
                host_key_policy: HostKeyPolicy::Tofu,
            })
            .unwrap();

        assert_eq!(storage.list_hosts().unwrap().len(), 1);
        assert_eq!(storage.get_host(&host.id).unwrap().host, "example.com");
        assert!(storage.remove_host(&host.id).unwrap());
        assert!(storage.list_hosts().unwrap().is_empty());
    }
}
