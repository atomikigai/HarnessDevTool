use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct MailboxMessage {
    pub id: String,
    pub from_session_id: String,
    pub to_session_id: String,
    pub body: String,
    /// Unix epoch milliseconds.
    #[cfg_attr(feature = "ts-export", ts(type = "number"))]
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub acked: bool,
    #[cfg_attr(feature = "ts-export", ts(type = "number", optional = nullable))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acked_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acked_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum MailboxEvent {
    #[serde(rename = "mailbox.sent")]
    Sent {
        id: String,
        from_session_id: String,
        to_session_id: String,
        body: String,
        created_at: i64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task_id: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        scopes: Vec<String>,
    },
    #[serde(rename = "mailbox.acked")]
    Acked {
        id: String,
        acked_at: i64,
        acked_by: String,
    },
}

#[derive(Debug, Clone)]
pub struct MailboxStore {
    root: PathBuf,
}

impl MailboxStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn send(
        &self,
        from_session_id: &str,
        to_session_id: &str,
        body: impl Into<String>,
        task_id: Option<String>,
        scopes: Vec<String>,
    ) -> Result<MailboxMessage, std::io::Error> {
        let mut scopes = scopes;
        scopes.retain(|scope| !scope.trim().is_empty());
        scopes.sort();
        scopes.dedup();

        let event = MailboxEvent::Sent {
            id: uuid::Uuid::new_v4().to_string(),
            from_session_id: from_session_id.to_string(),
            to_session_id: to_session_id.to_string(),
            body: body.into(),
            created_at: Utc::now().timestamp_millis(),
            task_id,
            scopes,
        };
        self.append(to_session_id, &event)?;
        let messages = derive_messages(std::iter::once(event));
        Ok(messages.into_iter().next().expect("sent message"))
    }

    pub fn ack(
        &self,
        to_session_id: &str,
        message_id: &str,
        acked_by: &str,
    ) -> Result<Option<MailboxMessage>, std::io::Error> {
        let before = self.list(to_session_id)?;
        if !before.iter().any(|msg| msg.id == message_id) {
            return Ok(None);
        }
        let event = MailboxEvent::Acked {
            id: message_id.to_string(),
            acked_at: Utc::now().timestamp_millis(),
            acked_by: acked_by.to_string(),
        };
        self.append(to_session_id, &event)?;
        Ok(self
            .list(to_session_id)?
            .into_iter()
            .find(|msg| msg.id == message_id))
    }

    pub fn list(&self, to_session_id: &str) -> Result<Vec<MailboxMessage>, std::io::Error> {
        let path = self.path(to_session_id);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let file = OpenOptions::new().read(true).open(path)?;
        let mut events = Vec::new();
        for line in BufReader::new(file).lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<MailboxEvent>(&line) {
                Ok(event) => events.push(event),
                Err(e) => {
                    tracing::warn!(error = %e, "skipping invalid mailbox event");
                }
            }
        }
        Ok(derive_messages(events))
    }

    fn append(&self, to_session_id: &str, event: &MailboxEvent) -> Result<(), std::io::Error> {
        std::fs::create_dir_all(self.root.join(to_session_id))?;
        let path = self.path(to_session_id);
        let mut options = OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path)?;
        serde_json::to_writer(&mut file, event)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        Ok(())
    }

    fn path(&self, to_session_id: &str) -> PathBuf {
        self.root.join(to_session_id).join("mailbox.jsonl")
    }
}

fn derive_messages<I>(events: I) -> Vec<MailboxMessage>
where
    I: IntoIterator<Item = MailboxEvent>,
{
    let mut messages = Vec::new();
    let mut acked: HashSet<String> = HashSet::new();
    let mut ack_meta: Vec<(String, i64, String)> = Vec::new();

    for event in events {
        match event {
            MailboxEvent::Sent {
                id,
                from_session_id,
                to_session_id,
                body,
                created_at,
                task_id,
                scopes,
            } => messages.push(MailboxMessage {
                id,
                from_session_id,
                to_session_id,
                body,
                created_at,
                task_id,
                scopes,
                acked: false,
                acked_at: None,
                acked_by: None,
            }),
            MailboxEvent::Acked {
                id,
                acked_at,
                acked_by,
            } => {
                acked.insert(id.clone());
                ack_meta.push((id, acked_at, acked_by));
            }
        }
    }

    for msg in &mut messages {
        if acked.contains(&msg.id) {
            msg.acked = true;
            if let Some((_, at, by)) = ack_meta.iter().rev().find(|(id, _, _)| id == &msg.id) {
                msg.acked_at = Some(*at);
                msg.acked_by = Some(by.clone());
            }
        }
    }
    messages.sort_by_key(|msg| msg.created_at);
    messages
}

#[allow(dead_code)]
pub fn mailbox_path(root: &Path, session_id: &str) -> PathBuf {
    root.join(session_id).join("mailbox.jsonl")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mailbox_is_append_only_and_ack_is_derived() {
        let root = std::env::temp_dir().join(format!("harness-mailbox-{}", uuid::Uuid::new_v4()));
        let store = MailboxStore::new(&root);

        let sent = store
            .send(
                "parent",
                "child",
                "please report status",
                Some("T-0001".into()),
                vec!["backend".into(), "backend".into(), "".into()],
            )
            .unwrap();
        assert_eq!(sent.from_session_id, "parent");
        assert_eq!(sent.to_session_id, "child");
        assert_eq!(sent.task_id.as_deref(), Some("T-0001"));
        assert_eq!(sent.scopes, vec!["backend"]);
        assert!(!sent.acked);

        let listed = store.list("child").unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, sent.id);
        assert!(!listed[0].acked);

        let acked = store.ack("child", &sent.id, "child").unwrap().unwrap();
        assert!(acked.acked);
        assert_eq!(acked.acked_by.as_deref(), Some("child"));

        let raw = std::fs::read_to_string(root.join("child").join("mailbox.jsonl")).unwrap();
        assert_eq!(raw.lines().count(), 2);

        let _ = std::fs::remove_dir_all(root);
    }
}
