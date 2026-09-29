//! Path safety and Unicode normalization for tree entry names (§5.3, §11 #14).
//!
//! Forge enforces a safe subset of Git's permissive name space: names must be
//! valid UTF-8, normalized to NFC, non-empty, and must not be `.`, `..`, contain
//! `/` or NUL, or be absolute. This is a documented deviation from Git, which
//! permits unusual names, because Forge names are anchored onchain.

use unicode_normalization::UnicodeNormalization;

use crate::error::ObjectError;

/// Returns the NFC normalization of `value`.
pub fn normalize_nfc(value: &str) -> String {
    value.nfc().collect()
}

/// Validates an already-normalized name against the Forge-safe subset.
///
/// # Errors
/// Returns [`ObjectError::InvalidName`] on any violation.
pub fn validate_name(name: &str) -> Result<(), ObjectError> {
    if name.is_empty() {
        return Err(ObjectError::InvalidName("empty name".into()));
    }
    if name == "." || name == ".." {
        return Err(ObjectError::InvalidName(format!("reserved name: {name}")));
    }
    if name.contains('/') {
        return Err(ObjectError::InvalidName(format!(
            "name contains '/': {name}"
        )));
    }
    if name.contains('\0') {
        return Err(ObjectError::InvalidName("name contains NUL".into()));
    }
    if name.starts_with('/') || name.starts_with('\\') {
        return Err(ObjectError::InvalidName(format!("absolute name: {name}")));
    }
    Ok(())
}

/// Normalizes `name` to NFC and validates it.
///
/// # Errors
/// Returns [`ObjectError::InvalidName`] if the bytes are not valid UTF-8 or the
/// normalized name violates the Forge-safe subset.
pub fn sanitize_name(name: &[u8]) -> Result<Vec<u8>, ObjectError> {
    let text = std::str::from_utf8(name)
        .map_err(|_| ObjectError::InvalidName("name is not valid UTF-8".into()))?;
    let normalized = normalize_nfc(text);
    validate_name(&normalized)?;
    Ok(normalized.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal_and_separators() {
        for bad in [b"".as_slice(), b".", b"..", b"a/b", b"/abs", b"nul\0"] {
            assert!(sanitize_name(bad).is_err(), "should reject {bad:?}");
        }
    }

    #[test]
    fn accepts_plain_names() {
        assert_eq!(sanitize_name(b"README.md").unwrap(), b"README.md");
        assert_eq!(sanitize_name(b"src").unwrap(), b"src");
    }

    #[test]
    fn normalizes_to_nfc() {
        // "e" + combining acute accent -> precomposed "é" (U+00E9).
        let decomposed = "e\u{0301}.rs";
        let normalized = sanitize_name(decomposed.as_bytes()).unwrap();
        assert_eq!(normalized, "é.rs".as_bytes());
    }
}
