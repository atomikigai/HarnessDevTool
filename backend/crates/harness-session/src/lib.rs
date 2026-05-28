//! harness-session: spawn and manage agent CLI processes attached to PTYs.

pub mod detect;
pub mod errors;
pub mod kind;
pub mod manager;
pub mod meta;
pub mod output;
pub mod session;

pub use detect::{detect as detect_state, AgentState};
pub use errors::SessionError;
pub use kind::AgentKind;
pub use manager::{Manager, SessionEvent, SpawnOpts};
pub use meta::{SessionMeta, SessionStatus};
pub use session::AgentSession;
