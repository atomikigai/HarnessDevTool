use std::path::{Component, Path, PathBuf};

use harness_core::{
    ingest_data, ingest_office, ingest_pdf, knowledge_search, KnowledgeIngestRequest,
};
use serde_json::{json, Value};

fn str_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("missing or non-string arg: {key}"))
}

pub fn pdf_ingest(
    harness_home: &std::path::Path,
    profile: &str,
    args: &Value,
) -> Result<Value, String> {
    let source_path = resolve_data_source_path(str_arg(args, "source_path")?)?;
    let title = args
        .get("title")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let result = ingest_pdf(
        harness_home,
        profile,
        KnowledgeIngestRequest { source_path, title },
    )
    .map_err(|e| e.to_string())?;
    Ok(json!(result))
}

pub fn office_ingest(
    harness_home: &std::path::Path,
    profile: &str,
    args: &Value,
) -> Result<Value, String> {
    let source_path = resolve_data_source_path(str_arg(args, "source_path")?)?;
    let title = args
        .get("title")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let result = ingest_office(
        harness_home,
        profile,
        KnowledgeIngestRequest { source_path, title },
    )
    .map_err(|e| e.to_string())?;
    Ok(json!(result))
}

pub fn data_ingest(
    harness_home: &std::path::Path,
    profile: &str,
    args: &Value,
) -> Result<Value, String> {
    let source_path = resolve_data_source_path(str_arg(args, "source_path")?)?;
    let title = args
        .get("title")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let result = ingest_data(
        harness_home,
        profile,
        KnowledgeIngestRequest { source_path, title },
    )
    .map_err(|e| e.to_string())?;
    Ok(json!(result))
}

pub fn search(
    harness_home: &std::path::Path,
    profile: &str,
    args: &Value,
) -> Result<Value, String> {
    let query = str_arg(args, "query")?;
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|value| value as usize);
    let hits = knowledge_search(harness_home, profile, query, limit).map_err(|e| e.to_string())?;
    Ok(json!({ "query": query, "hits": hits }))
}

fn resolve_data_source_path(raw: &str) -> Result<PathBuf, String> {
    let root = data_root()?;
    resolve_data_source_path_in_root(raw, &root)
}

fn resolve_data_source_path_in_root(raw: &str, root: &Path) -> Result<PathBuf, String> {
    if raw.is_empty() {
        return Err("source_path is required".into());
    }
    let root = root
        .canonicalize()
        .map_err(|e| format!("resolving data root failed: {e}"))?;
    if !root.is_dir() {
        return Err(format!("data root is not a directory: {}", root.display()));
    }
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        reject_parent_dir(&path, "source_path")?;
    }
    let resolved = if path.is_absolute() {
        path.clone()
    } else {
        root.join(&path)
    };
    let canonical = resolved
        .canonicalize()
        .map_err(|_| format!("source_path not found: {}", path.display()))?;
    ensure_under_root(&canonical, &root, "source_path")?;
    Ok(canonical)
}

fn data_root() -> Result<PathBuf, String> {
    let root = match std::env::var("HARNESS_DATA_ROOT") {
        Ok(raw) if !raw.trim().is_empty() => PathBuf::from(raw.trim()),
        _ => std::env::current_dir().map_err(|e| format!("resolving current dir failed: {e}"))?,
    };
    let canonical = root
        .canonicalize()
        .map_err(|e| format!("resolving data root failed: {e}"))?;
    if !canonical.is_dir() {
        return Err(format!(
            "data root is not a directory: {}",
            canonical.display()
        ));
    }
    Ok(canonical)
}

fn reject_parent_dir(path: &Path, field: &str) -> Result<(), String> {
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(format!(
            "{field} must not contain parent directory traversal"
        ));
    }
    Ok(())
}

fn ensure_under_root(path: &Path, root: &Path, field: &str) -> Result<(), String> {
    if !path.starts_with(root) {
        return Err(format!(
            "{field} escapes the configured data root ({}); set HARNESS_DATA_ROOT to allow a broader base",
            root.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_ingest_resolves_relative_source_under_data_root() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("input.csv"), "id\n1\n").unwrap();

        let resolved = resolve_data_source_path_in_root("input.csv", root.path()).unwrap();

        assert_eq!(
            resolved,
            root.path().join("input.csv").canonicalize().unwrap()
        );
    }

    #[test]
    fn data_ingest_rejects_parent_traversal() {
        let root = tempfile::tempdir().unwrap();

        let err = resolve_data_source_path_in_root("../input.csv", root.path()).unwrap_err();

        assert!(err.contains("parent directory traversal"));
    }

    #[test]
    fn pdf_ingest_rejects_absolute_source_outside_data_root() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let outside_pdf = outside.path().join("outside.pdf");
        std::fs::write(&outside_pdf, b"%PDF-1.4\n").unwrap();

        let err = resolve_data_source_path_in_root(outside_pdf.to_str().unwrap(), root.path())
            .unwrap_err();

        assert!(err.contains("data root"));
        assert!(err.contains(&root.path().canonicalize().unwrap().display().to_string()));
        assert!(err.contains("HARNESS_DATA_ROOT"));
    }

    #[cfg(unix)]
    #[test]
    fn data_ingest_rejects_symlink_escape_from_data_root() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let outside_file = outside.path().join("outside.csv");
        std::fs::write(&outside_file, "id\n1\n").unwrap();
        std::os::unix::fs::symlink(&outside_file, root.path().join("link.csv")).unwrap();

        let err = resolve_data_source_path_in_root("link.csv", root.path()).unwrap_err();

        assert!(err.contains("data root"));
    }
}
