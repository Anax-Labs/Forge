//! Tree objects: deterministic directory listings (§5.3).
//!
//! Entries are serialized as `mode SP name NUL oid` and sorted by a key that is
//! the name with a trailing `/` appended for subtrees. This reproduces Git's
//! ordering exactly, including the subtle cases:
//!
//! - `foo` (file) sorts before `foo/` (tree), because `\0` < `/`.
//! - `foo.txt` sorts before `foo/`, because `.` (0x2E) < `/` (0x2F).
//! - `foo/` sorts before `foo0`, because `/` (0x2F) < `0` (0x30).

use std::cmp::Ordering;

use crate::error::ObjectError;
use crate::hash::{HashAlgorithm, Oid};
use crate::object::{self, ObjectType};
use crate::path;

/// A Git tree entry mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntryMode {
    /// `100644` — regular non-executable file.
    Regular,
    /// `100755` — executable file.
    Executable,
    /// `120000` — symlink; the target is blob content and is never resolved.
    Symlink,
    /// `40000` — subtree (directory).
    Tree,
    /// `160000` — gitlink/submodule commit reference (no URL).
    Gitlink,
}

impl EntryMode {
    /// The canonical ASCII mode string as written by Git.
    pub const fn as_ascii(self) -> &'static str {
        match self {
            EntryMode::Regular => "100644",
            EntryMode::Executable => "100755",
            EntryMode::Symlink => "120000",
            EntryMode::Tree => "40000",
            EntryMode::Gitlink => "160000",
        }
    }

    /// Whether this entry is a subtree (affects sort order and kind).
    pub const fn is_tree(self) -> bool {
        matches!(self, EntryMode::Tree)
    }

    /// Parses a mode string, accepting the canonical `40000` and the legacy
    /// zero-padded `040000` for subtrees.
    ///
    /// # Errors
    /// Returns [`ObjectError::InvalidMode`] for unsupported modes.
    pub fn from_ascii(value: &str) -> Result<Self, ObjectError> {
        match value {
            "100644" => Ok(EntryMode::Regular),
            "100755" => Ok(EntryMode::Executable),
            "120000" => Ok(EntryMode::Symlink),
            "40000" | "040000" => Ok(EntryMode::Tree),
            "160000" => Ok(EntryMode::Gitlink),
            other => Err(ObjectError::InvalidMode(other.to_string())),
        }
    }
}

/// A single tree entry after normalization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    /// Entry mode.
    pub mode: EntryMode,
    /// NFC-normalized entry name.
    pub name: Vec<u8>,
    /// Object id of the referenced blob, tree, or commit.
    pub oid: Oid,
}

/// Builds a canonical tree payload from entries.
#[derive(Debug, Clone)]
pub struct TreeBuilder {
    algorithm: HashAlgorithm,
    entries: Vec<TreeEntry>,
}

impl TreeBuilder {
    /// Creates an empty builder for `algorithm`.
    pub fn new(algorithm: HashAlgorithm) -> Self {
        Self {
            algorithm,
            entries: Vec::new(),
        }
    }

    /// Adds an entry. The name is validated and normalized to NFC; the oid must
    /// use the builder's algorithm.
    ///
    /// # Errors
    /// Returns [`ObjectError::InvalidName`] for unsafe names,
    /// [`ObjectError::DuplicateEntry`] for repeated names, and
    /// [`ObjectError::AlgorithmMismatch`] for cross-algorithm oids.
    pub fn add(
        &mut self,
        mode: EntryMode,
        name: &[u8],
        oid: Oid,
    ) -> Result<&mut Self, ObjectError> {
        let name = path::sanitize_name(name)?;
        if self.entries.iter().any(|entry| entry.name == name) {
            return Err(ObjectError::DuplicateEntry(
                String::from_utf8_lossy(&name).into_owned(),
            ));
        }
        oid.ensure_algorithm(self.algorithm)?;
        self.entries.push(TreeEntry { mode, name, oid });
        Ok(self)
    }

    /// Number of entries added so far.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no entries have been added.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Sorts entries into canonical order and returns the tree payload.
    pub fn build_payload(mut self) -> Vec<u8> {
        serialize_entries(&mut self.entries)
    }

    /// Returns the tree object id.
    pub fn build_oid(self) -> Oid {
        let algorithm = self.algorithm;
        let payload = self.build_payload();
        object::oid(ObjectType::Tree, &payload, algorithm)
    }
}

/// Sorts `entries` in place and serializes the canonical tree payload.
pub fn serialize_entries(entries: &mut [TreeEntry]) -> Vec<u8> {
    entries.sort_by(cmp_entries);
    let mut out = Vec::new();
    for entry in entries.iter() {
        out.extend_from_slice(entry.mode.as_ascii().as_bytes());
        out.push(b' ');
        out.extend_from_slice(&entry.name);
        out.push(0);
        out.extend_from_slice(entry.oid.as_bytes());
    }
    out
}

/// Parses a tree payload into entries in stored order.
///
/// # Errors
/// Returns [`ObjectError::InvalidMode`], [`ObjectError::InvalidName`], or
/// [`ObjectError::Cbor`]-style framing errors surfaced as
/// [`ObjectError::InvalidMode`] via [`ObjectError::InvalidName`].
pub fn parse_tree(payload: &[u8], algorithm: HashAlgorithm) -> Result<Vec<TreeEntry>, ObjectError> {
    let digest_len = algorithm.digest_len();
    let mut entries = Vec::new();
    let mut pos = 0usize;
    while pos < payload.len() {
        // mode ascii up to space
        let space = payload[pos..]
            .iter()
            .position(|b| *b == b' ')
            .ok_or_else(|| ObjectError::InvalidMode("missing space after mode".into()))?;
        let mode_str = std::str::from_utf8(&payload[pos..pos + space])
            .map_err(|_| ObjectError::InvalidMode("non-utf8 mode".into()))?;
        let mode = EntryMode::from_ascii(mode_str)?;
        pos += space + 1;

        // name up to NUL
        let nul = payload[pos..]
            .iter()
            .position(|b| *b == 0)
            .ok_or_else(|| ObjectError::InvalidName("missing NUL after name".into()))?;
        let name_bytes = payload[pos..pos + nul].to_vec();
        let name_str = std::str::from_utf8(&name_bytes)
            .map_err(|_| ObjectError::InvalidName("non-utf8 name".into()))?;
        path::validate_name(name_str)?;
        pos += nul + 1;

        // digest
        if pos + digest_len > payload.len() {
            return Err(ObjectError::InvalidOidLength(
                payload.len() - pos,
                algorithm.tag(),
            ));
        }
        let oid = Oid::new(algorithm, payload[pos..pos + digest_len].to_vec())?;
        pos += digest_len;

        entries.push(TreeEntry {
            mode,
            name: name_bytes,
            oid,
        });
    }
    Ok(entries)
}

/// Whether `entries` are already in canonical Forge/Git order.
pub fn is_canonically_sorted(entries: &[TreeEntry]) -> bool {
    entries
        .windows(2)
        .all(|w| cmp_entries(&w[0], &w[1]) == Ordering::Less)
}

fn cmp_entries(a: &TreeEntry, b: &TreeEntry) -> Ordering {
    let a_key = a
        .name
        .iter()
        .copied()
        .chain(a.mode.is_tree().then_some(b'/'));
    let b_key = b
        .name
        .iter()
        .copied()
        .chain(b.mode.is_tree().then_some(b'/'));
    a_key.cmp(b_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blob(content: &[u8]) -> Oid {
        crate::blob::blob_oid(content, HashAlgorithm::Sha256)
    }

    #[test]
    fn ordering_matches_git_edge_cases() {
        let b = blob(b"x");
        let mut entries = [
            TreeEntry {
                mode: EntryMode::Tree,
                name: b"foo".to_vec(),
                oid: b.clone(),
            },
            TreeEntry {
                mode: EntryMode::Regular,
                name: b"foo.txt".to_vec(),
                oid: b.clone(),
            },
            TreeEntry {
                mode: EntryMode::Regular,
                name: b"foo".to_vec(),
                oid: b.clone(),
            },
        ];
        entries.sort_by(cmp_entries);
        let names: Vec<&[u8]> = entries.iter().map(|e| e.name.as_slice()).collect();
        // foo (file) < foo.txt (file) < foo/ (tree)
        assert_eq!(names, vec![b"foo".as_slice(), b"foo.txt", b"foo"]);
        // The tree `foo/` must be last.
        assert_eq!(entries.last().unwrap().mode, EntryMode::Tree);
    }

    #[test]
    fn duplicate_names_rejected() {
        let mut builder = TreeBuilder::new(HashAlgorithm::Sha256);
        builder.add(EntryMode::Regular, b"a", blob(b"1")).unwrap();
        assert!(builder.add(EntryMode::Regular, b"a", blob(b"2")).is_err());
    }

    #[test]
    fn unsafe_names_rejected() {
        let mut builder = TreeBuilder::new(HashAlgorithm::Sha256);
        assert!(builder
            .add(EntryMode::Regular, b"../x", blob(b"1"))
            .is_err());
    }

    #[test]
    fn roundtrip_parse() {
        let mut builder = TreeBuilder::new(HashAlgorithm::Sha256);
        builder
            .add(EntryMode::Regular, b"README.md", blob(b"hello\n"))
            .unwrap()
            .add(EntryMode::Executable, b"run.sh", blob(b"#!/bin/sh\n"))
            .unwrap();
        let payload = builder.clone().build_payload();
        let parsed = parse_tree(&payload, HashAlgorithm::Sha256).unwrap();
        assert!(is_canonically_sorted(&parsed));
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].name, b"README.md");
        assert_eq!(parsed[0].mode, EntryMode::Regular);
    }
}
