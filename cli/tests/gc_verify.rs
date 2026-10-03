//! CLI fixture: `forge gc --verify-availability` lists unbacked objects.

use forge_object::hash::HashAlgorithm;
use forge_object::object::ObjectType;
use forge_storage::object::GitObject;
use forge_storage::{cas_path, storage_index_path, FsBackend, StorageBackend, StorageIndex};
use std::process::Command;

#[test]
fn gc_verify_availability_lists_unbacked_objects() {
    let dir = tempfile::tempdir().unwrap();
    let cas = FsBackend::open("cas", cas_path(dir.path())).unwrap();
    let obj = GitObject::from_payload(ObjectType::Blob, b"missing".to_vec(), HashAlgorithm::Sha256);
    let loc = cas.put(&obj.framed()).unwrap();
    let mut index = StorageIndex::new("sha256");
    index.record(&obj, vec![loc.clone()]).unwrap();
    index.save(storage_index_path(dir.path())).unwrap();
    cas.delete(&loc.id).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_forge"))
        .current_dir(dir.path())
        .args(["gc", "--verify-availability"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "expected non-zero exit, stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("missing") || stderr.contains("missing"),
        "stdout={stdout} stderr={stderr}"
    );
}
