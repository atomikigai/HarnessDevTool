use std::ffi::OsStr;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use harness_core::{infer_docs_backend, DocsBackend};
use serde_json::{json, Value};

const DEFAULT_SOURCE_DIR: &str = "docs";
const DEFAULT_OUTPUT_DIR: &str = "docs-site";

pub fn build(root: &Path, args: &Value) -> Result<Value, String> {
    let root = canonical_root(root)?;
    let source_dir = resolve_under_root(
        &root,
        opt_str(args, "source_dir").unwrap_or(DEFAULT_SOURCE_DIR),
    )?;
    if !source_dir.is_dir() {
        return Err(format!(
            "source_dir is not a directory: {}",
            source_dir.display()
        ));
    }
    let output_dir = resolve_under_root(
        &root,
        opt_str(args, "output_dir").unwrap_or(DEFAULT_OUTPUT_DIR),
    )?;
    ensure_not_overlapping(&source_dir, &output_dir)?;

    let stack = detect_workspace_stack(&root)?;
    let backend = match opt_str(args, "backend") {
        None | Some("auto") => infer_docs_backend(&stack),
        Some(raw) => raw.parse::<DocsBackend>()?,
    };
    let title = opt_str(args, "title").unwrap_or("Project Docs");
    let install = args
        .get("install")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let run_build = args
        .get("run_build")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    let copied = match backend {
        DocsBackend::Starlight => scaffold_starlight(&source_dir, &output_dir, title)?,
        DocsBackend::Mdbook => scaffold_mdbook(&source_dir, &output_dir, title)?,
        DocsBackend::Vitepress => scaffold_vitepress(&source_dir, &output_dir, title)?,
    };

    let build = if run_build {
        run_backend_build(backend, &output_dir, install)?
    } else {
        BuildOutcome::skipped("run_build=false")
    };

    Ok(json!({
        "backend": backend.as_str(),
        "stack": stack,
        "source_dir": relative_to(&root, &source_dir).unwrap_or_else(|| source_dir.display().to_string()),
        "output_dir": relative_to(&root, &output_dir).unwrap_or_else(|| output_dir.display().to_string()),
        "site_dir": site_dir_for(backend, &root, &output_dir),
        "copied_markdown_files": copied,
        "build_ran": build.ran,
        "build_ok": build.ok,
        "build_skipped_reason": build.skipped_reason,
        "stdout": build.stdout,
        "stderr": build.stderr,
    }))
}

fn scaffold_starlight(source_dir: &Path, output_dir: &Path, title: &str) -> Result<usize, String> {
    std::fs::create_dir_all(output_dir)
        .map_err(|e| format!("create {}: {e}", output_dir.display()))?;
    write_if_absent(
        &output_dir.join("package.json"),
        &serde_json::to_string_pretty(&json!({
            "type": "module",
            "scripts": {
                "dev": "astro dev",
                "build": "astro build",
                "preview": "astro preview"
            },
            "dependencies": {
                "@astrojs/starlight": "latest",
                "astro": "latest"
            }
        }))
        .map_err(|e| e.to_string())?,
    )?;
    write_if_absent(
        &output_dir.join("astro.config.mjs"),
        &format!(
            "import {{ defineConfig }} from 'astro/config';\nimport starlight from '@astrojs/starlight';\n\nexport default defineConfig({{\n  integrations: [\n    starlight({{\n      title: {},\n    }}),\n  ],\n}});\n",
            js_string(title)
        ),
    )?;
    write_if_absent(
        &output_dir.join("src/content.config.ts"),
        "import { defineCollection } from 'astro:content';\nimport { docsLoader, i18nLoader } from '@astrojs/starlight/loaders';\nimport { docsSchema, i18nSchema } from '@astrojs/starlight/schema';\n\nexport const collections = {\n  docs: defineCollection({ loader: docsLoader(), schema: docsSchema() }),\n  i18n: defineCollection({ loader: i18nLoader(), schema: i18nSchema() }),\n};\n",
    )?;
    copy_markdown_tree(
        source_dir,
        &output_dir.join("src/content/docs"),
        MarkdownMode::Frontmatter,
    )
}

fn scaffold_mdbook(source_dir: &Path, output_dir: &Path, title: &str) -> Result<usize, String> {
    std::fs::create_dir_all(output_dir)
        .map_err(|e| format!("create {}: {e}", output_dir.display()))?;
    write_if_absent(
        &output_dir.join("book.toml"),
        &format!(
            "[book]\ntitle = {}\nlanguage = \"en\"\nsrc = \"src\"\n\n[build]\nbuild-dir = \"dist\"\n",
            toml_string(title)
        ),
    )?;
    let target = output_dir.join("src");
    let copied = copy_markdown_tree(source_dir, &target, MarkdownMode::Plain)?;
    write_if_absent(
        &target.join("SUMMARY.md"),
        "# Summary\n\n- [Home](index.md)\n",
    )?;
    if !target.join("index.md").exists() {
        write_if_absent(&target.join("index.md"), &format!("# {title}\n"))?;
    }
    Ok(copied)
}

fn scaffold_vitepress(source_dir: &Path, output_dir: &Path, title: &str) -> Result<usize, String> {
    std::fs::create_dir_all(output_dir)
        .map_err(|e| format!("create {}: {e}", output_dir.display()))?;
    write_if_absent(
        &output_dir.join("package.json"),
        &serde_json::to_string_pretty(&json!({
            "type": "module",
            "scripts": {
                "dev": "vitepress dev docs",
                "build": "vitepress build docs",
                "preview": "vitepress preview docs"
            },
            "devDependencies": {
                "vitepress": "latest"
            }
        }))
        .map_err(|e| e.to_string())?,
    )?;
    write_if_absent(
        &output_dir.join("docs/.vitepress/config.mts"),
        &format!(
            "import {{ defineConfig }} from 'vitepress';\n\nexport default defineConfig({{\n  title: {},\n}});\n",
            js_string(title)
        ),
    )?;
    copy_markdown_tree(source_dir, &output_dir.join("docs"), MarkdownMode::Plain)
}

#[derive(Clone, Copy)]
enum MarkdownMode {
    Plain,
    Frontmatter,
}

fn copy_markdown_tree(
    source_dir: &Path,
    target_dir: &Path,
    mode: MarkdownMode,
) -> Result<usize, String> {
    let mut files = Vec::new();
    collect_markdown(source_dir, source_dir, &mut files)?;
    if files.is_empty() {
        return Err(format!(
            "source_dir has no markdown files: {}",
            source_dir.display()
        ));
    }
    std::fs::create_dir_all(target_dir)
        .map_err(|e| format!("create {}: {e}", target_dir.display()))?;
    for source in &files {
        let rel = source.strip_prefix(source_dir).map_err(|e| e.to_string())?;
        let target = target_dir.join(rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create {}: {e}", parent.display()))?;
        }
        let mut text = std::fs::read_to_string(source)
            .map_err(|e| format!("read {}: {e}", source.display()))?;
        if matches!(mode, MarkdownMode::Frontmatter) {
            text = ensure_frontmatter(&text, source);
        }
        write_file(&target, &text)?;
    }
    Ok(files.len())
}

fn collect_markdown(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let mut entries = std::fs::read_dir(dir)
        .map_err(|e| format!("read_dir {}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        let name = path.file_name().and_then(OsStr::to_str).unwrap_or("");
        if should_skip(name) {
            continue;
        }
        let ty = entry
            .file_type()
            .map_err(|e| format!("file_type {}: {e}", path.display()))?;
        if ty.is_dir() {
            collect_markdown(root, &path, out)?;
        } else if ty.is_file()
            && matches!(path.extension().and_then(OsStr::to_str), Some("md" | "mdx"))
            && path.starts_with(root)
        {
            out.push(path);
        }
    }
    Ok(())
}

fn ensure_frontmatter(text: &str, source: &Path) -> String {
    if text.trim_start().starts_with("---") {
        return text.to_string();
    }
    let title = text
        .lines()
        .find_map(|line| line.strip_prefix("# ").map(str::trim))
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            source
                .file_stem()
                .and_then(OsStr::to_str)
                .unwrap_or("Untitled")
                .replace(['-', '_'], " ")
        });
    format!("---\ntitle: {}\n---\n\n{}", toml_string(&title), text)
}

fn run_backend_build(
    backend: DocsBackend,
    output_dir: &Path,
    install: bool,
) -> Result<BuildOutcome, String> {
    match backend {
        DocsBackend::Starlight | DocsBackend::Vitepress => run_node_build(output_dir, install),
        DocsBackend::Mdbook => run_mdbook_build(output_dir),
    }
}

fn run_node_build(output_dir: &Path, install: bool) -> Result<BuildOutcome, String> {
    let pnpm = match which::which("pnpm") {
        Ok(path) => path,
        Err(_) => return Ok(BuildOutcome::skipped("pnpm not found on PATH")),
    };
    if install {
        let install_out = Command::new(&pnpm)
            .arg("install")
            .current_dir(output_dir)
            .output()
            .map_err(|e| format!("run pnpm install: {e}"))?;
        if !install_out.status.success() {
            return Ok(BuildOutcome::from_output(true, install_out));
        }
    } else if !output_dir.join("node_modules").is_dir() {
        return Ok(BuildOutcome::skipped(
            "node_modules missing; rerun with install=true or run pnpm install in output_dir",
        ));
    }
    let output = Command::new(&pnpm)
        .arg("build")
        .current_dir(output_dir)
        .output()
        .map_err(|e| format!("run pnpm build: {e}"))?;
    Ok(BuildOutcome::from_output(true, output))
}

fn run_mdbook_build(output_dir: &Path) -> Result<BuildOutcome, String> {
    let mdbook = match which::which("mdbook") {
        Ok(path) => path,
        Err(_) => return Ok(BuildOutcome::skipped("mdbook not found on PATH")),
    };
    let output = Command::new(mdbook)
        .arg("build")
        .current_dir(output_dir)
        .output()
        .map_err(|e| format!("run mdbook build: {e}"))?;
    Ok(BuildOutcome::from_output(true, output))
}

struct BuildOutcome {
    ran: bool,
    ok: bool,
    skipped_reason: Option<String>,
    stdout: String,
    stderr: String,
}

impl BuildOutcome {
    fn skipped(reason: impl Into<String>) -> Self {
        Self {
            ran: false,
            ok: false,
            skipped_reason: Some(reason.into()),
            stdout: String::new(),
            stderr: String::new(),
        }
    }

    fn from_output(ran: bool, output: std::process::Output) -> Self {
        Self {
            ran,
            ok: output.status.success(),
            skipped_reason: None,
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        }
    }
}

fn detect_workspace_stack(root: &Path) -> Result<Vec<String>, String> {
    let mut stack = Vec::new();
    detect_workspace_stack_inner(root, root, 0, &mut stack)?;
    stack.sort();
    stack.dedup();
    Ok(stack)
}

fn detect_workspace_stack_inner(
    root: &Path,
    dir: &Path,
    depth: usize,
    stack: &mut Vec<String>,
) -> Result<(), String> {
    if depth > 4 {
        return Ok(());
    }
    let mut entries = std::fs::read_dir(dir)
        .map_err(|e| format!("read_dir {}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        let name = path.file_name().and_then(OsStr::to_str).unwrap_or("");
        if should_skip(name) {
            continue;
        }
        if name == "Cargo.toml" {
            stack.push("rust".to_string());
        } else if name == "package.json" {
            stack.push("node".to_string());
        } else if name == "pyproject.toml" || name == "requirements.txt" {
            stack.push("python".to_string());
        } else if name == "go.mod" {
            stack.push("go".to_string());
        }
        let ty = entry
            .file_type()
            .map_err(|e| format!("file_type {}: {e}", path.display()))?;
        if ty.is_dir() && path.starts_with(root) {
            detect_workspace_stack_inner(root, &path, depth + 1, stack)?;
        }
    }
    Ok(())
}

fn site_dir_for(backend: DocsBackend, root: &Path, output_dir: &Path) -> String {
    let site = match backend {
        DocsBackend::Starlight => output_dir.join("dist"),
        DocsBackend::Mdbook => output_dir.join("dist"),
        DocsBackend::Vitepress => output_dir.join("docs/.vitepress/dist"),
    };
    relative_to(root, &site).unwrap_or_else(|| site.display().to_string())
}

fn write_if_absent(path: &Path, content: &str) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }
    write_file(path, content)
}

fn write_file(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(OsStr::to_str)
            .unwrap_or("harness")
    ));
    {
        let mut file =
            std::fs::File::create(&tmp).map_err(|e| format!("create {}: {e}", tmp.display()))?;
        file.write_all(content.as_bytes())
            .map_err(|e| format!("write {}: {e}", tmp.display()))?;
        file.sync_all()
            .map_err(|e| format!("fsync {}: {e}", tmp.display()))?;
    }
    std::fs::rename(&tmp, path)
        .map_err(|e| format!("rename {} -> {}: {e}", tmp.display(), path.display()))
}

fn canonical_root(root: &Path) -> Result<PathBuf, String> {
    std::fs::canonicalize(root).map_err(|e| format!("canonicalize {}: {e}", root.display()))
}

fn resolve_under_root(root: &Path, raw: &str) -> Result<PathBuf, String> {
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
    if !canonical.starts_with(root) {
        return Err("path resolves outside the workspace".to_string());
    }
    Ok(canonical)
}

fn ensure_not_overlapping(source: &Path, output: &Path) -> Result<(), String> {
    if source == output || source.starts_with(output) || output.starts_with(source) {
        return Err("source_dir and output_dir must not overlap".to_string());
    }
    Ok(())
}

fn relative_to(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
}

fn opt_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
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

fn js_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"Project Docs\"".to_string())
}

fn toml_string(value: &str) -> String {
    js_string(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starlight_scaffold_copies_markdown_with_frontmatter() {
        let dir = tempfile::tempdir().unwrap();
        let docs = dir.path().join("docs");
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::write(docs.join("intro.md"), "# Intro\n\nHello").unwrap();

        let result = build(
            dir.path(),
            &json!({
                "source_dir": "docs",
                "output_dir": "docs-site",
                "backend": "starlight",
                "run_build": false
            }),
        )
        .unwrap();

        assert_eq!(result["backend"], "starlight");
        assert_eq!(result["copied_markdown_files"], 1);
        let copied =
            std::fs::read_to_string(dir.path().join("docs-site/src/content/docs/intro.md"))
                .unwrap();
        assert!(copied.starts_with("---\ntitle: \"Intro\"\n---"));
        assert!(dir.path().join("docs-site/astro.config.mjs").is_file());
    }

    #[test]
    fn build_rejects_overlapping_paths() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("docs")).unwrap();
        std::fs::write(dir.path().join("docs/index.md"), "# Index").unwrap();
        let err = build(
            dir.path(),
            &json!({
                "source_dir": "docs",
                "output_dir": "docs/site",
                "run_build": false
            }),
        )
        .unwrap_err();
        assert!(err.contains("must not overlap"));
    }
}
