use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocsBackend {
    Starlight,
    Mdbook,
    Vitepress,
}

impl DocsBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Starlight => "starlight",
            Self::Mdbook => "mdbook",
            Self::Vitepress => "vitepress",
        }
    }
}

impl fmt::Display for DocsBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for DocsBackend {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "starlight" => Ok(Self::Starlight),
            "mdbook" => Ok(Self::Mdbook),
            "vitepress" => Ok(Self::Vitepress),
            other => Err(format!(
                "unknown docs backend `{other}`; expected starlight, mdbook, or vitepress"
            )),
        }
    }
}

pub fn infer_docs_backend(stack: &[String]) -> DocsBackend {
    let mut normalized = stack
        .iter()
        .map(|item| item.to_ascii_lowercase())
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();

    if normalized == ["rust"] {
        DocsBackend::Mdbook
    } else {
        DocsBackend::Starlight
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_docs_backend_prefers_mdbook_for_rust_only() {
        assert_eq!(infer_docs_backend(&["rust".into()]), DocsBackend::Mdbook);
    }

    #[test]
    fn infer_docs_backend_defaults_to_starlight_for_mixed_or_unknown() {
        assert_eq!(
            infer_docs_backend(&["node".into(), "rust".into()]),
            DocsBackend::Starlight
        );
        assert_eq!(infer_docs_backend(&[]), DocsBackend::Starlight);
    }

    #[test]
    fn backend_from_str_rejects_unknown_values() {
        assert_eq!(
            "vitepress".parse::<DocsBackend>(),
            Ok(DocsBackend::Vitepress)
        );
        assert!("vite".parse::<DocsBackend>().is_err());
    }
}
