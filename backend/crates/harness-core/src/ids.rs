//! Validation for ids used as filesystem path segments.

pub fn validate_path_id(kind: &str, value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err(format!("{kind} must not be empty"));
    }
    if !value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(format!(
            "{kind} must contain only ASCII letters, digits, `_` or `-`"
        ));
    }
    Ok(())
}

pub fn validate_thread_id(value: &str) -> Result<(), String> {
    validate_path_id("thread_id", value)
}

pub fn validate_task_id(value: &str) -> Result<(), String> {
    validate_path_id("task_id", value)
}

pub fn validate_profile_id(value: &str) -> Result<(), String> {
    validate_path_id("profile_id", value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_ids_accept_expected_task_thread_profile_shapes() {
        for value in ["default", "thread-123", "T-0001", "abc_DEF-123"] {
            validate_path_id("id", value).unwrap();
        }
    }

    #[test]
    fn path_ids_reject_traversal_and_absolute_shapes() {
        for value in ["", "..", "../x", "/tmp/x", "a/b", "a\\b", "x.y", "bad\0id"] {
            assert!(validate_path_id("id", value).is_err(), "{value:?}");
        }
    }
}
