//! Best-effort sandbox contract for commands executed by the harness bridge.
//!
//! This crate intentionally does **not** wrap the agent CLIs themselves. Claude,
//! Codex, Cursor and Antigravity keep their own sandbox/approval behavior. The
//! harness sandbox is for child processes the bridge launches directly, such as
//! module helpers and future evaluator shell checks.

use std::collections::BTreeMap;
#[cfg(target_os = "linux")]
use std::os::unix::fs::PermissionsExt;
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
    #[error("sandbox path is not valid UTF-8: {0}")]
    NonUtf8Path(String),
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
    #[cfg(target_os = "linux")]
    let warning = linux_platform_warning(profile.level, || find_program_in_path("bwrap"));
    #[cfg(not(target_os = "linux"))]
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
        let mut command = sandboxed_command_for_program(&self.program, &self.profile)?;
        command.args(&self.args);
        command.stdin(Stdio::null());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        let plan = apply_to_tokio_command(&mut command, &self.profile)?;
        Ok((command, plan))
    }
}

fn sandboxed_command_for_program(
    program: &Path,
    profile: &SandboxProfile,
) -> SandboxResult<Command> {
    #[cfg(target_os = "linux")]
    {
        if profile.level != SandboxLevel::None {
            if let Some(bwrap) = find_program_in_path("bwrap") {
                let mut command = Command::new(bwrap);
                command.args(bubblewrap_args(profile, program)?);
                return Ok(command);
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        if profile.level != SandboxLevel::None {
            let mut command = Command::new("sandbox-exec");
            command.arg("-p");
            command.arg(sandbox_exec_profile(profile)?);
            command.arg(program);
            return Ok(command);
        }
    }
    let _ = profile;
    Ok(Command::new(program))
}

#[cfg(target_os = "linux")]
fn bubblewrap_args(profile: &SandboxProfile, program: &Path) -> SandboxResult<Vec<String>> {
    profile.validate()?;
    let program = path_arg(program)?;
    let mut args = vec![
        "--ro-bind".into(),
        "/".into(),
        "/".into(),
        "--tmpfs".into(),
        "/tmp".into(),
        "--dev".into(),
        "/dev".into(),
        "--proc".into(),
        "/proc".into(),
        "--unshare-pid".into(),
    ];

    if profile.level != SandboxLevel::WorkspaceNet {
        args.push("--unshare-net".into());
    }

    for path in &profile.writable_roots {
        let path = path_arg(path)?;
        args.push("--bind".into());
        args.push(path.clone());
        args.push(path);
    }

    args.push(program);
    Ok(args)
}

pub fn sandbox_exec_profile(profile: &SandboxProfile) -> SandboxResult<String> {
    profile.validate()?;
    let mut out = String::from("(version 1)\n");

    match profile.level {
        SandboxLevel::None => {
            out.push_str("(allow default)\n");
            return Ok(out);
        }
        SandboxLevel::Workspace | SandboxLevel::WorkspaceNet | SandboxLevel::Strict => {
            out.push_str("(deny default)\n");
            out.push_str("(allow process*)\n");
            out.push_str("(allow sysctl-read)\n");
            out.push_str("(allow mach-lookup)\n");
            out.push_str("(allow file-read-metadata)\n");
            out.push_str("(allow file-read*");
            for path in macos_default_read_roots(profile) {
                out.push_str(" (subpath ");
                out.push_str(&sandbox_profile_quote_path(&path)?);
                out.push(')');
            }
            out.push_str(")\n");

            if !profile.writable_roots.is_empty() {
                out.push_str("(allow file-write*");
                for path in &profile.writable_roots {
                    out.push_str(" (subpath ");
                    out.push_str(&sandbox_profile_quote_path(path)?);
                    out.push(')');
                }
                out.push_str(")\n");
            }

            if profile.level == SandboxLevel::WorkspaceNet {
                out.push_str("(allow network*)\n");
            }
        }
    }

    Ok(out)
}

fn macos_default_read_roots(profile: &SandboxProfile) -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from("/bin"),
        PathBuf::from("/sbin"),
        PathBuf::from("/usr"),
        PathBuf::from("/System"),
        PathBuf::from("/Library"),
        PathBuf::from("/private/etc"),
    ];
    if let Some(root) = profile.workspace_root.as_ref() {
        roots.push(root.clone());
    }
    roots.extend(profile.readable_roots.iter().cloned());
    roots
}

fn sandbox_profile_quote_path(path: &Path) -> SandboxResult<String> {
    let raw = path
        .to_str()
        .ok_or_else(|| SandboxError::NonUtf8Path(path.display().to_string()))?;
    Ok(format!(
        "\"{}\"",
        raw.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}

fn path_arg(path: &Path) -> SandboxResult<String> {
    path.to_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| SandboxError::NonUtf8Path(path.display().to_string()))
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
    Some(
        "linux bubblewrap (bwrap) was not found in PATH; command runs with workspace cwd only"
            .into(),
    )
}

#[cfg(target_os = "linux")]
fn platform_strict_warning() -> Option<String> {
    Some(
        "linux strict sandbox requires bubblewrap (bwrap) in PATH; command runs with warning only"
            .into(),
    )
}

#[cfg(target_os = "linux")]
fn linux_platform_warning(
    level: SandboxLevel,
    bwrap_lookup: impl FnOnce() -> Option<PathBuf>,
) -> Option<String> {
    match level {
        SandboxLevel::None => None,
        SandboxLevel::Workspace | SandboxLevel::WorkspaceNet | SandboxLevel::Strict => {
            if bwrap_lookup().is_some() {
                None
            } else if level == SandboxLevel::Strict {
                platform_strict_warning()
            } else {
                platform_workspace_warning()
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn find_program_in_path(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| {
            candidate.is_file()
                && candidate
                    .metadata()
                    .map(|meta| meta.permissions().mode() & 0o111 != 0)
                    .unwrap_or(false)
        })
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
        #[cfg(target_os = "macos")]
        assert_eq!(command.as_std().get_program(), "sandbox-exec");
        #[cfg(target_os = "linux")]
        if let Some(bwrap) = find_program_in_path("bwrap") {
            assert_eq!(command.as_std().get_program(), bwrap.as_os_str());
        } else {
            assert_eq!(command.as_std().get_program(), "echo");
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            assert_eq!(command.as_std().get_program(), "echo");
        }
        assert_eq!(command.as_std().get_current_dir(), Some(root.as_path()));
    }

    #[test]
    fn sandbox_exec_profile_limits_workspace_and_network() {
        let root = std::env::temp_dir();
        let profile = SandboxProfile::workspace_net(&root);
        let profile_text = sandbox_exec_profile(&profile).unwrap();
        let root = sandbox_profile_quote_path(&root).unwrap();

        assert!(profile_text.contains("(version 1)"));
        assert!(profile_text.contains("(deny default)"));
        assert!(profile_text.contains("(allow process*)"));
        assert!(profile_text.contains("(allow network*)"));
        assert!(profile_text.contains(&format!("(subpath {root})")));
    }

    #[test]
    fn strict_sandbox_exec_profile_does_not_allow_network_or_writes() {
        let root = std::env::temp_dir();
        let profile = SandboxProfile::strict(&root);
        let profile_text = sandbox_exec_profile(&profile).unwrap();

        assert!(profile_text.contains("(deny default)"));
        assert!(!profile_text.contains("(allow network*)"));
        assert!(!profile_text.contains("(allow file-write*"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_plan_falls_back_when_bwrap_is_missing() {
        let profile = SandboxProfile::workspace("/workspace");
        let warning = linux_platform_warning(profile.level, || None);

        assert_eq!(
            warning,
            Some(
                "linux bubblewrap (bwrap) was not found in PATH; command runs with workspace cwd only"
                    .into()
            )
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_plan_is_available_when_bwrap_exists() {
        let profile = SandboxProfile::strict("/workspace");
        let warning =
            linux_platform_warning(profile.level, || Some(PathBuf::from("/usr/bin/bwrap")));

        assert_eq!(warning, None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn bubblewrap_workspace_args_are_exact() {
        let profile = SandboxProfile::workspace("/workspace");

        assert_eq!(
            bubblewrap_args(&profile, Path::new("/bin/echo")).unwrap(),
            strings([
                "--ro-bind",
                "/",
                "/",
                "--tmpfs",
                "/tmp",
                "--dev",
                "/dev",
                "--proc",
                "/proc",
                "--unshare-pid",
                "--unshare-net",
                "--bind",
                "/workspace",
                "/workspace",
                "/bin/echo",
            ])
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn bubblewrap_workspace_net_args_are_exact() {
        let profile = SandboxProfile::workspace_net("/workspace");

        assert_eq!(
            bubblewrap_args(&profile, Path::new("/bin/echo")).unwrap(),
            strings([
                "--ro-bind",
                "/",
                "/",
                "--tmpfs",
                "/tmp",
                "--dev",
                "/dev",
                "--proc",
                "/proc",
                "--unshare-pid",
                "--bind",
                "/workspace",
                "/workspace",
                "/bin/echo",
            ])
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn bubblewrap_strict_args_are_exact() {
        let profile = SandboxProfile::strict("/workspace");

        assert_eq!(
            bubblewrap_args(&profile, Path::new("/bin/echo")).unwrap(),
            strings([
                "--ro-bind",
                "/",
                "/",
                "--tmpfs",
                "/tmp",
                "--dev",
                "/dev",
                "--proc",
                "/proc",
                "--unshare-pid",
                "--unshare-net",
                "/bin/echo",
            ])
        );
    }

    #[cfg(target_os = "linux")]
    fn strings<const N: usize>(items: [&str; N]) -> Vec<String> {
        items.into_iter().map(ToOwned::to_owned).collect()
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[ignore = "requires bubblewrap and user namespaces on the host"]
    async fn bubblewrap_prevents_writes_outside_workspace() {
        if find_program_in_path("bwrap").is_none() {
            return;
        }

        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap()
            .join("target")
            .join("harness-sandbox-bwrap-test")
            .join(format!("{}", std::process::id()));
        let workspace = root.join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        let outside = root.join("outside");
        let _ = std::fs::remove_file(&outside);

        let (mut command, plan) =
            SandboxCommand::new("/bin/sh", SandboxProfile::workspace(&workspace))
                .args(["-c", "printf ok > inside && ! printf no > ../outside"])
                .into_tokio_command()
                .unwrap();
        let output = command.output().await.unwrap();

        assert!(plan.available);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            std::fs::read_to_string(workspace.join("inside")).unwrap(),
            "ok"
        );
        assert!(!outside.exists());
    }
}
