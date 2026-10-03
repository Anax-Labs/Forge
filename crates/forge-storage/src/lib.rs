//! Content-addressed storage for Forge: IPFS hot / Arweave cold (§8.3).
//!
//! Git object ids from [`forge_object`] are the protocol addresses. CIDs,
//! Arweave transaction ids, and local paths are **hints** recorded in
//! `.forge/storage-index` and are re-verified on every read by recomputing
//! the Git OID (and, for CAR blocks, the CID).

#![forbid(unsafe_code)]
#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::type_complexity)]

pub mod arweave;
pub mod backend;
pub mod car;
pub mod cid;
pub mod error;
pub mod fs;
pub mod gc;
pub mod graph;
pub mod hint;
pub mod http;
pub mod index;
pub mod ipfs;
pub mod memory;
pub mod multi;
pub mod object;

pub use arweave::{ArweaveBundler, DEFAULT_CHUNK_BYTES};
pub use backend::{fetch_verified, Locator, LocatorBackend, StorageBackend, StorageBackendKind};
pub use car::Car;
pub use cid::Cid;
pub use error::StorageError;
pub use fs::FsBackend;
pub use gc::{report_for_forge_dir, verify_availability, AvailabilityReport, AvailabilityStatus};
pub use graph::{ObjectGraph, ObjectSource};
pub use hint::hint_hash;
pub use index::{cas_path, storage_index_path, StorageIndex};
pub use ipfs::IpfsClient;
pub use memory::MemoryBackend;
pub use multi::MultiPin;
pub use object::GitObject;

use forge_object::hash::Oid;

/// Uploads a CARv1 of `graph` to every pin in `pins` (≥2) and records the
/// locators against every contained oid in `index`.
///
/// # Errors
/// Returns pin, CAR, or verification errors.
pub fn upload_bundle(
    pins: &MultiPin<'_>,
    graph: &ObjectGraph,
    root: &Oid,
    index: &mut StorageIndex,
) -> Result<Vec<Locator>, StorageError> {
    let car = graph.to_car(root)?.encode();
    let locators = pins.put(&car)?;
    for obj in graph.objects() {
        index.record(obj, locators.clone())?;
    }
    Ok(locators)
}

/// Fetches an object by oid using index locators. Every returned byte is
/// re-hashed; a matching CID/TXID is never sufficient on its own.
///
/// # Errors
/// Returns [`StorageError::NotFound`] or [`StorageError::OidMismatch`].
pub fn fetch_object(
    pins: &MultiPin<'_>,
    index: &StorageIndex,
    oid: &Oid,
) -> Result<GitObject, StorageError> {
    let locators = index
        .locators_for(oid)
        .ok_or_else(|| StorageError::NotFound(oid.to_tagged_string()))?;
    let bytes = pins.get(locators)?;
    match Car::decode(&bytes) {
        Ok(car) => car.get(oid),
        Err(StorageError::InvalidCar(_)) => GitObject::verify_framed(&bytes, oid),
        Err(err) => Err(err),
    }
}
