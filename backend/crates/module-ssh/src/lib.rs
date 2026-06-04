//! `module-ssh` — SSH/SFTP manager backend module.
//!
//! This crate owns saved SSH hosts, TOFU host-key metadata, session handles and
//! transfer queue state for the F4 SSH module. Network operations are wired in
//! incrementally; storage and API contracts are stable first so server/MCP/UI
//! can depend on typed shapes.

pub mod error;
pub mod manager;
pub mod storage;
pub mod types;

pub use error::{SshError, SshResult};
pub use manager::Manager;
pub use types::{
    AuthMethod, Host, HostInput, HostKeyPolicy, HostTestResult, RemoteEntry, RemoteEntryKind,
    SftpListResult, SftpTransfer, SftpTransferStatus, SshExecResult, SshIdentity, SshIdentityInput,
    SshIdentityKind, SshKnownHost, SshSession, SshSessionStatus,
};
