//! `forge gc --verify-availability` — report unpinned / absent / corrupt objects (§8.3, §13).

use std::fmt::Write as _;
use std::path::Path;

use crate::backend::{fetch_verified, StorageBackend};
use crate::error::StorageError;
use crate::fs::FsBackend;
use crate::index::{cas_path, storage_index_path, StorageIndex};
use forge_object::hash::Oid;

/// Pin-count classification against the ≥2 availability policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AvailabilityStatus {
    /// At least two independent pins returned matching bytes.
    Available,
    /// Exactly one pin still has the object.
    Degraded,
    /// No pin could serve the object.
    Missing,
    /// At least one pin served bytes that failed OID verification.
    Corrupt,
}

/// Per-object availability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectAvailability {
    /// Tagged oid.
    pub oid: String,
    /// `blob` / `tree` / `commit` / `tag`.
    pub object_type: String,
    /// Pins that served verified bytes.
    pub live_pins: usize,
    /// Provider ids that were missing.
    pub missing_providers: Vec<String>,
    /// Summary status.
    pub status: AvailabilityStatus,
}

/// Full report for an index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailabilityReport {
    /// One row per indexed object.
    pub objects: Vec<ObjectAvailability>,
}

impl AvailabilityReport {
    /// True when every object has ≥2 live verified pins and none are corrupt.
    pub fn fully_available(&self) -> bool {
        self.objects
            .iter()
            .all(|row| row.status == AvailabilityStatus::Available)
    }

    /// Objects that are missing, degraded, or corrupt.
    pub fn unbacked(&self) -> impl Iterator<Item = &ObjectAvailability> {
        self.objects
            .iter()
            .filter(|row| !matches!(row.status, AvailabilityStatus::Available))
    }

    /// Human-readable summary for the CLI.
    pub fn render(&self) -> String {
        let mut out = format!("availability: {} objects\n", self.objects.len());
        for row in &self.objects {
            let status = match row.status {
                AvailabilityStatus::Available => "available",
                AvailabilityStatus::Degraded => "degraded",
                AvailabilityStatus::Missing => "missing",
                AvailabilityStatus::Corrupt => "corrupt",
            };
            let _ = writeln!(out, "  {}  {status}  pins={}", row.oid, row.live_pins);
        }
        out
    }
}

fn backend_named<'a>(
    backends: &'a [&dyn StorageBackend],
    provider: &str,
) -> Option<&'a dyn StorageBackend> {
    backends
        .iter()
        .copied()
        .find(|b| b.provider_id() == provider)
}

/// Checks every object in `index` against `backends` (matched by provider id).
///
/// # Errors
/// Returns codec errors; missing pins are recorded, not raised.
pub fn verify_availability(
    index: &StorageIndex,
    backends: &[&dyn StorageBackend],
) -> Result<AvailabilityReport, StorageError> {
    let mut objects = Vec::new();
    for (oid_s, entry) in &index.objects {
        let oid = Oid::parse_tagged(oid_s)?;
        let mut live = 0usize;
        let mut missing = Vec::new();
        let mut corrupt = false;
        for locator in &entry.locators {
            let Some(backend) = backend_named(backends, &locator.provider) else {
                missing.push(locator.provider.clone());
                continue;
            };
            match fetch_verified(backend, locator, &oid) {
                Ok(_) => live += 1,
                Err(StorageError::NotFound(_)) => missing.push(locator.provider.clone()),
                Err(StorageError::OidMismatch { .. } | StorageError::CidMismatch { .. }) => {
                    corrupt = true;
                }
                Err(err) => return Err(err),
            }
        }
        let status = if corrupt {
            AvailabilityStatus::Corrupt
        } else if live >= 2 {
            AvailabilityStatus::Available
        } else if live == 1 {
            AvailabilityStatus::Degraded
        } else {
            AvailabilityStatus::Missing
        };
        objects.push(ObjectAvailability {
            oid: oid_s.clone(),
            object_type: entry.object_type.clone(),
            live_pins: live,
            missing_providers: missing,
            status,
        });
    }
    Ok(AvailabilityReport { objects })
}

/// Loads `{forge_dir}/.forge/storage-index` and checks the local CAS plus any
/// extra backends (IPFS / Arweave) supplied by the caller.
///
/// # Errors
/// Returns I/O errors when the index is missing, and verification errors.
pub fn report_for_forge_dir(
    forge_dir: impl AsRef<Path>,
    extra: &[&dyn StorageBackend],
) -> Result<AvailabilityReport, StorageError> {
    let forge_dir = forge_dir.as_ref();
    let index = StorageIndex::load(storage_index_path(forge_dir))?;
    let cas = FsBackend::open("cas", cas_path(forge_dir))?;
    let mut backends: Vec<&dyn StorageBackend> = Vec::with_capacity(extra.len() + 1);
    backends.push(&cas);
    backends.extend_from_slice(extra);
    verify_availability(&index, &backends)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::IndexEntry;
    use crate::memory::MemoryBackend;
    use crate::object::GitObject;
    use crate::{Locator, LocatorBackend};
    use forge_object::hash::HashAlgorithm;
    use forge_object::object::ObjectType;

    #[test]
    fn reports_missing_and_degraded() {
        let obj = GitObject::from_payload(ObjectType::Blob, b"n".to_vec(), HashAlgorithm::Sha256);
        let a = MemoryBackend::new("a");
        let b = MemoryBackend::new("b");
        let loc_a = a.put(&obj.framed()).unwrap();
        let loc_b = Locator {
            backend: LocatorBackend::Memory,
            provider: "b".into(),
            id: loc_a.id.clone(),
        };
        b.put(&obj.framed()).unwrap();
        let mut index = StorageIndex::new("sha256");
        index
            .record(&obj, vec![loc_a.clone(), loc_b.clone()])
            .unwrap();
        let backends: Vec<&dyn StorageBackend> = vec![&a, &b];
        let report = verify_availability(&index, &backends).unwrap();
        assert!(report.fully_available());
        b.kill();
        let report = verify_availability(&index, &backends).unwrap();
        assert_eq!(report.objects[0].status, AvailabilityStatus::Degraded);
        a.kill();
        let report = verify_availability(&index, &backends).unwrap();
        assert_eq!(report.objects[0].status, AvailabilityStatus::Missing);
        let mut empty = StorageIndex::new("sha256");
        empty.objects.insert(
            obj.oid.to_tagged_string(),
            IndexEntry {
                object_type: "blob".into(),
                locators: vec![],
            },
        );
        let report = verify_availability(&empty, &backends).unwrap();
        assert_eq!(report.unbacked().count(), 1);
    }
}
