//! harness-session: spawn and manage agent CLI processes attached to PTYs.

mod adapter;
pub mod detect;
pub mod errors;
pub mod kind;
pub mod mailbox;
pub mod manager;
pub mod meta;
pub mod output;
pub mod session;

pub use detect::{detect as detect_state, AgentState};
pub use errors::SessionError;
pub use kind::AgentKind;
pub use mailbox::{MailboxMessage, MailboxStore};
pub use manager::{Manager, McpServerConfig, SessionEvent, SpawnOpts};
pub use meta::{LoadedCapabilities, SessionMeta, SessionRepoContext, SessionResult, SessionStatus};
pub use session::AgentSession;
