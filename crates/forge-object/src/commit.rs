//! Commit objects (§5.4).
//!
//! A commit references the full directory tree and its parent commit(s). The
//! author and committer identity strings are for display/interoperability only:
//! **authorship is defined by the wallet signature over the commit id**, not by
//! the string (see [`crate::attestation`]).
//!
//! Serialization is exactly:
//!
//! ```text
//! tree <hex>
//! parent <hex>        (zero or more, in first-parent order)
//! author <identity> <unix-ts> <tz>
//! committer <identity> <unix-ts> <tz>
//!
//! <message bytes>
//! ```

use crate::error::ObjectError;
use crate::hash::{HashAlgorithm, Oid};
use crate::object::{self, ObjectType};

/// A Git identity plus timestamp and numeric timezone offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// Display name (ASCII, no angle brackets or control characters).
    pub name: String,
    /// Email address (no angle brackets or control characters).
    pub email: String,
    /// Seconds since the Unix epoch.
    pub timestamp: i64,
    /// Timezone offset in minutes from UTC (e.g. `0`, `-300`).
    pub tz_offset_minutes: i32,
}

impl Identity {
    /// Builds and validates an identity.
    ///
    /// # Errors
    /// Returns [`ObjectError::InvalidIdentity`] when a field contains `<`, `>`,
    /// NUL, or a line break, or when the timezone offset is out of range.
    pub fn new(
        name: impl Into<String>,
        email: impl Into<String>,
        timestamp: i64,
        tz_offset_minutes: i32,
    ) -> Result<Self, ObjectError> {
        let name = name.into();
        let email = email.into();
        for (label, value) in [("name", &name), ("email", &email)] {
            if value.contains(['<', '>', '\0', '\n', '\r']) {
                return Err(ObjectError::InvalidIdentity(format!(
                    "{label} contains a reserved character"
                )));
            }
        }
        if !(-24 * 60..=24 * 60).contains(&tz_offset_minutes) {
            return Err(ObjectError::InvalidIdentity(
                "timezone offset out of range".into(),
            ));
        }
        Ok(Self {
            name,
            email,
            timestamp,
            tz_offset_minutes,
        })
    }

    fn format(&self) -> String {
        format!(
            "{} <{}> {} {}",
            self.name,
            self.email,
            self.timestamp,
            format_tz_offset(self.tz_offset_minutes)
        )
    }
}

/// Formats a timezone offset as Git does: `+HHMM` / `-HHMM`.
pub fn format_tz_offset(minutes: i32) -> String {
    let sign = if minutes < 0 { '-' } else { '+' };
    let abs = minutes.abs();
    format!("{}{:02}{:02}", sign, abs / 60, abs % 60)
}

/// A commit object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    algorithm: HashAlgorithm,
    /// Top-level tree oid.
    pub tree: Oid,
    /// Parent commit oids in first-parent order.
    pub parents: Vec<Oid>,
    /// Author identity and timestamp.
    pub author: Identity,
    /// Committer identity and timestamp.
    pub committer: Identity,
    /// Raw message bytes.
    pub message: Vec<u8>,
}

impl Commit {
    /// Builds a commit, validating that the tree and all parents use
    /// `algorithm`.
    ///
    /// # Errors
    /// Returns [`ObjectError::AlgorithmMismatch`] for cross-algorithm oids.
    pub fn new(
        algorithm: HashAlgorithm,
        tree: Oid,
        parents: Vec<Oid>,
        author: Identity,
        committer: Identity,
        message: impl Into<Vec<u8>>,
    ) -> Result<Self, ObjectError> {
        tree.ensure_algorithm(algorithm)?;
        for parent in &parents {
            parent.ensure_algorithm(algorithm)?;
        }
        Ok(Self {
            algorithm,
            tree,
            parents,
            author,
            committer,
            message: message.into(),
        })
    }

    /// The hash algorithm of this commit.
    pub const fn algorithm(&self) -> HashAlgorithm {
        self.algorithm
    }

    /// Serializes the commit payload (without the Git framing header).
    pub fn payload(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"tree ");
        out.extend_from_slice(self.tree.to_hex().as_bytes());
        out.push(b'\n');
        for parent in &self.parents {
            out.extend_from_slice(b"parent ");
            out.extend_from_slice(parent.to_hex().as_bytes());
            out.push(b'\n');
        }
        out.extend_from_slice(b"author ");
        out.extend_from_slice(self.author.format().as_bytes());
        out.push(b'\n');
        out.extend_from_slice(b"committer ");
        out.extend_from_slice(self.committer.format().as_bytes());
        out.push(b'\n');
        out.push(b'\n');
        out.extend_from_slice(&self.message);
        out
    }

    /// The commit object id.
    pub fn oid(&self) -> Oid {
        object::oid(ObjectType::Commit, &self.payload(), self.algorithm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tz_formatting() {
        assert_eq!(format_tz_offset(0), "+0000");
        assert_eq!(format_tz_offset(-300), "-0500");
        assert_eq!(format_tz_offset(330), "+0530");
    }

    #[test]
    fn payload_shape() {
        let identity = Identity::new("Alice", "alice@example.com", 1_700_000_000, 0).unwrap();
        let commit = Commit::new(
            HashAlgorithm::Sha256,
            Oid::new(HashAlgorithm::Sha256, vec![1u8; 32]).unwrap(),
            vec![],
            identity.clone(),
            identity,
            b"Initial commit\n".to_vec(),
        )
        .unwrap();
        let payload = String::from_utf8(commit.payload()).unwrap();
        assert!(payload.starts_with("tree 010101"));
        assert!(payload.contains("\nauthor Alice <alice@example.com> 1700000000 +0000\n"));
        assert!(payload.ends_with("\n\nInitial commit\n"));
        assert!(!payload.contains("parent "));
    }

    #[test]
    fn reserved_identity_characters_rejected() {
        assert!(Identity::new("Bad\nName", "e@x", 0, 0).is_err());
        assert!(Identity::new("Bad<Name", "e@x", 0, 0).is_err());
    }

    #[test]
    fn cross_algorithm_tree_rejected() {
        let identity = Identity::new("A", "a@x", 0, 0).unwrap();
        let sha1_tree = Oid::new(HashAlgorithm::Sha1, vec![0u8; 20]).unwrap();
        assert!(Commit::new(
            HashAlgorithm::Sha256,
            sha1_tree,
            vec![],
            identity.clone(),
            identity,
            Vec::new(),
        )
        .is_err());
    }
}
