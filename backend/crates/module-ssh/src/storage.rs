use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::error::{SshError, SshResult};
use crate::types::{Host, HostInput, SshIdentity, SshIdentityInput, SshIdentityKind, SshKnownHost};

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

    pub fn list_identities(&self) -> SshResult<Vec<SshIdentity>> {
        let conn = self.open_identities_db()?;
        let mut stmt = conn.prepare(
            "SELECT id, label, kind, key_path, agent_socket, created_at, updated_at
             FROM identities
             ORDER BY label COLLATE NOCASE, id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SshIdentity {
                id: row.get(0)?,
                label: row.get(1)?,
                kind: parse_identity_kind(row.get::<_, String>(2)?),
                key_path: row.get(3)?,
                agent_socket: row.get(4)?,
                created_at: parse_rfc3339(row.get::<_, String>(5)?),
                updated_at: parse_rfc3339(row.get::<_, String>(6)?),
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn add_identity(&self, input: SshIdentityInput) -> SshResult<SshIdentity> {
        validate_identity_input(&input)?;
        let conn = self.open_identities_db()?;
        let now = Utc::now();
        let identity = SshIdentity {
            id: uuid::Uuid::new_v4().to_string(),
            label: input.label,
            kind: input.kind,
            key_path: input.key_path,
            agent_socket: input.agent_socket,
            created_at: now,
            updated_at: now,
        };
        conn.execute(
            "INSERT INTO identities
             (id, label, kind, key_path, agent_socket, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                identity.id,
                identity.label,
                identity_kind_str(&identity.kind),
                identity.key_path,
                identity.agent_socket,
                identity.created_at.to_rfc3339(),
                identity.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(identity)
    }

    pub fn list_known_hosts(&self) -> SshResult<Vec<SshKnownHost>> {
        let path = self.known_hosts_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let file = std::fs::File::open(path)?;
        let mut hosts = Vec::new();
        for line in BufReader::new(file).lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            hosts.push(serde_json::from_str::<SshKnownHost>(&line)?);
        }
        hosts.sort_by(|a, b| a.host.cmp(&b.host).then(a.port.cmp(&b.port)));
        Ok(hosts)
    }

    pub fn record_known_host(
        &self,
        host: &str,
        port: u16,
        fingerprint: &str,
        key_type: Option<String>,
    ) -> SshResult<SshKnownHost> {
        if host.trim().is_empty() {
            return Err(SshError::Validation("host is required".into()));
        }
        if port == 0 {
            return Err(SshError::Validation("port must be > 0".into()));
        }
        if fingerprint.trim().is_empty() {
            return Err(SshError::Validation("fingerprint is required".into()));
        }

        let mut hosts = self.list_known_hosts()?;
        let now = Utc::now();
        let mut next = SshKnownHost {
            host: host.trim().to_string(),
            port,
            fingerprint: fingerprint.trim().to_string(),
            key_type,
            first_seen_at: now,
            last_seen_at: now,
        };
        if let Some(existing) = hosts
            .iter_mut()
            .find(|known| known.host == next.host && known.port == next.port)
        {
            next.first_seen_at = existing.first_seen_at;
            *existing = next.clone();
        } else {
            hosts.push(next.clone());
        }
        self.write_known_hosts(&hosts)?;
        Ok(next)
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    fn path(&self) -> PathBuf {
        self.root.join("hosts.toml")
    }

    fn identities_path(&self) -> PathBuf {
        self.root.join("identities.db")
    }

    fn known_hosts_path(&self) -> PathBuf {
        self.root.join("known_hosts")
    }

    fn open_identities_db(&self) -> SshResult<Connection> {
        std::fs::create_dir_all(&self.root)?;
        let path = self.identities_path();
        let conn = Connection::open(&path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS identities (
               id TEXT PRIMARY KEY,
               label TEXT NOT NULL,
               kind TEXT NOT NULL,
               key_path TEXT,
               agent_socket TEXT,
               created_at TEXT NOT NULL,
               updated_at TEXT NOT NULL
             );",
        )?;
        set_private_file_permissions(&path)?;
        Ok(conn)
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

    fn write_known_hosts(&self, hosts: &[SshKnownHost]) -> SshResult<()> {
        if let Some(parent) = self.known_hosts_path().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let path = self.known_hosts_path();
        let tmp = path.with_extension("tmp");
        let mut options = OpenOptions::new();
        options.create(true).write(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&tmp)?;
        for host in hosts {
            serde_json::to_writer(&mut file, host)?;
            file.write_all(b"\n")?;
        }
        file.sync_all()?;
        std::fs::rename(&tmp, &path)?;
        set_private_file_permissions(&path)?;
        Ok(())
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

fn validate_identity_input(input: &SshIdentityInput) -> SshResult<()> {
    if input.label.trim().is_empty() {
        return Err(SshError::Validation("label is required".into()));
    }
    match input.kind {
        SshIdentityKind::KeyFile if input.key_path.as_deref().unwrap_or("").trim().is_empty() => {
            Err(SshError::Validation(
                "key_path is required for key_file identity".into(),
            ))
        }
        SshIdentityKind::Agent
            if input
                .agent_socket
                .as_deref()
                .unwrap_or("")
                .trim()
                .is_empty() =>
        {
            Err(SshError::Validation(
                "agent_socket is required for agent identity".into(),
            ))
        }
        _ => Ok(()),
    }
}

fn identity_kind_str(kind: &SshIdentityKind) -> &'static str {
    match kind {
        SshIdentityKind::KeyFile => "key_file",
        SshIdentityKind::Agent => "agent",
    }
}

fn parse_identity_kind(value: String) -> SshIdentityKind {
    match value.as_str() {
        "agent" => SshIdentityKind::Agent,
        _ => SshIdentityKind::KeyFile,
    }
}

fn parse_rfc3339(value: String) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&value)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
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
    std::fs::rename(&tmp, path)?;
    set_private_file_permissions(path)?;
    Ok(())
}

fn set_private_file_permissions(path: &Path) -> SshResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(path, perms)?;
    }
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

    #[test]
    fn identities_round_trip_in_private_sqlite_db() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(dir.path()).unwrap();
        let identity = storage
            .add_identity(SshIdentityInput {
                label: "deploy".into(),
                kind: SshIdentityKind::KeyFile,
                key_path: Some("/keys/deploy".into()),
                agent_socket: None,
            })
            .unwrap();

        let listed = storage.list_identities().unwrap();
        assert_eq!(listed, vec![identity]);
        assert!(dir.path().join("identities.db").exists());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.path().join("identities.db"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn known_hosts_replaces_host_port_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(dir.path()).unwrap();
        storage
            .record_known_host("example.com", 22, "SHA256:first", Some("ed25519".into()))
            .unwrap();
        let updated = storage
            .record_known_host("example.com", 22, "SHA256:next", Some("ed25519".into()))
            .unwrap();

        let listed = storage.list_known_hosts().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0], updated);
        assert_eq!(listed[0].fingerprint, "SHA256:next");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.path().join("known_hosts"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
    }
}
