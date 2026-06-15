//! Deterministic repo-intelligence rails.
//!
//! These tools intentionally expose a small, typed view of the workspace so
//! agents do not have to rediscover structure by reading files blindly.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use gix;
use serde_json::{json, Value};
use tokei::{Config, Languages};

const DEFAULT_SCAN_LIMIT: usize = 400;
const DEFAULT_MAX_DEPTH: usize = 4;
const DEFAULT_MAX_BYTES: usize = 64 * 1024;
const DEFAULT_FIND_LIMIT: usize = 80;
const DEFAULT_FIND_MAX_BYTES: usize = 256 * 1024;
const DEFAULT_MANIFEST_LIMIT: usize = 1000;
const DEFAULT_SYMBOL_LIMIT: usize = 80;
const SYMBOL_MAX_FILE_BYTES: u64 = 512 * 1024;
const MAX_GIT_BYTES: usize = 128 * 1024;
const MAX_GIT_MUTATION_BYTES: usize = 64 * 1024;

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

pub fn find(root: &Path, args: &Value) -> Result<Value, String> {
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
        .unwrap_or(DEFAULT_FIND_LIMIT)
        .min(DEFAULT_SCAN_LIMIT);
    let name_contains = opt_str(args, "name_contains").map(|s| s.to_ascii_lowercase());
    let content_contains = opt_str(args, "content_contains").map(|s| s.to_ascii_lowercase());
    let extensions = args
        .get("extensions")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(|s| s.trim_start_matches('.').to_ascii_lowercase())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if name_contains.is_none() && content_contains.is_none() && extensions.is_empty() {
        return Err(
            "repo_find requires at least one of name_contains, content_contains or extensions"
                .into(),
        );
    }

    let files = collect_files(&dir, max_depth, DEFAULT_SCAN_LIMIT)?;
    let mut matches = Vec::new();
    let mut scanned_content_files = 0usize;
    for file in files {
        if matches.len() >= limit {
            break;
        }
        let rel = relative_to(&dir, &file).unwrap_or_else(|| file.display().to_string());
        let file_name = file
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or("")
            .to_ascii_lowercase();
        let ext = file
            .extension()
            .and_then(OsStr::to_str)
            .unwrap_or("")
            .to_ascii_lowercase();
        if !extensions.is_empty() && !extensions.iter().any(|wanted| wanted == &ext) {
            continue;
        }
        let name_matched = name_contains
            .as_ref()
            .is_some_and(|needle| file_name.contains(needle));
        let mut content_matched = false;
        let mut line_hits = Vec::new();
        if let Some(needle) = &content_contains {
            if let Ok(metadata) = std::fs::metadata(&file) {
                if metadata.len() <= DEFAULT_FIND_MAX_BYTES as u64 {
                    if let Ok(text) = std::fs::read_to_string(&file) {
                        scanned_content_files += 1;
                        for (idx, line) in text.lines().enumerate() {
                            if line.to_ascii_lowercase().contains(needle) {
                                content_matched = true;
                                line_hits.push(json!({
                                    "line": idx + 1,
                                    "preview": truncate_preview(line),
                                }));
                                if line_hits.len() >= 5 {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
        if name_matched
            || content_matched
            || (!extensions.is_empty() && content_contains.is_none() && name_contains.is_none())
        {
            matches.push(json!({
                "path": rel,
                "name_matched": name_matched,
                "content_matched": content_matched,
                "line_hits": line_hits,
            }));
        }
    }

    Ok(json!({
        "root": dir.display().to_string(),
        "limit": limit,
        "max_depth": max_depth,
        "query": {
            "name_contains": name_contains,
            "content_contains": content_contains,
            "extensions": extensions,
        },
        "scanned_content_files": scanned_content_files,
        "matches": matches,
    }))
}

pub fn manifest(root: &Path, args: &Value) -> Result<Value, String> {
    let dir = resolve_under_root(root, opt_str(args, "path").unwrap_or("."))?;
    if !dir.is_dir() {
        return Err(format!("path is not a directory: {}", dir.display()));
    }
    let max_depth = args
        .get("max_depth")
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .unwrap_or(usize::MAX);
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .unwrap_or(DEFAULT_MANIFEST_LIMIT)
        .min(DEFAULT_MANIFEST_LIMIT);
    let files = collect_files(&dir, max_depth, limit)?;
    let git_status = git_status_map(&dir);
    let mut extensions: BTreeMap<String, usize> = BTreeMap::new();
    let mut total_bytes = 0u64;
    let mut entries = Vec::new();

    for file in files {
        let rel = relative_to(&dir, &file).unwrap_or_else(|| file.display().to_string());
        let metadata =
            std::fs::metadata(&file).map_err(|e| format!("metadata {}: {e}", file.display()))?;
        let ext = file
            .extension()
            .and_then(OsStr::to_str)
            .unwrap_or("")
            .to_ascii_lowercase();
        if !ext.is_empty() {
            *extensions.entry(ext.clone()).or_insert(0) += 1;
        }
        total_bytes = total_bytes.saturating_add(metadata.len());
        entries.push(json!({
            "path": rel,
            "extension": ext,
            "bytes": metadata.len(),
            "git_status": git_status.get(&relative_to(&dir, &file).unwrap_or_default()).cloned(),
        }));
    }

    Ok(json!({
        "root": dir.display().to_string(),
        "limit": limit,
        "truncated": entries.len() >= limit,
        "summary": {
            "files": entries.len(),
            "total_bytes": total_bytes,
            "extensions": extensions,
            "important_files": important_files(&dir),
            "git": git_summary(&dir),
        },
        "files": entries,
    }))
}

pub fn symbol_search(root: &Path, args: &Value) -> Result<Value, String> {
    let dir = resolve_under_root(root, opt_str(args, "path").unwrap_or("."))?;
    if !dir.is_dir() {
        return Err(format!("path is not a directory: {}", dir.display()));
    }
    let query = opt_str(args, "query").unwrap_or("").to_ascii_lowercase();
    let language = opt_str(args, "language").map(|s| s.to_ascii_lowercase());
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .unwrap_or(DEFAULT_SYMBOL_LIMIT)
        .min(DEFAULT_SYMBOL_LIMIT);
    let files = collect_files(&dir, usize::MAX, DEFAULT_MANIFEST_LIMIT)?;
    let mut symbols = Vec::new();

    for file in files {
        if symbols.len() >= limit {
            break;
        }
        if !is_symbol_file(&file, language.as_deref()) {
            continue;
        }
        if std::fs::metadata(&file)
            .map(|m| m.len() > SYMBOL_MAX_FILE_BYTES)
            .unwrap_or(true)
        {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let rel = relative_to(&dir, &file).unwrap_or_else(|| file.display().to_string());
        for (idx, line) in text.lines().enumerate() {
            if symbols.len() >= limit {
                break;
            }
            let Some(symbol) = parse_symbol(line, &file) else {
                continue;
            };
            if !query.is_empty()
                && !symbol.name.to_ascii_lowercase().contains(&query)
                && !rel.to_ascii_lowercase().contains(&query)
            {
                continue;
            }
            symbols.push(json!({
                "name": symbol.name,
                "kind": symbol.kind,
                "language": symbol.language,
                "path": rel,
                "line": idx + 1,
                "preview": truncate_preview(line),
            }));
        }
    }

    Ok(json!({
        "root": dir.display().to_string(),
        "query": query,
        "language": language,
        "limit": limit,
        "total": symbols.len(),
        "has_more": symbols.len() >= limit,
        "symbols": symbols,
        "engine": "lightweight-patterns",
        "hint": "For deep callers/callees and cross-language resolution, load code_graph and use codebase-memory-mcp when available.",
    }))
}

pub fn related_files(root: &Path, args: &Value) -> Result<Value, String> {
    let path = resolve_under_root(root, str_arg(args, "path")?)?;
    let root = canonical_root(root)?;
    if !path.exists() {
        return Err(format!("path does not exist: {}", path.display()));
    }
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .unwrap_or(40)
        .min(80);
    let rel = relative_to(&root, &path).unwrap_or_else(|| path.display().to_string());
    let stem = path
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    let ext = path.extension().and_then(OsStr::to_str).unwrap_or("");
    let files = collect_files(&root, usize::MAX, DEFAULT_MANIFEST_LIMIT)?;
    let mut related = Vec::new();
    let mut seen = BTreeSet::new();

    for file in files {
        if related.len() >= limit {
            break;
        }
        if file == path {
            continue;
        }
        let candidate_rel = relative_to(&root, &file).unwrap_or_else(|| file.display().to_string());
        if !seen.insert(candidate_rel.clone()) {
            continue;
        }
        let name = file
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or("")
            .to_ascii_lowercase();
        let candidate_stem = file
            .file_stem()
            .and_then(OsStr::to_str)
            .unwrap_or("")
            .to_ascii_lowercase();
        let reason = if !stem.is_empty()
            && (candidate_stem == stem
                || candidate_stem.contains(&stem)
                || stem.contains(&candidate_stem))
        {
            "matching_stem"
        } else if is_test_path(&candidate_rel) && !stem.is_empty() && name.contains(&stem) {
            "test_for_file"
        } else if same_parent(&path, &file) {
            "same_directory"
        } else if related_extension(ext, file.extension().and_then(OsStr::to_str).unwrap_or("")) {
            "related_extension"
        } else {
            continue;
        };
        related.push(json!({
            "path": candidate_rel,
            "reason": reason,
        }));
    }

    Ok(json!({
        "path": rel,
        "limit": limit,
        "related": related,
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

fn truncate_preview(line: &str) -> String {
    let trimmed = line.trim();
    let mut out = trimmed.chars().take(180).collect::<String>();
    if trimmed.chars().count() > 180 {
        out.push_str("...");
    }
    out
}

fn compact_pattern_query(query: &str) -> String {
    let compact = query
        .trim()
        .trim_start_matches(".*")
        .trim_end_matches(".*")
        .trim_start_matches('^')
        .trim_end_matches('$')
        .replace(".*", " ")
        .replace(['(', ')', '[', ']', '{', '}', '|', '\\'], " ");
    compact
        .split_whitespace()
        .next()
        .unwrap_or(query.trim())
        .to_string()
}

fn changed_paths_from_git(root: &Path) -> Result<Vec<String>, String> {
    let output = run_git(root, &["status", "--porcelain"], MAX_GIT_BYTES).unwrap_or_default();
    let mut paths = Vec::new();
    for line in output.lines() {
        if line.len() < 4 {
            continue;
        }
        if let Some(path) = line[3..].split(" -> ").last() {
            paths.push(path.to_string());
        }
    }
    if paths.is_empty() {
        let output = run_git(root, &["diff", "--name-only", "HEAD"], MAX_GIT_BYTES)?;
        paths.extend(output.lines().map(str::to_string));
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn largest_files_from_manifest(manifest: &Value, limit: usize) -> Vec<Value> {
    let mut files = manifest["files"].as_array().cloned().unwrap_or_default();
    files.sort_by(|a, b| {
        b["bytes"]
            .as_u64()
            .unwrap_or(0)
            .cmp(&a["bytes"].as_u64().unwrap_or(0))
    });
    files
        .into_iter()
        .take(limit)
        .map(|file| {
            json!({
                "path": file["path"].clone(),
                "bytes": file["bytes"].clone(),
                "extension": file["extension"].clone(),
            })
        })
        .collect()
}

fn read_line_window(
    path: &Path,
    center_line: Option<usize>,
    start_line: Option<usize>,
    end_line: Option<usize>,
    context: usize,
) -> Result<(usize, usize, String), String> {
    if !path.is_file() {
        return Err(format!("path is not a file: {}", path.display()));
    }
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let lines = text.lines().collect::<Vec<_>>();
    if lines.is_empty() {
        return Ok((1, 1, String::new()));
    }
    let mut start = start_line.unwrap_or_else(|| center_line.unwrap_or(1).saturating_sub(context));
    if start == 0 {
        start = 1;
    }
    let mut end = end_line.unwrap_or_else(|| {
        center_line
            .map(|line| line.saturating_add(context))
            .unwrap_or_else(|| (start + context * 2).saturating_sub(1))
    });
    end = end.min(lines.len());
    if start > end {
        return Err("start_line must be <= end_line".to_string());
    }
    let max_lines = 120usize;
    if end.saturating_sub(start).saturating_add(1) > max_lines {
        end = start + max_lines - 1;
    }
    let content = lines[start - 1..end].join("\n");
    Ok((start, end, content))
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

pub fn git_branch_create(root: &Path, args: &Value) -> Result<Value, String> {
    let root = git_worktree_root(root)?;
    let branch = str_arg(args, "branch")?;
    validate_branch_name(branch)?;
    let checkout = args
        .get("checkout")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let start_point = opt_str(args, "start_point");

    let mut git_args = vec![OsString::from("branch"), OsString::from(branch)];
    if let Some(start_point) = start_point {
        validate_refish(start_point, "start_point")?;
        git_args.push(OsString::from(start_point));
    }
    let output = run_git_os(&root, &git_args, MAX_GIT_MUTATION_BYTES)?;

    let checkout_output = if checkout {
        Some(run_git_os(
            &root,
            &[OsString::from("checkout"), OsString::from(branch)],
            MAX_GIT_MUTATION_BYTES,
        )?)
    } else {
        None
    };

    Ok(json!({
        "ok": true,
        "branch": branch,
        "checked_out": checkout,
        "output": output,
        "checkout_output": checkout_output,
        "status": run_git(&root, &["status", "--short", "--branch"], MAX_GIT_BYTES)?,
    }))
}

pub fn git_commit(
    root: &Path,
    args: &Value,
    write_paths: &[String],
    forbidden_paths: &[String],
) -> Result<Value, String> {
    let root = git_worktree_root(root)?;
    let message = str_arg(args, "message")?.trim();
    if message.is_empty() {
        return Err("commit message cannot be empty".into());
    }
    let paths = string_array_arg(args, "paths")?;
    if paths.is_empty() {
        return Err("repo_git_commit requires a non-empty paths array".into());
    }

    ensure_staged_paths_allowed(&root, write_paths, forbidden_paths)?;
    let resolved = resolve_git_paths(&root, &paths, write_paths, forbidden_paths)?;
    let mut add_args = vec![OsString::from("add"), OsString::from("--")];
    add_args.extend(resolved);
    let add_output = run_git_os(&root, &add_args, MAX_GIT_MUTATION_BYTES)?;
    ensure_staged_paths_allowed(&root, write_paths, forbidden_paths)?;

    let mut commit_args = vec![
        OsString::from("commit"),
        OsString::from("-m"),
        OsString::from(message),
    ];
    if args
        .get("allow_empty")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        commit_args.push(OsString::from("--allow-empty"));
    }
    let commit_output = run_git_os(&root, &commit_args, MAX_GIT_MUTATION_BYTES)?;
    let head = run_git(&root, &["rev-parse", "--short", "HEAD"], 1024)?
        .trim()
        .to_string();

    Ok(json!({
        "ok": true,
        "head": head,
        "add_output": add_output,
        "commit_output": commit_output,
        "status": run_git(&root, &["status", "--short", "--branch"], MAX_GIT_BYTES)?,
    }))
}

pub fn git_push(root: &Path, args: &Value) -> Result<Value, String> {
    let root = git_worktree_root(root)?;
    let remote = opt_str(args, "remote").unwrap_or("origin");
    validate_refish(remote, "remote")?;
    let branch = match opt_str(args, "branch") {
        Some(branch) => {
            validate_branch_name(branch)?;
            branch.to_string()
        }
        None => current_branch(&root)?,
    };
    let set_upstream = args
        .get("set_upstream")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    let mut git_args = vec![OsString::from("push")];
    if set_upstream {
        git_args.push(OsString::from("--set-upstream"));
    }
    git_args.push(OsString::from(remote));
    git_args.push(OsString::from(&branch));

    Ok(json!({
        "ok": true,
        "remote": remote,
        "branch": branch,
        "output": run_git_os(&root, &git_args, MAX_GIT_MUTATION_BYTES)?,
    }))
}

pub fn git_pr_create(root: &Path, args: &Value) -> Result<Value, String> {
    let root = git_worktree_root(root)?;
    which::which("gh").map_err(|_| {
        "repo_github_pr_create requires GitHub CLI `gh` installed and authenticated".to_string()
    })?;
    let title = str_arg(args, "title")?.trim();
    if title.is_empty() {
        return Err("PR title cannot be empty".into());
    }
    let body = opt_str(args, "body").unwrap_or("");
    let base = opt_str(args, "base");
    let head = opt_str(args, "head");
    if let Some(base) = base {
        validate_branch_name(base)?;
    }
    if let Some(head) = head {
        validate_branch_name(head)?;
    }
    let draft = args.get("draft").and_then(Value::as_bool).unwrap_or(false);

    let gh_args = build_gh_pr_create_args(title, body, base, head, draft)?;
    let output = run_command_os(&root, "gh", &gh_args, MAX_GIT_MUTATION_BYTES)?;
    Ok(json!({
        "ok": true,
        "url": first_url(&output),
        "output": output,
    }))
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

pub fn code_graph_status(root: &Path, args: &Value) -> Result<Value, String> {
    let codebase_memory = codebase_memory_status(root, args)?;
    Ok(json!({
        "available": codebase_memory["installed"].as_bool().unwrap_or(false),
        "engine": "codebase-memory-mcp",
        "upstream_mode": "persistent_mcp_when_smart_loaded",
        "persistent_upstream": true,
        "codebase_memory": codebase_memory,
        "harness_wrappers": [
            "repo_code_graph_index",
            "repo_code_graph_search",
            "repo_change_impact",
            "repo_architecture_pack",
            "repo_code_snippet"
        ],
        "upstream_tools_when_configured": [
            "codebase_memory__index_repository",
            "codebase_memory__search_graph",
            "codebase_memory__trace_path",
            "codebase_memory__detect_changes",
            "codebase_memory__get_architecture",
            "codebase_memory__get_code_snippet"
        ],
        "native_fallbacks": [
            "repo_manifest",
            "repo_symbol_search",
            "repo_related_files",
            "repo_find",
            "repo_read_file"
        ],
        "next_step": "Use Harness wrappers for compact bounded answers. If codebase-memory-mcp tools are listed, use prefixed upstream tools for deep graph traversal.",
    }))
}

pub fn code_graph_index(root: &Path, args: &Value) -> Result<Value, String> {
    let root = resolve_under_root(root, opt_str(args, "path").unwrap_or("."))?;
    if !root.is_dir() {
        return Err(format!("path is not a directory: {}", root.display()));
    }
    let codebase_memory = codebase_memory_status(&root, args)?;
    let run = args.get("run").and_then(Value::as_bool).unwrap_or(false);
    let persist = args
        .get("persistence")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let index_command = json!({
        "repo_path": root.display().to_string(),
        "persistence": persist,
    });
    let output = if run {
        if codebase_memory["installed"].as_bool().unwrap_or(false) {
            let command = codebase_memory["binary"]
                .as_str()
                .ok_or_else(|| "codebase-memory-mcp binary missing from status".to_string())?;
            let args = vec![
                OsString::from("cli"),
                OsString::from("index_repository"),
                OsString::from(index_command.to_string()),
            ];
            Some(run_command_os(&root, command, &args, MAX_GIT_BYTES)?)
        } else {
            return Err(
                "repo_code_graph_index requires codebase-memory-mcp installed when run=true".into(),
            );
        }
    } else {
        None
    };
    let manifest = manifest(&root, &json!({"limit": 200, "max_depth": 4}))?;
    Ok(json!({
        "root": root.display().to_string(),
        "run": run,
        "engine": if run { "codebase-memory-mcp-cli" } else { "native-summary" },
        "codebase_memory": codebase_memory,
        "index_command": {
            "tool": "index_repository",
            "arguments": index_command,
        },
        "output": output,
        "fallback_index": {
            "manifest_summary": manifest["summary"].clone(),
            "truncated": manifest["truncated"].clone(),
        }
    }))
}

pub fn code_graph_search(root: &Path, args: &Value) -> Result<Value, String> {
    let query = opt_str(args, "query")
        .or_else(|| opt_str(args, "name_pattern"))
        .unwrap_or("")
        .trim();
    if query.is_empty() {
        return Err("repo_code_graph_search requires query or name_pattern".into());
    }
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .unwrap_or(20)
        .min(50);
    let path = opt_str(args, "path").unwrap_or(".");
    let language = opt_str(args, "language");
    let symbol_args = json!({
        "query": compact_pattern_query(query),
        "path": path,
        "language": language,
        "limit": limit,
    });
    let symbols = symbol_search(root, &symbol_args)?;
    let files = find(
        root,
        &json!({
            "path": path,
            "name_contains": compact_pattern_query(query),
            "limit": limit,
            "max_depth": args.get("max_depth").and_then(Value::as_u64).unwrap_or(usize::MAX as u64),
        }),
    )
    .unwrap_or_else(|e| json!({ "error": e, "matches": [] }));
    Ok(json!({
        "engine": "native_fallback",
        "query": query,
        "limit": limit,
        "symbols": symbols["symbols"].clone(),
        "files": files["matches"].clone(),
        "upstream_hint": "If codebase_memory__search_graph is visible, use it for callers/callees, semantic search, degree filters, and cross-repo graph relationships.",
    }))
}

pub fn change_impact(root: &Path, args: &Value) -> Result<Value, String> {
    let root = canonical_root(root)?;
    let explicit_paths = args
        .get("paths")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).collect::<Vec<_>>())
        .unwrap_or_default();
    let paths = if explicit_paths.is_empty() {
        changed_paths_from_git(&root)?
    } else {
        explicit_paths.into_iter().map(str::to_string).collect()
    };
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .unwrap_or(40)
        .min(80);
    let mut impacted = Vec::new();
    let mut tests = BTreeSet::new();
    let mut related_paths = BTreeSet::new();
    let mut symbols = Vec::new();
    for raw in paths.iter().take(limit) {
        let path = resolve_under_root(&root, raw)?;
        let rel = relative_to(&root, &path).unwrap_or_else(|| path.display().to_string());
        let related = related_files(&root, &json!({"path": rel, "limit": 20}))
            .unwrap_or_else(|e| json!({ "error": e, "related": [] }));
        for item in related["related"].as_array().into_iter().flatten() {
            if let Some(path) = item["path"].as_str() {
                if is_test_path(path) {
                    tests.insert(path.to_string());
                }
                related_paths.insert(path.to_string());
            }
        }
        let stem = path.file_stem().and_then(OsStr::to_str).unwrap_or("");
        if !stem.is_empty() {
            let symbol_hits = symbol_search(&root, &json!({"query": stem, "limit": 10}))
                .unwrap_or_else(|e| json!({ "error": e, "symbols": [] }));
            for symbol in symbol_hits["symbols"].as_array().into_iter().flatten() {
                symbols.push(symbol.clone());
            }
        }
        impacted.push(json!({
            "path": rel,
            "exists": path.exists(),
            "related": related["related"].clone(),
        }));
    }
    Ok(json!({
        "engine": "native_fallback",
        "changed_files": paths,
        "impacted_files": impacted,
        "likely_tests": tests.into_iter().collect::<Vec<_>>(),
        "related_paths": related_paths.into_iter().take(limit).collect::<Vec<_>>(),
        "symbols": symbols.into_iter().take(limit).collect::<Vec<_>>(),
        "upstream_hint": "If codebase_memory__detect_changes is visible, use it for graph-backed blast radius and risk classification.",
    }))
}

pub fn architecture_pack(root: &Path, args: &Value) -> Result<Value, String> {
    let path = opt_str(args, "path").unwrap_or(".");
    let dir = resolve_under_root(root, path)?;
    if !dir.is_dir() {
        return Err(format!("path is not a directory: {}", dir.display()));
    }
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .unwrap_or(80)
        .min(200);
    let manifest = manifest(&dir, &json!({"limit": limit, "max_depth": 6}))?;
    let analyze = analyze(&dir, &json!({}))?;
    let hotspots = largest_files_from_manifest(&manifest, 12);
    Ok(json!({
        "engine": "native_fallback",
        "root": dir.display().to_string(),
        "stack": analyze["stack"].clone(),
        "package_manager": analyze["package_manager"].clone(),
        "scripts": analyze["scripts"].clone(),
        "key_files": analyze["key_files"].clone(),
        "git": analyze["git"].clone(),
        "code_stats": analyze["code_stats"].clone(),
        "manifest_summary": manifest["summary"].clone(),
        "hotspots": hotspots,
        "upstream_hint": "If codebase_memory__get_architecture is visible, use it for graph-backed routes, packages, layers, clusters, ADRs, and boundaries.",
    }))
}

pub fn code_snippet(root: &Path, args: &Value) -> Result<Value, String> {
    let root = canonical_root(root)?;
    let context = args
        .get("context_lines")
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .unwrap_or(8)
        .min(30);
    let (path, center_line, symbol) = if let Some(path) = opt_str(args, "path") {
        let path = resolve_under_root(&root, path)?;
        let line = args.get("line").and_then(Value::as_u64).map(|v| v as usize);
        (path, line, None)
    } else {
        let symbol = str_arg(args, "symbol")?;
        let hits = symbol_search(&root, &json!({"query": symbol, "limit": 1}))?;
        let first = hits["symbols"]
            .as_array()
            .and_then(|items| items.first())
            .ok_or_else(|| format!("symbol not found: {symbol}"))?;
        let path = first["path"]
            .as_str()
            .ok_or_else(|| "symbol search returned hit without path".to_string())?;
        let line = first["line"].as_u64().map(|v| v as usize);
        (resolve_under_root(&root, path)?, line, Some(first.clone()))
    };
    let start = args
        .get("start_line")
        .and_then(Value::as_u64)
        .map(|v| v as usize);
    let end = args
        .get("end_line")
        .and_then(Value::as_u64)
        .map(|v| v as usize);
    let snippet = read_line_window(&path, center_line, start, end, context)?;
    Ok(json!({
        "engine": "native_fallback",
        "path": relative_to(&root, &path).unwrap_or_else(|| path.display().to_string()),
        "symbol": symbol,
        "start_line": snippet.0,
        "end_line": snippet.1,
        "content": snippet.2,
        "upstream_hint": "If codebase_memory__get_code_snippet is visible and you have qualified_name from search_graph, use it for precise graph-backed symbol snippets.",
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

fn git_worktree_root(root: &Path) -> Result<PathBuf, String> {
    let root = canonical_root(root)?;
    let top = run_git(&root, &["rev-parse", "--show-toplevel"], 4096)?;
    let top = top.trim();
    if top.is_empty() {
        return Err("workspace is not inside a git worktree".into());
    }
    let top = PathBuf::from(top);
    let canonical_top =
        std::fs::canonicalize(&top).map_err(|e| format!("canonicalize git root {top:?}: {e}"))?;
    if !root.starts_with(&canonical_top) && !canonical_top.starts_with(&root) {
        return Err("resolved git worktree is outside the workspace".into());
    }
    Ok(canonical_top)
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

fn important_files(root: &Path) -> Vec<String> {
    let candidates = [
        "AGENTS.md",
        "DESIGN.md",
        "README.md",
        "Cargo.toml",
        "package.json",
        "pnpm-lock.yaml",
        "Justfile",
        "docker-compose.yml",
        ".env",
        ".env.example",
    ];
    candidates
        .iter()
        .filter(|path| root.join(path).exists())
        .map(|path| (*path).to_string())
        .collect()
}

fn git_status_map(root: &Path) -> BTreeMap<String, String> {
    let Ok(status) = run_git(root, &["status", "--porcelain"], MAX_GIT_BYTES) else {
        return BTreeMap::new();
    };
    status
        .lines()
        .filter_map(|line| {
            if line.len() < 4 {
                return None;
            }
            let status = line[..2].trim().to_string();
            let path = line[3..].split(" -> ").last()?.to_string();
            Some((path, status))
        })
        .collect()
}

#[derive(Debug)]
struct SymbolHit {
    name: String,
    kind: &'static str,
    language: &'static str,
}

fn is_symbol_file(path: &Path, language: Option<&str>) -> bool {
    let ext = path
        .extension()
        .and_then(OsStr::to_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    let inferred = match ext.as_str() {
        "rs" => "rust",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" => "javascript",
        "svelte" => "svelte",
        "py" => "python",
        "go" => "go",
        _ => return false,
    };
    match language {
        Some(wanted) => wanted == inferred || wanted == ext,
        None => true,
    }
}

fn parse_symbol(line: &str, path: &Path) -> Option<SymbolHit> {
    let ext = path.extension().and_then(OsStr::to_str).unwrap_or("");
    let trimmed = line.trim_start();
    match ext {
        "rs" => parse_after_keywords(
            trimmed,
            "rust",
            &[
                ("fn ", "function"),
                ("struct ", "struct"),
                ("enum ", "enum"),
                ("trait ", "trait"),
                ("mod ", "module"),
            ],
        ),
        "ts" | "tsx" | "js" | "jsx" => parse_after_keywords(
            trimmed,
            if ext.starts_with('t') {
                "typescript"
            } else {
                "javascript"
            },
            &[
                ("export function ", "function"),
                ("function ", "function"),
                ("export class ", "class"),
                ("class ", "class"),
                ("export const ", "const"),
                ("const ", "const"),
                ("export let ", "let"),
                ("let ", "let"),
                ("export type ", "type"),
                ("type ", "type"),
                ("export interface ", "interface"),
                ("interface ", "interface"),
            ],
        ),
        "svelte" => parse_after_keywords(
            trimmed,
            "svelte",
            &[
                ("export let ", "prop"),
                ("let ", "state"),
                ("const ", "const"),
                ("function ", "function"),
            ],
        ),
        "py" => parse_after_keywords(
            trimmed,
            "python",
            &[("def ", "function"), ("class ", "class")],
        ),
        "go" => parse_after_keywords(trimmed, "go", &[("func ", "function"), ("type ", "type")]),
        _ => None,
    }
}

fn parse_after_keywords(
    line: &str,
    language: &'static str,
    patterns: &[(&'static str, &'static str)],
) -> Option<SymbolHit> {
    for (prefix, kind) in patterns {
        let line = line
            .strip_prefix("pub ")
            .or_else(|| line.strip_prefix("pub(crate) "))
            .unwrap_or(line);
        let Some(rest) = line.strip_prefix(prefix) else {
            continue;
        };
        let name = rest
            .trim_start()
            .trim_start_matches("async ")
            .trim_start_matches('*')
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
            .collect::<String>();
        if !name.is_empty() {
            return Some(SymbolHit {
                name,
                kind,
                language,
            });
        }
    }
    None
}

fn is_test_path(path: &str) -> bool {
    let path = path.to_ascii_lowercase();
    path.contains("/test")
        || path.contains("/tests/")
        || path.contains("__tests__")
        || path.ends_with("_test.rs")
        || path.ends_with(".test.ts")
        || path.ends_with(".spec.ts")
        || path.ends_with(".test.js")
        || path.ends_with(".spec.js")
}

fn same_parent(a: &Path, b: &Path) -> bool {
    a.parent().is_some() && a.parent() == b.parent()
}

fn related_extension(a: &str, b: &str) -> bool {
    matches!(
        (a, b),
        ("svelte", "ts")
            | ("svelte", "css")
            | ("ts", "svelte")
            | ("ts", "tsx")
            | ("tsx", "ts")
            | ("rs", "toml")
            | ("toml", "rs")
    )
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
    let args = args.iter().map(OsString::from).collect::<Vec<_>>();
    run_git_os(root, &args, max_bytes)
}

fn run_git_os(root: &Path, args: &[OsString], max_bytes: usize) -> Result<String, String> {
    run_command_os(root, "git", args, max_bytes)
}

fn run_command_os(
    root: &Path,
    program: &str,
    args: &[OsString],
    max_bytes: usize,
) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|e| format!("{program} {:?}: {e}", display_args(args)))?;
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
    if output.status.success() {
        Ok(text)
    } else {
        Err(format!(
            "{program} {:?} failed with status {}: {}",
            display_args(args),
            output.status,
            text.trim()
        ))
    }
}

fn display_args(args: &[OsString]) -> Vec<String> {
    args.iter()
        .map(|arg| arg.to_string_lossy().into())
        .collect()
}

fn validate_branch_name(branch: &str) -> Result<(), String> {
    let trimmed = branch.trim();
    if trimmed.is_empty() || trimmed != branch {
        return Err("branch name cannot be empty or padded".into());
    }
    if branch.starts_with('-')
        || branch.starts_with('/')
        || branch.ends_with('/')
        || branch.contains("..")
        || branch.contains("@{")
        || branch.contains('\\')
        || branch.ends_with(".lock")
        || branch.split('/').any(|part| part == "." || part == "..")
        || branch.chars().any(|c| {
            c.is_control() || c.is_whitespace() || matches!(c, '~' | '^' | ':' | '?' | '*' | '[')
        })
    {
        return Err(format!("invalid branch name: {branch}"));
    }
    Ok(())
}

fn validate_refish(value: &str, label: &str) -> Result<(), String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed != value || value.starts_with('-') {
        return Err(format!("{label} cannot be empty, padded, or option-like"));
    }
    if value
        .chars()
        .any(|c| c.is_control() || c.is_whitespace() || c == '\0')
    {
        return Err(format!(
            "{label} cannot contain whitespace/control characters"
        ));
    }
    Ok(())
}

fn string_array_arg(args: &Value, key: &str) -> Result<Vec<String>, String> {
    let Some(value) = args.get(key) else {
        return Ok(Vec::new());
    };
    let items = value
        .as_array()
        .ok_or_else(|| format!("{key} must be an array of strings"))?;
    items
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("{key} must be an array of strings"))
        })
        .collect()
}

fn resolve_git_paths(
    root: &Path,
    raw_paths: &[String],
    write_paths: &[String],
    forbidden_paths: &[String],
) -> Result<Vec<OsString>, String> {
    raw_paths
        .iter()
        .map(|path| {
            let resolved = resolve_under_root(root, path)?;
            ensure_write_allowed(root, &resolved, write_paths, forbidden_paths)?;
            relative_to(root, &resolved)
                .map(OsString::from)
                .ok_or_else(|| format!("path resolves outside git root: {path}"))
        })
        .collect()
}

fn ensure_staged_paths_allowed(
    root: &Path,
    write_paths: &[String],
    forbidden_paths: &[String],
) -> Result<(), String> {
    let raw = run_git_os(
        root,
        &[
            OsString::from("diff"),
            OsString::from("--cached"),
            OsString::from("--name-only"),
            OsString::from("-z"),
        ],
        MAX_GIT_BYTES,
    )?;
    let mut violations = Vec::new();
    for rel in raw.split('\0').filter(|path| !path.is_empty()) {
        let resolved = resolve_under_root(root, rel)?;
        if let Err(reason) = ensure_write_allowed(root, &resolved, write_paths, forbidden_paths) {
            violations.push(json!({
                "path": rel,
                "reason": reason,
            }));
        }
    }

    if violations.is_empty() {
        return Ok(());
    }

    Err(format!(
        "repo_git_commit denied: staged scope drift detected: {}",
        serde_json::to_string(&violations).unwrap_or_else(|_| "[]".to_string())
    ))
}

fn current_branch(root: &Path) -> Result<String, String> {
    let branch = run_git(root, &["branch", "--show-current"], 1024)?
        .trim()
        .to_string();
    if branch.is_empty() {
        Err("cannot infer current branch while HEAD is detached; pass branch explicitly".into())
    } else {
        Ok(branch)
    }
}

fn build_gh_pr_create_args(
    title: &str,
    body: &str,
    base: Option<&str>,
    head: Option<&str>,
    draft: bool,
) -> Result<Vec<OsString>, String> {
    let mut args = vec![
        OsString::from("pr"),
        OsString::from("create"),
        OsString::from("--title"),
        OsString::from(title),
        OsString::from("--body"),
        OsString::from(body),
    ];
    if let Some(base) = base {
        args.push(OsString::from("--base"));
        args.push(OsString::from(base));
    }
    if let Some(head) = head {
        args.push(OsString::from("--head"));
        args.push(OsString::from(head));
    }
    if draft {
        args.push(OsString::from("--draft"));
    }
    Ok(args)
}

fn first_url(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .find(|word| word.starts_with("https://") || word.starts_with("http://"))
        .map(|word| {
            word.trim_matches(|c: char| c == '"' || c == '\'' || c == ',')
                .to_string()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn branch_validation_blocks_option_like_and_path_escape_refs() {
        assert!(validate_branch_name("feature/git-tools").is_ok());
        assert!(validate_branch_name("-bad").is_err());
        assert!(validate_branch_name("bad branch").is_err());
        assert!(validate_branch_name("bad..branch").is_err());
        assert!(validate_branch_name("bad@{branch").is_err());
    }

    #[test]
    fn gh_pr_create_args_are_vectorized() {
        let args = build_gh_pr_create_args(
            "Add git tools",
            "body text",
            Some("main"),
            Some("feature/git-tools"),
            true,
        )
        .unwrap();
        let display = display_args(&args);
        assert_eq!(
            display,
            vec![
                "pr",
                "create",
                "--title",
                "Add git tools",
                "--body",
                "body text",
                "--base",
                "main",
                "--head",
                "feature/git-tools",
                "--draft"
            ]
        );
    }

    #[test]
    fn git_commit_requires_explicit_staging_scope() {
        let root = tempfile::tempdir().unwrap();
        run_git(root.path(), &["init"], 4096).unwrap();
        let err = git_commit(
            root.path(),
            &json!({ "message": "test" }),
            &[".".into()],
            &[],
        )
        .unwrap_err();
        assert!(err.contains("non-empty paths array"));
    }

    #[test]
    fn git_commit_denies_staged_paths_outside_task_scope() {
        let root = tempfile::tempdir().unwrap();
        run_git(root.path(), &["init"], 4096).unwrap();
        std::fs::create_dir_all(root.path().join("src")).unwrap();
        std::fs::write(root.path().join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
        std::fs::write(root.path().join("README.md"), "drift\n").unwrap();
        run_git(root.path(), &["add", "README.md"], 4096).unwrap();

        let err = git_commit(
            root.path(),
            &json!({
                "message": "scoped commit",
                "paths": ["src/lib.rs"]
            }),
            &["src".into()],
            &[],
        )
        .unwrap_err();

        assert!(err.contains("staged scope drift"));
        assert!(err.contains("README.md"));
    }

    #[test]
    fn git_commit_denies_staged_forbidden_subpath() {
        let root = tempfile::tempdir().unwrap();
        run_git(root.path(), &["init"], 4096).unwrap();
        std::fs::create_dir_all(root.path().join("src/secrets")).unwrap();
        std::fs::write(root.path().join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
        std::fs::write(root.path().join("src/secrets/token.txt"), "secret\n").unwrap();
        run_git(root.path(), &["add", "src/secrets/token.txt"], 4096).unwrap();

        let err = git_commit(
            root.path(),
            &json!({
                "message": "scoped commit",
                "paths": ["src/lib.rs"]
            }),
            &["src".into()],
            &["src/secrets".into()],
        )
        .unwrap_err();

        assert!(err.contains("staged scope drift"));
        assert!(err.contains("src/secrets/token.txt"));
    }

    #[test]
    fn manifest_symbol_search_and_related_files_are_bounded() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("src")).unwrap();
        std::fs::create_dir_all(root.path().join("tests")).unwrap();
        std::fs::write(
            root.path().join("src/lib.rs"),
            "pub fn alpha() {}\nstruct Beta;\n",
        )
        .unwrap();
        std::fs::write(root.path().join("tests/lib_test.rs"), "use app::alpha;\n").unwrap();

        let manifest = manifest(root.path(), &json!({ "limit": 10 })).unwrap();
        assert_eq!(manifest["summary"]["files"].as_u64(), Some(2));
        assert!(manifest["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["path"] == "src/lib.rs"));

        let symbols = symbol_search(root.path(), &json!({ "query": "alpha" })).unwrap();
        assert!(symbols["symbols"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["name"] == "alpha"));

        let related = related_files(root.path(), &json!({ "path": "src/lib.rs" })).unwrap();
        assert!(related["related"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["path"] == "tests/lib_test.rs"));
    }

    #[test]
    fn code_graph_wrappers_return_bounded_native_fallbacks() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("src")).unwrap();
        std::fs::create_dir_all(root.path().join("tests")).unwrap();
        std::fs::write(
            root.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\n",
        )
        .unwrap();
        std::fs::write(
            root.path().join("src/lib.rs"),
            "pub fn alpha_service() {\n    beta_helper();\n}\n\nfn beta_helper() {}\n",
        )
        .unwrap();
        std::fs::write(
            root.path().join("tests/lib_test.rs"),
            "use demo::alpha_service;\n",
        )
        .unwrap();
        run_git(root.path(), &["init"], 4096).unwrap();

        let indexed = code_graph_index(root.path(), &json!({})).unwrap();
        assert_eq!(indexed["run"], false);
        assert_eq!(indexed["engine"], "native-summary");

        let search =
            code_graph_search(root.path(), &json!({"query": "alpha", "limit": 5})).unwrap();
        assert_eq!(search["engine"], "native_fallback");
        assert!(search["symbols"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["name"] == "alpha_service"));

        let snippet = code_snippet(
            root.path(),
            &json!({"symbol": "alpha_service", "context_lines": 1}),
        )
        .unwrap();
        assert_eq!(snippet["path"], "src/lib.rs");
        assert!(snippet["content"]
            .as_str()
            .unwrap()
            .contains("alpha_service"));

        std::fs::write(
            root.path().join("src/lib.rs"),
            "pub fn alpha_service() {\n    beta_helper();\n    beta_helper();\n}\n\nfn beta_helper() {}\n",
        )
        .unwrap();
        let impact = change_impact(root.path(), &json!({"paths": ["src/lib.rs"]})).unwrap();
        assert!(impact["likely_tests"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "tests/lib_test.rs"));

        let pack = architecture_pack(root.path(), &json!({"limit": 20})).unwrap();
        assert!(pack["stack"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "rust"));
        assert!(pack["hotspots"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["path"] == "src/lib.rs"));
    }
}
