use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Decision;

#[cfg(feature = "ts-export")]
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct Rule {
    pub tool: String,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub args_match: BTreeMap<String, String>,
    pub decision: Decision,
}

impl Rule {
    pub fn matches(&self, tool: &str, args: &serde_json::Value, role: Option<&str>) -> bool {
        if self.tool != tool {
            return false;
        }
        if let Some(rule_role) = self.role.as_deref() {
            let Some(role) = role else {
                return false;
            };
            if !rule_role.eq_ignore_ascii_case(role) {
                return false;
            }
        }
        self.args_match.iter().all(|(key, pattern)| {
            args.get(key)
                .and_then(|value| value.as_str())
                .is_some_and(|value| glob_match(pattern, value))
        })
    }
}

fn glob_match(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return pattern == value;
    }

    let starts_with_star = pattern.starts_with('*');
    let ends_with_star = pattern.ends_with('*');
    let parts: Vec<&str> = pattern.split('*').filter(|part| !part.is_empty()).collect();
    if parts.is_empty() {
        return true;
    }

    let mut rest = value;
    if !starts_with_star {
        let first = parts[0];
        if !rest.starts_with(first) {
            return false;
        }
        rest = &rest[first.len()..];
    }

    let start_idx = usize::from(!starts_with_star);
    let end_idx = if ends_with_star {
        parts.len()
    } else {
        parts.len().saturating_sub(1)
    };
    for part in &parts[start_idx..end_idx] {
        let Some(idx) = rest.find(part) else {
            return false;
        };
        rest = &rest[idx + part.len()..];
    }

    ends_with_star || rest.ends_with(parts[parts.len() - 1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_variants() {
        assert!(glob_match("*foo", "barfoo"));
        assert!(glob_match("foo*", "foobar"));
        assert!(glob_match("*foo*", "barfoobaz"));
        assert!(glob_match("foo*bar", "foobazbar"));
        assert!(!glob_match("foo*bar", "barfoobaz"));
    }
}
