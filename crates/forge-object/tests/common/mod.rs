//! Shared golden-vector fixture for `forge-object` integration tests.
//!
//! Both the git cross-compatibility test and the checked-in golden-vector test
//! build the exact same inputs through this module, so a change to any canonical
//! encoding is caught in one place.

#![allow(dead_code)]

use std::collections::BTreeMap;

use forge_object::attestation::{Attestation, ATTESTATION_VERSION};
use forge_object::commit::{Commit, Identity};
use forge_object::error::ObjectError;
use forge_object::hash::{HashAlgorithm, Oid};
use forge_object::tree::{EntryMode, TreeBuilder};
use forge_object::{blob, history};
use serde_json::{json, Map, Value};

/// Fixed repository PDA used for history/repo-root vectors.
pub const REPO_PDA: [u8; 32] = [0x11; 32];
/// Fixed base58 author placeholder.
pub const AUTHOR: &str = "9aBauthorPlaceholder1111111111111111111111";
/// Fixed base58 nonce placeholder.
pub const NONCE: &str = "nonceNonceNonceNonceNonceNonce22";
/// Fixed commit timestamp.
pub const COMMIT_TS: i64 = 1_700_000_000;
/// Fixed commit message.
pub const COMMIT_MSG: &[u8] = b"Initial commit\n";

/// A file to place in the fixture tree.
#[derive(Clone, Debug)]
pub struct FileSpec {
    /// Repo-relative path using `/` separators.
    pub path: String,
    /// Raw file content (or symlink target for [`EntryMode::Symlink`]).
    pub content: Vec<u8>,
    /// Entry mode.
    pub mode: EntryMode,
}

fn spec(path: &str, content: Vec<u8>, mode: EntryMode) -> FileSpec {
    FileSpec {
        path: path.to_string(),
        content,
        mode,
    }
}

/// The canonical fixture file set.
pub fn fixture() -> Vec<FileSpec> {
    vec![
        spec("README.md", b"hello\n".to_vec(), EntryMode::Regular),
        spec(
            "run.sh",
            b"#!/bin/sh\necho hi\n".to_vec(),
            EntryMode::Executable,
        ),
        spec(
            "src/main.rs",
            b"fn main() {}\n".to_vec(),
            EntryMode::Regular,
        ),
        spec("src/lib.rs", Vec::new(), EntryMode::Regular),
        spec(
            "docs/notes.md",
            "café\n".as_bytes().to_vec(),
            EntryMode::Regular,
        ),
        spec("link", b"README.md".to_vec(), EntryMode::Symlink),
    ]
}

/// The result of building the fixture tree.
pub struct BuiltTree {
    /// Root tree oid.
    pub root: Oid,
    /// Blob oids keyed by repo-relative path.
    pub blobs: BTreeMap<String, Oid>,
}

/// Builds the fixture tree recursively using the canonical engine.
///
/// # Errors
/// Propagates tree construction errors.
pub fn build_tree_files(
    files: &[FileSpec],
    algorithm: HashAlgorithm,
) -> Result<BuiltTree, ObjectError> {
    build_dir(files, "", algorithm)
}

fn build_dir(
    files: &[FileSpec],
    prefix: &str,
    algorithm: HashAlgorithm,
) -> Result<BuiltTree, ObjectError> {
    let mut builder = TreeBuilder::new(algorithm);
    let mut blobs = BTreeMap::new();
    let mut subdirs: BTreeMap<String, Vec<FileSpec>> = BTreeMap::new();

    for file in files {
        let relative = if prefix.is_empty() {
            file.path.as_str()
        } else {
            file.path
                .strip_prefix(prefix)
                .expect("fixture paths are well formed")
                .trim_start_matches('/')
        };
        if let Some((dir, _rest)) = relative.split_once('/') {
            subdirs
                .entry(dir.to_string())
                .or_default()
                .push(file.clone());
        } else {
            let blob_oid = blob::blob_oid(&file.content, algorithm);
            blobs.insert(file.path.clone(), blob_oid.clone());
            builder.add(file.mode, relative.as_bytes(), blob_oid)?;
        }
    }

    for (dir, children) in subdirs {
        let child_prefix = if prefix.is_empty() {
            dir.clone()
        } else {
            format!("{prefix}/{dir}")
        };
        let subtree = build_dir(&children, &child_prefix, algorithm)?;
        blobs.extend(subtree.blobs);
        builder.add(EntryMode::Tree, dir.as_bytes(), subtree.root)?;
    }

    Ok(BuiltTree {
        root: builder.build_oid(),
        blobs,
    })
}

/// Builds the deterministic fixture commit over `root`.
///
/// # Errors
/// Propagates commit construction errors.
pub fn commit_for(root: Oid, algorithm: HashAlgorithm) -> Result<Commit, ObjectError> {
    let identity = Identity::new("Alice", "alice@example.com", COMMIT_TS, 0)?;
    Commit::new(
        algorithm,
        root,
        Vec::new(),
        identity.clone(),
        identity,
        COMMIT_MSG.to_vec(),
    )
}

/// Builds the deterministic fixture attestation for a root commit.
///
/// # Errors
/// Propagates attestation construction errors.
pub fn attestation_for(
    root: Oid,
    commit: Oid,
    algorithm: HashAlgorithm,
) -> Result<Attestation, ObjectError> {
    let message_hash =
        Oid::new(algorithm, algorithm.digest(COMMIT_MSG)).expect("digest length matches algorithm");
    Attestation::new(
        ATTESTATION_VERSION,
        "7xKrepoPdaPlaceholder111111111111111111111",
        commit,
        Vec::new(),
        root,
        AUTHOR,
        COMMIT_TS,
        message_hash,
        NONCE,
    )
}

/// Computes the full set of golden values for both hash algorithms.
///
/// This is compared byte-for-byte against `tests/vectors/golden.json`.
pub fn compute_json() -> Value {
    let files = fixture();
    let file_json: Vec<Value> = files
        .iter()
        .map(|f| {
            json!({
                "path": f.path,
                "content_hex": hex::encode(&f.content),
                "mode": f.mode.as_ascii(),
            })
        })
        .collect();

    let mut algorithms = Map::new();
    for (name, algorithm) in [
        ("sha1", HashAlgorithm::Sha1),
        ("sha256", HashAlgorithm::Sha256),
    ] {
        let built = build_tree_files(&files, algorithm).expect("fixture tree builds");
        let commit = commit_for(built.root.clone(), algorithm).expect("fixture commit builds");
        let attestation =
            attestation_for(built.root.clone(), commit.oid(), algorithm).expect("attestation");
        let genesis = history::genesis_history_root(&REPO_PDA, algorithm);
        let history_root =
            history::append_history_root(algorithm, &genesis, &commit.oid(), 0).expect("history");
        let repo_root = history::repo_root(algorithm, &REPO_PDA, &commit.oid(), &history_root, 1)
            .expect("root");

        let blobs: Map<String, Value> = built
            .blobs
            .iter()
            .map(|(path, oid)| (path.clone(), Value::String(oid.to_hex())))
            .collect();

        algorithms.insert(
            name.to_string(),
            json!({
                "blobs": blobs,
                "root_tree": built.root.to_hex(),
                "commit": commit.oid().to_hex(),
                "genesis_history_root": genesis.to_hex(),
                "history_root": history_root.to_hex(),
                "repo_root": repo_root.to_hex(),
                "attestation_cbor_hex": hex::encode(attestation.to_canonical_cbor()),
                "attestation_hash": attestation.attestation_hash().to_hex(),
            }),
        );
    }

    json!({
        "repo_pda": hex::encode(REPO_PDA),
        "files": file_json,
        "algorithms": algorithms,
    })
}
