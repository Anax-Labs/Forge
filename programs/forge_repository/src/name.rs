//! Name validation for repositories, branches, and tags.
//!
//! Names are fixed `[u8; 32]` buffers padded with NUL bytes. The valid form is
//! a non-empty sequence of printable ASCII (`0x21..=0x7e`) followed only by NUL
//! padding. This is intentionally stricter than Git refnames: it keeps seeds
//! unambiguous and prevents control characters / path separators from reaching
//! onchain metadata or clients (§11 #12, §11 #14).
//!
//! Note: this is a *documented deviation* from Git, which permits a wider set
//! of bytes; Forge names are protocol-level identifiers, not filesystem paths.

use crate::errors::ForgeError;
use anchor_lang::prelude::*;

/// Returns the length of the NUL-terminated prefix, or an error if the buffer
/// is not a valid Forge name.
fn checked_prefix(name: &[u8; 32]) -> Result<usize> {
    let end = name.iter().position(|b| *b == 0).unwrap_or(name.len());
    require!(end > 0, ForgeError::InvalidName);
    // Everything after the first NUL must be NUL padding.
    require!(name[end..].iter().all(|b| *b == 0), ForgeError::InvalidName);
    Ok(end)
}

/// Validates a NUL-padded `[u8; 32]` name.
///
/// # Errors
/// Returns [`ForgeError::InvalidName`] if the name is empty, has non-zero
/// padding, or contains bytes outside printable ASCII.
pub fn validate_name(name: &[u8; 32]) -> Result<()> {
    let end = checked_prefix(name)?;
    for byte in &name[..end] {
        require!(*byte >= 0x21 && *byte <= 0x7e, ForgeError::InvalidName);
    }
    Ok(())
}

/// Returns the accepted ASCII prefix of a validated name.
///
/// Callers must validate with [`validate_name`] first; this helper is provided
/// for logging/debugging only and never trusts raw input without validation.
pub fn name_prefix(name: &[u8; 32]) -> &[u8] {
    let end = name.iter().position(|b| *b == 0).unwrap_or(name.len());
    &name[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn padded(s: &str) -> [u8; 32] {
        let mut out = [0u8; 32];
        out[..s.len()].copy_from_slice(s.as_bytes());
        out
    }

    #[test]
    fn accepts_printable_ascii() {
        assert!(validate_name(&padded("main")).is_ok());
        assert!(validate_name(&padded("feature/x-1.0")).is_ok());
    }

    #[test]
    fn rejects_empty_and_non_zero_padding() {
        assert!(validate_name(&[0u8; 32]).is_err());
        let mut bad = padded("main");
        bad[31] = b'!';
        assert!(validate_name(&bad).is_err());
    }

    #[test]
    fn rejects_control_and_space() {
        assert!(validate_name(&padded("bad name")).is_err());
        let mut ctrl = padded("bad");
        ctrl[3] = 0x01;
        assert!(validate_name(&ctrl).is_err());
    }
}
