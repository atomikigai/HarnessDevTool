//! Knowledge ingestion helpers for turning large technical documents into
//! compact, indexable Markdown shards under a profile.

use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use pdf_oxide::converters::ConversionOptions;
use pdf_oxide::PdfDocument;
use serde::{Deserialize, Serialize};

use crate::Error;

const MAX_SHARD_CHARS: usize = 7_500;
const MIN_SHARD_CHARS: usize = 2_000;

#[derive(Debug, Clone, Deserialize)]
pub struct KnowledgeIngestRequest {
    pub source_path: PathBuf,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KnowledgeIngestResult {
    pub title: String,
    pub slug: String,
    pub output_dir: PathBuf,
    pub index_path: PathBuf,
    pub shard_count: usize,
    pub shards: Vec<KnowledgeShard>,
    pub source_path: PathBuf,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KnowledgeShard {
    pub id: String,
    pub title: String,
    pub path: PathBuf,
    pub pages: Vec<usize>,
    pub bytes: u64,
}

#[derive(Debug, Clone)]
struct TextShard {
    title: String,
    pages: Vec<usize>,
    content: String,
}

pub fn ingest_pdf(
    harness_home: &Path,
    profile: &str,
    req: KnowledgeIngestRequest,
) -> Result<KnowledgeIngestResult, Error> {
    validate_pdf_path(&req.source_path)?;
    let text = extract_pdf_text(&req.source_path)?;
    ingest_text(harness_home, profile, req.source_path, req.title, &text)
}

pub fn ingest_text(
    harness_home: &Path,
    profile: &str,
    source_path: PathBuf,
    title: Option<String>,
    text: &str,
) -> Result<KnowledgeIngestResult, Error> {
    let title = title
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| title_from_path(&source_path));
    let slug = sanitize_segment(&title);
    let root = harness_home
        .join("profiles")
        .join(sanitize_segment(profile))
        .join("knowledge")
        .join("pdf");
    std::fs::create_dir_all(&root)?;
    let output_dir = unique_dir(&root, &slug)?;
    let shards_dir = output_dir.join("shards");
    std::fs::create_dir_all(&shards_dir)?;

    let generated_at = Utc::now();
    let normalized = normalize_text(text);
    if normalized.trim().is_empty() {
        return Err(Error::Validation(
            "pdf extraction produced no readable text".to_string(),
        ));
    }
    let text_shards = shard_text(&normalized);
    let mut shards = Vec::with_capacity(text_shards.len());
    for (idx, shard) in text_shards.iter().enumerate() {
        let id = format!("{:03}", idx + 1);
        let filename = format!("{id}-{}.md", sanitize_segment(&shard.title));
        let path = shards_dir.join(filename);
        let content = render_shard(&title, &id, shard);
        write_file(&path, &content)?;
        shards.push(KnowledgeShard {
            id,
            title: shard.title.clone(),
            path: path.clone(),
            pages: shard.pages.clone(),
            bytes: content.len() as u64,
        });
    }

    let index_path = output_dir.join("index.md");
    write_file(
        &index_path,
        &render_index(&title, &source_path, generated_at, &shards),
    )?;
    write_file(
        &output_dir.join("metadata.json"),
        &serde_json::to_string_pretty(&serde_json::json!({
            "title": title,
            "slug": slug,
            "source_path": source_path,
            "generated_at": generated_at,
            "shard_count": shards.len(),
            "shards": shards,
        }))
        .map_err(|e| Error::Other(e.into()))?,
    )?;

    Ok(KnowledgeIngestResult {
        title,
        slug,
        output_dir,
        index_path,
        shard_count: shards.len(),
        shards,
        source_path,
        generated_at,
    })
}

fn validate_pdf_path(path: &Path) -> Result<(), Error> {
    if path.extension().and_then(OsStr::to_str) != Some("pdf") {
        return Err(Error::Validation(
            "source_path must end with .pdf".to_string(),
        ));
    }
    if !path.is_file() {
        return Err(Error::NotFound(format!("pdf {}", path.display())));
    }
    Ok(())
}

fn extract_pdf_text(path: &Path) -> Result<String, Error> {
    let doc = PdfDocument::open(path)
        .map_err(|e| Error::Validation(format!("failed to open PDF: {e}")))?;
    let page_count = doc
        .page_count()
        .map_err(|e| Error::Validation(format!("failed to read PDF page count: {e}")))?;
    let options = ConversionOptions {
        detect_headings: true,
        ..Default::default()
    };
    let mut pages = Vec::with_capacity(page_count);
    for page in 0..page_count {
        let markdown = doc.to_markdown(page, &options).map_err(|e| {
            Error::Validation(format!("failed to extract PDF page {}: {e}", page + 1))
        })?;
        pages.push(markdown);
    }
    Ok(pages.join("\u{c}"))
}

fn normalize_text(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
        .split("\n\n\n")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn shard_text(text: &str) -> Vec<TextShard> {
    let pages: Vec<&str> = text.split('\u{c}').collect();
    let mut shards = Vec::new();
    let mut current = String::new();
    let mut current_pages = Vec::new();
    let mut current_title = String::new();

    for (page_idx, page) in pages.iter().enumerate() {
        let page_number = page_idx + 1;
        for block in page.split("\n\n").map(str::trim).filter(|b| !b.is_empty()) {
            if current_title.is_empty() {
                current_title =
                    infer_title(block).unwrap_or_else(|| format!("Pages {page_number}"));
            }
            if current.len() >= MIN_SHARD_CHARS && current.len() + block.len() + 2 > MAX_SHARD_CHARS
            {
                shards.push(TextShard {
                    title: current_title.clone(),
                    pages: current_pages.clone(),
                    content: current.trim().to_string(),
                });
                current.clear();
                current_pages.clear();
                current_title =
                    infer_title(block).unwrap_or_else(|| format!("Pages {page_number}"));
            }
            if !current_pages.contains(&page_number) {
                current_pages.push(page_number);
            }
            if !current.is_empty() {
                current.push_str("\n\n");
            }
            current.push_str(block);
        }
    }

    if !current.trim().is_empty() {
        shards.push(TextShard {
            title: current_title,
            pages: current_pages,
            content: current.trim().to_string(),
        });
    }
    shards
}

fn infer_title(block: &str) -> Option<String> {
    block.lines().map(str::trim).find_map(|line| {
        let line = line.trim_matches(|c: char| c == '#' || c == '-' || c.is_whitespace());
        if line.len() < 4 || line.len() > 90 {
            return None;
        }
        let alpha = line.chars().filter(|c| c.is_alphabetic()).count();
        if alpha < 3 {
            return None;
        }
        let looks_numbered = line
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false);
        let uppercase = line.chars().filter(|c| c.is_uppercase()).count();
        if looks_numbered || uppercase * 2 >= alpha {
            Some(line.to_string())
        } else {
            None
        }
    })
}

fn render_index(
    title: &str,
    source_path: &Path,
    generated_at: DateTime<Utc>,
    shards: &[KnowledgeShard],
) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {title}\n\n"));
    out.push_str("## Uso para agentes\n\n");
    out.push_str(
        "Leer este indice primero y abrir solo los shards relevantes. Cada shard es corto, estable e independiente.\n\n",
    );
    out.push_str("## Metadata\n\n");
    out.push_str(&format!("- Source: `{}`\n", source_path.display()));
    out.push_str(&format!("- Generated: `{}`\n", generated_at.to_rfc3339()));
    out.push_str(&format!("- Shards: `{}`\n\n", shards.len()));
    out.push_str("## Shards\n\n");
    for shard in shards {
        let rel = shard
            .path
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or_default();
        let pages = format_pages(&shard.pages);
        out.push_str(&format!(
            "- [{} - {}](shards/{}) - pages {}\n",
            shard.id, shard.title, rel, pages
        ));
    }
    out
}

fn render_shard(doc_title: &str, id: &str, shard: &TextShard) -> String {
    format!(
        "# {} - {}\n\n- Source document: `{}`\n- Shard: `{}`\n- Source pages: `{}`\n\n## Content\n\n{}\n",
        id,
        shard.title,
        doc_title,
        id,
        format_pages(&shard.pages),
        shard.content
    )
}

fn format_pages(pages: &[usize]) -> String {
    if pages.is_empty() {
        return "unknown".to_string();
    }
    pages
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn title_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or("document")
        .replace(['_', '-'], " ")
        .trim()
        .to_string()
}

fn sanitize_segment(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else if (ch.is_whitespace() || ch == '-' || ch == '_' || ch == '.') && !out.ends_with('-')
        {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "document".to_string()
    } else {
        out
    }
}

fn unique_dir(root: &Path, slug: &str) -> Result<PathBuf, Error> {
    for i in 0..1000 {
        let name = if i == 0 {
            slug.to_string()
        } else {
            format!("{slug}-{i}")
        };
        let path = root.join(name);
        match std::fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(Error::Validation(format!(
        "could not create unique knowledge directory for {slug}"
    )))
}

fn write_file(path: &Path, content: &str) -> Result<(), Error> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::Validation(format!("invalid path {}", path.display())))?;
    std::fs::create_dir_all(parent)?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(content.as_bytes())?;
    tmp.flush()?;
    tmp.persist(path).map_err(|e| Error::Io(e.error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingest_text_writes_index_and_small_shards() {
        let dir = tempfile::tempdir().unwrap();
        let source = PathBuf::from("/tmp/INST.VT.002-V-03.pdf");
        let repeated =
            "1. OBJETIVO\n\nConfigurar el modulo VT con pasos tecnicos y parametros.\n\n";
        let text = format!(
            "{}\u{c}2. PROCEDIMIENTO\n\n{}{}{}",
            repeated,
            repeated.repeat(180),
            "\u{c}",
            "3. VALIDACION\n\nVerificar estados, alarmas y registros finales."
        );

        let result = ingest_text(
            dir.path(),
            "default",
            source.clone(),
            Some("INST VT 002 V 03".to_string()),
            &text,
        )
        .unwrap();

        assert_eq!(result.title, "INST VT 002 V 03");
        assert!(result
            .output_dir
            .ends_with("profiles/default/knowledge/pdf/inst-vt-002-v-03"));
        assert!(result.index_path.is_file());
        assert!(result.shard_count >= 2);
        for shard in &result.shards {
            assert!(shard.path.is_file());
            let content = std::fs::read_to_string(&shard.path).unwrap();
            assert!(content.contains("## Content"));
            assert!(content.len() <= MAX_SHARD_CHARS + 800);
        }
        let index = std::fs::read_to_string(&result.index_path).unwrap();
        assert!(index.contains("## Uso para agentes"));
        assert!(index.contains("shards/001-"));
    }

    #[test]
    fn repeated_ingest_uses_unique_directory() {
        let dir = tempfile::tempdir().unwrap();
        let source = PathBuf::from("/tmp/doc.pdf");
        let text = "MANUAL TECNICO\n\nContenido suficiente para un shard.";

        let first = ingest_text(dir.path(), "default", source.clone(), None, text).unwrap();
        let second = ingest_text(dir.path(), "default", source, None, text).unwrap();

        assert_ne!(first.output_dir, second.output_dir);
        assert!(second
            .output_dir
            .ends_with("profiles/default/knowledge/pdf/doc-1"));
    }
}
