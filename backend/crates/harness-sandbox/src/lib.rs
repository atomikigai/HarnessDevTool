//! Best-effort sandbox contract for commands executed by the harness bridge.
//!
//! This crate intentionally does **not** wrap the agent CLIs themselves. Claude,
//! Codex, Cursor and Antigravity keep their own sandbox/approval behavior. The
//! harness sandbox is for child processes the bridge launches directly, such as
//! module helpers and future evaluator shell checks.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::process::Command;

pub type SandboxResult<T> = Result<T, SandboxError>;

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("sandbox level {0:?} is not available on this platform yet")]
    UnsupportedLevel(SandboxLevel),
    #[error("workspace root is required for sandbox level {0:?}")]
    MissingWorkspace(SandboxLevel),
    #[error("sandbox path must be absolute: {0}")]
    NonAbsolutePath(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SandboxLevel {
    None,
    Workspace,
    WorkspaceNet,
    Strict,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SandboxProfile {
    pub level: SandboxLevel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_root: Option<PathBuf>,
    #[serde(default)]
    pub writable_roots: Vec<PathBuf>,
    #[serde(default)]
    pub readable_roots: Vec<PathBuf>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

impl SandboxProfile {
    pub fn none() -> Self {
        Self {
            level: SandboxLevel::None,
            workspace_root: None,
            writable_roots: Vec::new(),
            readable_roots: Vec::new(),
            env: BTreeMap::new(),
        }
    }

    pub fn workspace(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            level: SandboxLevel::Workspace,
            workspace_root: Some(root.clone()),
            writable_roots: vec![root],
            readable_roots: Vec::new(),
            env: BTreeMap::new(),
        }
    }

    pub fn workspace_net(root: impl Into<PathBuf>) -> Self {
        let mut profile = Self::workspace(root);
        profile.level = SandboxLevel::WorkspaceNet;
        profile
    }

    pub fn strict(root: impl Into<PathBuf>) -> Self {
        Self {
            level: SandboxLevel::Strict,
            workspace_root: Some(root.into()),
            writable_roots: Vec::new(),
            readable_roots: Vec::new(),
            env: BTreeMap::new(),
        }
    }

    pub fn validate(&self) -> SandboxResult<()> {
        match self.level {
            SandboxLevel::None => Ok(()),
            SandboxLevel::Workspace | SandboxLevel::WorkspaceNet | SandboxLevel::Strict => {
                let root = self
                    .workspace_root
                    .as_deref()
                    .ok_or(SandboxError::MissingWorkspace(self.level))?;
                ensure_absolute(root)?;
                for path in self.writable_roots.iter().chain(self.readable_roots.iter()) {
                    ensure_absolute(path)?;
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxPlan {
    pub level: SandboxLevel,
    pub available: bool,
    pub warning: Option<String>,
}

pub fn plan(profile: &SandboxProfile) -> SandboxResult<SandboxPlan> {
    profile.validate()?;
    let warning = match profile.level {
        SandboxLevel::None => None,
        SandboxLevel::Workspace | SandboxLevel::WorkspaceNet => platform_workspace_warning(),
        SandboxLevel::Strict => platform_strict_warning(),
    };
    Ok(SandboxPlan {
        level: profile.level,
        available: warning.is_none(),
        warning,
    })
}

pub fn apply_to_tokio_command(
    command: &mut Command,
    profile: &SandboxProfile,
) -> SandboxResult<SandboxPlan> {
    let plan = plan(profile)?;
    if let Some(root) = profile.workspace_root.as_ref() {
        command.current_dir(root);
    }
    for (key, value) in &profile.env {
        command.env(key, value);
    }
    command.env("HARNESS_SANDBOX_LEVEL", sandbox_level_str(profile.level));
    if let Some(warning) = plan.warning.as_deref() {
        command.env("HARNESS_SANDBOX_WARNING", warning);
    }
    Ok(plan)
}

#[derive(Debug, Clone)]
pub struct SandboxCommand {
    program: PathBuf,
    args: Vec<String>,
    profile: SandboxProfile,
}

impl SandboxCommand {
    pub fn new(program: impl Into<PathBuf>, profile: SandboxProfile) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            profile,
        }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn into_tokio_command(self) -> SandboxResult<(Command, SandboxPlan)> {
        let mut command = Command::new(&self.program);
        command.args(&self.args);
        command.stdin(Stdio::null());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        let plan = apply_to_tokio_command(&mut command, &self.profile)?;
        Ok((command, plan))
    }
}

pub fn sandbox_level_str(level: SandboxLevel) -> &'static str {
    match level {
        SandboxLevel::None => "none",
        SandboxLevel::Workspace => "workspace",
        SandboxLevel::WorkspaceNet => "workspace-net",
        SandboxLevel::Strict => "strict",
    }
}

fn ensure_absolute(path: &Path) -> SandboxResult<()> {
    if path.is_absolute() {
        Ok(())
    } else {
        Err(SandboxError::NonAbsolutePath(path.display().to_string()))
    }
}

#[cfg(target_os = "linux")]
fn platform_workspace_warning() -> Option<String> {
    Some("linux seccomp/bind-mount enforcement is not wired yet; command runs with workspace cwd only".into())
}

#[cfg(target_os = "linux")]
fn platform_strict_warning() -> Option<String> {
    Some(
        "linux strict sandbox requires seccompiler/bind mounts; command runs with warning only"
            .into(),
    )
}

#[cfg(target_os = "macos")]
fn platform_workspace_warning() -> Option<String> {
    Some("macOS sandbox-exec profile generation is not wired yet; command runs with workspace cwd only".into())
}

#[cfg(target_os = "macos")]
fn platform_strict_warning() -> Option<String> {
    Some("macOS strict sandbox requires sandbox-exec profile generation; command runs with warning only".into())
}

#[cfg(target_os = "windows")]
fn platform_workspace_warning() -> Option<String> {
    Some("Windows sandbox is a documented F6 stub; command runs with workspace cwd only".into())
}

#[cfg(target_os = "windows")]
fn platform_strict_warning() -> Option<String> {
    Some("Windows strict sandbox is a documented F6 stub; command runs with warning only".into())
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn platform_workspace_warning() -> Option<String> {
    Some("sandbox enforcement is not available on this platform; command runs with workspace cwd only".into())
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn platform_strict_warning() -> Option<String> {
    Some("strict sandbox is not available on this platform; command runs with warning only".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_workspace_requires_absolute_root() {
        let err = SandboxProfile::workspace("relative")
            .validate()
            .unwrap_err();
        assert!(matches!(err, SandboxError::NonAbsolutePath(_)));
    }

    #[test]
    fn none_profile_has_available_plan() {
        let plan = plan(&SandboxProfile::none()).unwrap();
        assert_eq!(plan.level, SandboxLevel::None);
        assert!(plan.available);
        assert_eq!(plan.warning, None);
    }

    #[test]
    fn workspace_profile_sets_level_and_cwd_on_command() {
        let root = std::env::temp_dir();
        let profile = SandboxProfile::workspace(&root);
        let mut command = Command::new("true");
        let plan = apply_to_tokio_command(&mut command, &profile).unwrap();

        assert_eq!(plan.level, SandboxLevel::Workspace);
        assert_eq!(sandbox_level_str(profile.level), "workspace");
        assert_eq!(command.as_std().get_current_dir(), Some(root.as_path()));
    }

    #[test]
    fn sandbox_command_builds_tokio_command() {
        let root = std::env::temp_dir();
        let profile = SandboxProfile::workspace_net(&root);
        let (command, plan) = SandboxCommand::new("echo", profile)
            .arg("ok")
            .into_tokio_command()
            .unwrap();

        assert_eq!(plan.level, SandboxLevel::WorkspaceNet);
        assert_eq!(command.as_std().get_program(), "echo");
        assert_eq!(command.as_std().get_current_dir(), Some(root.as_path()));
    }
}
