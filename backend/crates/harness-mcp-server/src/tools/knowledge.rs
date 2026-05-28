use std::path::PathBuf;

use harness_core::{check_pdftotext, ingest_pdf, KnowledgeIngestRequest};
use serde_json::{json, Value};

fn str_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("missing or non-string arg: {key}"))
}

pub fn pdftotext_check() -> Value {
    json!(check_pdftotext())
}

pub fn pdf_ingest(
    harness_home: &std::path::Path,
    profile: &str,
    args: &Value,
) -> Result<Value, String> {
    let source_path = PathBuf::from(str_arg(args, "source_path")?);
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
