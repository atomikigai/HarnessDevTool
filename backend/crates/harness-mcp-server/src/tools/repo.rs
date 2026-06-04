//! Deterministic repo-intelligence rails.
//!
//! These tools intentionally expose a small, typed view of the workspace so
//! agents do not have to rediscover structure by reading files blindly.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use gix;
use serde_json::{json, Value};
use tokei::{Config, Languages};

const DEFAULT_SCAN_LIMIT: usize = 400;
const DEFAULT_MAX_DEPTH: usize = 4;
const DEFAULT_MAX_BYTES: usize = 64 * 1024;
const MAX_GIT_BYTES: usize = 128 * 1024;

pub fn analyze(root: &Path, args: &Value) -> Result<Value, String> {
    let dir = resolve_under_root(root, opt_str(args, "path").unwrap_or("."))?;
    if !dir.is_dir() {
        return Err(format!("path is not a directory: {}", dir.display()));
    }

    let files = collect_files(&dir, DEFAULT_MAX_DEPTH, DEFAULT_SCAN_LIMIT)?;
    let file_names: Vec<String> = files
        .iter()
        .filter_map(|p| p.file_name().and_then(OsStr::to_str).map(String::from))
        .collect();
    let relative_files: Vec<String> = files.iter().filter_map(|p| relative_to(&dir, p)).collect();

    let package_json = read_json_if_exists(&dir.join("package.json"));
    let package_scripts = package_json
        .as_ref()
        .and_then(|v| v.get("scripts"))
        .and_then(Value::as_object)
        .map(|scripts| {
            scripts
                .iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();

    Ok(json!({
        "root": dir.display().to_string(),
        "stack": detect_stack(&file_names, &relative_files, &package_json),
        "package_manager": detect_package_manager(&file_names),
        "key_files": key_files(&file_names, &relative_files),
        "scripts": package_scripts,
        "env": {
            "has_env": dir.join(".env").exists(),
            "has_env_example": dir.join(".env.example").exists(),
        },
        "git": git_summary(&dir),
        "codebase_memory": codebase_memory_status(&dir, &json!({}))?,
        "sample_files": relative_files.into_iter().take(80).collect::<Vec<_>>(),
        "code_stats": code_stats(&dir),
    }))
}

pub fn scan(root: &Path, args: &Value) -> Result<Value, String> {
    let dir = resolve_under_root(root, opt_str(args, "path").unwrap_or("."))?;
    if !dir.is_dir() {
        return Err(format!("path is not a directory: {}", dir.display()));
    }
    let max_depth = args
        .get("max_depth")
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .unwrap_or(DEFAULT_MAX_DEPTH);
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .unwrap_or(DEFAULT_SCAN_LIMIT);
    let files = collect_files(&dir, max_depth, limit)?;
    Ok(json!({
        "root": dir.display().to_string(),
        "limit": limit,
        "max_depth": max_depth,
        "files": files.iter().filter_map(|p| relative_to(&dir, p)).collect::<Vec<_>>(),
    }))
}

pub fn read_file(root: &Path, args: &Value) -> Result<Value, String> {
    let path = resolve_under_root(root, str_arg(args, "path")?)?;
    if !path.is_file() {
        return Err(format!("path is not a file: {}", path.display()));
    }
    let max_bytes = args
        .get("max_bytes")
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .unwrap_or(DEFAULT_MAX_BYTES)
        .min(DEFAULT_MAX_BYTES);
    let mut content =
        std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let truncated = content.len() > max_bytes;
    if truncated {
        truncate_utf8_safe(&mut content, max_bytes);
    }
    if let Some(head_lines) = args.get("head_lines").and_then(Value::as_u64) {
        let lines: Vec<&str> = content.lines().take(head_lines as usize).collect();
        content = lines.join("\n");
    }
    Ok(json!({
        "path": relative_to(&canonical_root(root)?, &path).unwrap_or_else(|| path.display().to_string()),
        "bytes": content.len(),
        "truncated": truncated,
        "content": content,
    }))
}

pub fn write_file(
    root: &Path,
    args: &Value,
    write_paths: &[String],
    forbidden_paths: &[String],
) -> Result<Value, String> {
    let path_arg = str_arg(args, "path")?;
    let content = str_arg(args, "content")?;
    let root = canonical_root(root)?;
    let path = resolve_under_root(&root, path_arg)?;
    ensure_write_allowed(&root, &path, write_paths, forbidden_paths)?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create parent {}: {e}", parent.display()))?;
    }
    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("harness")
    ));
    {
        let mut file = std::fs::File::create(&tmp)
            .map_err(|e| format!("create temp {}: {e}", tmp.display()))?;
        file.write_all(content.as_bytes())
            .map_err(|e| format!("write temp {}: {e}", tmp.display()))?;
        file.sync_all()
            .map_err(|e| format!("fsync temp {}: {e}", tmp.display()))?;
    }
    std::fs::rename(&tmp, &path)
        .map_err(|e| format!("rename {} -> {}: {e}", tmp.display(), path.display()))?;

    Ok(json!({
        "ok": true,
        "path": relative_to(&root, &path).unwrap_or_else(|| path.display().to_string()),
        "bytes": content.len(),
    }))
}

fn truncate_utf8_safe(content: &mut String, max_bytes: usize) {
    let mut end = max_bytes.min(content.len());
    while !content.is_char_boundary(end) {
        end -= 1;
    }
    content.truncate(end);
}

fn ensure_write_allowed(
    root: &Path,
    path: &Path,
    write_paths: &[String],
    forbidden_paths: &[String],
) -> Result<(), String> {
    if write_paths.is_empty() {
        return Err("repo_write_file denied: current task has no write_paths".into());
    }
    for forbidden in forbidden_paths {
        let forbidden_path = resolve_under_root(root, forbidden)?;
        if path_is_or_under(path, &forbidden_path) {
            return Err(format!(
                "repo_write_file denied: {} is forbidden by task scope {}",
                relative_to(root, path).unwrap_or_else(|| path.display().to_string()),
                forbidden
            ));
        }
    }
    let allowed = write_paths.iter().any(|allowed| {
        resolve_under_root(root, allowed)
            .map(|allowed_path| path_is_or_under(path, &allowed_path))
            .unwrap_or(false)
    });
    if allowed {
        Ok(())
    } else {
        Err(format!(
            "repo_write_file denied: {} is outside task write_paths",
            relative_to(root, path).unwrap_or_else(|| path.display().to_string())
        ))
    }
}

fn path_is_or_under(path: &Path, base: &Path) -> bool {
    path == base || path.starts_with(base)
}

pub fn git_status(root: &Path, _args: &Value) -> Result<Value, String> {
    let root = canonical_root(root)?;
    Ok(json!({
        "root": root.display().to_string(),
        "status": run_git(&root, &["status", "--short", "--branch"], MAX_GIT_BYTES)?,
    }))
}

pub fn git_log(root: &Path, args: &Value) -> Result<Value, String> {
    let root = canonical_root(root)?;
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|v| v.clamp(1, 50))
        .unwrap_or(10)
        .to_string();
    let mut git_args = vec!["log", "--oneline", "--decorate", "-n", &limit];
    let path;
    if let Some(raw) = opt_str(args, "path") {
        path = resolve_under_root(&root, raw)?;
        git_args.push("--");
        git_args.push(path.to_str().ok_or("path is not valid utf-8")?);
    }
    Ok(json!({ "log": run_git(&root, &git_args, MAX_GIT_BYTES)? }))
}

pub fn git_diff(root: &Path, args: &Value) -> Result<Value, String> {
    let root = canonical_root(root)?;
    let mut git_args = vec!["diff"];
    if args.get("staged").and_then(Value::as_bool).unwrap_or(false) {
        git_args.push("--staged");
    }
    let path;
    if let Some(raw) = opt_str(args, "path") {
        path = resolve_under_root(&root, raw)?;
        git_args.push("--");
        git_args.push(path.to_str().ok_or("path is not valid utf-8")?);
    }
    let max_bytes = args
        .get("max_bytes")
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .unwrap_or(MAX_GIT_BYTES)
        .min(MAX_GIT_BYTES);
    Ok(json!({ "diff": run_git(&root, &git_args, max_bytes)? }))
}

pub fn codebase_memory_status(root: &Path, _args: &Value) -> Result<Value, String> {
    let root = canonical_root(root)?;
    let binary = which::which("codebase-memory-mcp").ok();
    let index_markers = [
        root.join(".codebase-memory"),
        root.join(".codebase-memory-mcp"),
        root.join(".cbm"),
    ];
    let marker = index_markers
        .iter()
        .find(|p| p.exists())
        .map(|p| p.display().to_string());
    Ok(json!({
        "installed": binary.is_some(),
        "binary": binary.map(|p| p.display().to_string()),
        "index_marker": marker,
        "recommended": true,
        "install_hint": "Install optional code intelligence accelerator from https://github.com/DeusData/codebase-memory-mcp, then run `codebase-memory-mcp install` or configure it through Harness.",
    }))
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing or non-string arg: {key}"))
}

fn opt_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

fn canonical_root(root: &Path) -> Result<PathBuf, String> {
    std::fs::canonicalize(root).map_err(|e| format!("canonicalize {}: {e}", root.display()))
}

fn resolve_under_root(root: &Path, raw: &str) -> Result<PathBuf, String> {
    let root = canonical_root(root)?;
    let rel = Path::new(raw);
    if rel.is_absolute() {
        return Err("absolute paths are not allowed; use workspace-relative paths".to_string());
    }
    if rel.components().any(|c| {
        matches!(
            c,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err("path must not escape the workspace".to_string());
    }
    let candidate = root.join(rel);
    let canonical = if candidate.exists() {
        std::fs::canonicalize(&candidate)
            .map_err(|e| format!("canonicalize {}: {e}", candidate.display()))?
    } else {
        candidate
    };
    if !canonical.starts_with(&root) {
        return Err("path resolves outside the workspace".to_string());
    }
    Ok(canonical)
}

fn collect_files(root: &Path, max_depth: usize, limit: usize) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    collect_files_inner(root, root, 0, max_depth, limit, &mut out)?;
    out.sort();
    Ok(out)
}

fn collect_files_inner(
    root: &Path,
    dir: &Path,
    depth: usize,
    max_depth: usize,
    limit: usize,
    out: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if out.len() >= limit || depth > max_depth {
        return Ok(());
    }
    let mut entries = std::fs::read_dir(dir)
        .map_err(|e| format!("read_dir {}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    entries.sort_by_key(|e| e.path());
    for entry in entries {
        if out.len() >= limit {
            break;
        }
        let path = entry.path();
        let name = path.file_name().and_then(OsStr::to_str).unwrap_or("");
        if should_skip(name) {
            continue;
        }
        let ty = entry
            .file_type()
            .map_err(|e| format!("file_type {}: {e}", path.display()))?;
        if ty.is_dir() {
            collect_files_inner(root, &path, depth + 1, max_depth, limit, out)?;
        } else if ty.is_file() && path.starts_with(root) {
            out.push(path);
        }
    }
    Ok(())
}

fn should_skip(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | "node_modules"
            | "target"
            | "dist"
            | "build"
            | ".svelte-kit"
            | ".next"
            | ".cache"
            | "vendor"
    )
}

fn relative_to(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
}

fn detect_package_manager(file_names: &[String]) -> Option<&'static str> {
    if file_names.iter().any(|f| f == "pnpm-lock.yaml") {
        Some("pnpm")
    } else if file_names.iter().any(|f| f == "yarn.lock") {
        Some("yarn")
    } else if file_names.iter().any(|f| f == "package-lock.json") {
        Some("npm")
    } else if file_names.iter().any(|f| f == "Cargo.lock") {
        Some("cargo")
    } else if file_names.iter().any(|f| f == "poetry.lock") {
        Some("poetry")
    } else if file_names.iter().any(|f| f == "go.mod") {
        Some("go")
    } else {
        None
    }
}

fn detect_stack(
    file_names: &[String],
    relative_files: &[String],
    package_json: &Option<Value>,
) -> Vec<String> {
    let mut stack = Vec::new();
    if file_names.iter().any(|f| f == "Cargo.toml") {
        stack.push("rust".to_string());
    }
    if file_names.iter().any(|f| f == "package.json") {
        stack.push("node".to_string());
    }
    if file_names
        .iter()
        .any(|f| f == "pyproject.toml" || f == "requirements.txt")
    {
        stack.push("python".to_string());
    }
    if file_names.iter().any(|f| f == "go.mod") {
        stack.push("go".to_string());
    }
    if relative_files.iter().any(|f| f.ends_with(".svelte")) {
        stack.push("svelte".to_string());
    }
    if package_json_contains(package_json, "vite") {
        stack.push("vite".to_string());
    }
    if package_json_contains(package_json, "next") {
        stack.push("nextjs".to_string());
    }
    if file_names
        .iter()
        .any(|f| f == "docker-compose.yml" || f == "Dockerfile")
    {
        stack.push("docker".to_string());
    }
    stack.sort();
    stack.dedup();
    stack
}

fn package_json_contains(package_json: &Option<Value>, key: &str) -> bool {
    package_json
        .as_ref()
        .and_then(Value::as_object)
        .is_some_and(|obj| {
            ["dependencies", "devDependencies"]
                .iter()
                .filter_map(|section| obj.get(*section).and_then(Value::as_object))
                .any(|deps| deps.contains_key(key))
        })
}

fn key_files(file_names: &[String], relative_files: &[String]) -> Vec<String> {
    let important = [
        "package.json",
        "Cargo.toml",
        "pyproject.toml",
        "go.mod",
        "Justfile",
        "docker-compose.yml",
        "Dockerfile",
        "AGENTS.md",
        "ARCHITECTURE.md",
        ".env.example",
    ];
    let mut out = Vec::new();
    for name in important {
        if file_names.iter().any(|f| f == name) || relative_files.iter().any(|f| f == name) {
            out.push(name.to_string());
        }
    }
    out
}

fn read_json_if_exists(path: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn git_summary(root: &Path) -> Value {
    match gix::open(root) {
        Err(_) => json!({ "is_repo": false }),
        Ok(repo) => {
            let branch = repo.head().ok().and_then(|h| {
                h.referent_name().map(|n| {
                    n.as_bstr()
                        .to_string()
                        .trim_start_matches("refs/heads/")
                        .to_string()
                })
            });
            let head = repo
                .head_id()
                .ok()
                .map(|id| id.to_hex_with_len(8).to_string());
            let is_dirty = run_git(root, &["status", "--short"], 4096)
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            json!({
                "is_repo": true,
                "branch": branch,
                "head": head,
                "is_dirty": is_dirty,
            })
        }
    }
}

fn code_stats(root: &Path) -> Value {
    let mut langs = Languages::new();
    langs.get_statistics(&[root], &[], &Config::default());
    let mut stats: Vec<(String, Value)> = langs
        .iter()
        .map(|(lang, ls)| {
            (
                lang.to_string(),
                json!({
                    "files": ls.reports.len(),
                    "lines": ls.lines(),
                    "code": ls.code,
                    "comments": ls.comments,
                    "blanks": ls.blanks,
                }),
            )
        })
        .collect();
    stats.sort_by(|a, b| {
        b.1["code"]
            .as_u64()
            .unwrap_or(0)
            .cmp(&a.1["code"].as_u64().unwrap_or(0))
    });
    let top10: serde_json::Map<String, Value> = stats.into_iter().take(10).collect();
    Value::Object(top10)
}

fn run_git(root: &Path, args: &[&str], max_bytes: usize) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|e| format!("git {:?}: {e}", args))?;
    let bytes = if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    };
    let mut text = String::from_utf8_lossy(bytes).to_string();
    if text.len() > max_bytes {
        text.truncate(max_bytes);
        text.push_str("\n[truncated]");
    }
    Ok(text)
}
