//! Storage consistency tests from architecture spec §24 / Phase 6.
//!
//! - OID/CID round-trip equality
//! - corrupted blob rejection
//! - killed-pin availability
//! - empty-object and large-bundle edges
//! - Arweave bundler dry-run (mock HTTP)

use forge_object::blob::blob_oid;
use forge_object::commit::{Commit, Identity};
use forge_object::hash::{HashAlgorithm, Oid};
use forge_object::object::ObjectType;
use forge_object::tree::{EntryMode, TreeBuilder};
use forge_storage::arweave::ArweaveBundler;
use forge_storage::gc::{verify_availability, AvailabilityStatus};
use forge_storage::graph::ObjectGraph;
use forge_storage::http::{HttpBody, HttpMethod, HttpResponse, MockTransport};
use forge_storage::object::GitObject;
use forge_storage::{
    fetch_object, upload_bundle, Car, Cid, MemoryBackend, MultiPin, StorageBackend, StorageError,
    StorageIndex,
};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};

fn identity() -> Identity {
    Identity::new("Alice", "a@x", 1, 0).unwrap()
}

fn mini_repo() -> (ObjectGraph, Oid) {
    let algo = HashAlgorithm::Sha256;
    let blob = GitObject::from_payload(ObjectType::Blob, b"hello\n".to_vec(), algo);
    let mut tree = TreeBuilder::new(algo);
    tree.add(EntryMode::Regular, b"hello.txt", blob.oid.clone())
        .unwrap();
    let tree_obj = GitObject::from_payload(ObjectType::Tree, tree.build_payload(), algo);
    let commit = Commit::new(
        algo,
        tree_obj.oid.clone(),
        vec![],
        identity(),
        identity(),
        b"first\n".to_vec(),
    )
    .unwrap();
    let commit_obj = GitObject::from_payload(ObjectType::Commit, commit.payload(), algo);
    let mut graph = ObjectGraph::new();
    graph.insert(blob).unwrap();
    graph.insert(tree_obj).unwrap();
    graph.insert(commit_obj.clone()).unwrap();
    (graph, commit_obj.oid)
}

#[test]
fn oid_cid_round_trip_equality() {
    let algo = HashAlgorithm::Sha256;
    let obj = GitObject::from_payload(ObjectType::Blob, b"hello\n".to_vec(), algo);
    let framed = obj.framed();
    let git_oid = blob_oid(b"hello\n", algo);
    assert_eq!(obj.oid, git_oid);
    let cid = Cid::of_raw_sha256(&framed);
    // For SHA-256 repos the Git OID digest equals the CID multihash (§8.3).
    assert_eq!(cid.digest(), git_oid.as_bytes());
    let car = Car::from_objects(std::slice::from_ref(&obj), &git_oid)
        .unwrap()
        .encode();
    let restored = Car::decode(&car).unwrap().get(&git_oid).unwrap();
    assert_eq!(restored, obj);
}

#[test]
fn corrupted_blob_is_rejected() {
    let a = MemoryBackend::new("a");
    let b = MemoryBackend::new("b");
    let pins = MultiPin::new(vec![&a, &b]).unwrap();
    let (graph, root) = mini_repo();
    let mut index = StorageIndex::new("sha256");
    let locs = upload_bundle(&pins, &graph, &root, &mut index).unwrap();
    let blob_oid = graph
        .objects()
        .find(|o| o.object_type == ObjectType::Blob)
        .unwrap()
        .oid
        .clone();

    let ok = fetch_object(&pins, &index, &blob_oid).unwrap();
    assert_eq!(ok.payload, b"hello\n");

    let mut corrupt = a.get(&locs[0].id).unwrap();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 0xff;
    a.insert_unchecked(&locs[0].id, corrupt.clone());
    b.insert_unchecked(&locs[1].id, corrupt);
    let err = fetch_object(&pins, &index, &blob_oid).unwrap_err();
    assert!(matches!(
        err,
        StorageError::CidMismatch { .. } | StorageError::OidMismatch { .. }
    ));
}

#[test]
fn killed_pin_still_serves_from_the_second() {
    let a = MemoryBackend::new("a");
    let b = MemoryBackend::new("b");
    let pins = MultiPin::new(vec![&a, &b]).unwrap();
    let (graph, root) = mini_repo();
    let mut index = StorageIndex::new("sha256");
    upload_bundle(&pins, &graph, &root, &mut index).unwrap();
    a.kill();
    let fetched = fetch_object(&pins, &index, &root).unwrap();
    assert_eq!(fetched.oid, root);

    let backends: Vec<&dyn StorageBackend> = vec![&a, &b];
    let report = verify_availability(&index, &backends).unwrap();
    assert_eq!(report.objects[0].status, AvailabilityStatus::Degraded);
}

#[test]
fn empty_blob_round_trip() {
    let obj = GitObject::from_payload(ObjectType::Blob, Vec::new(), HashAlgorithm::Sha256);
    let a = MemoryBackend::new("a");
    let b = MemoryBackend::new("b");
    let loc_a = a.put(&obj.framed()).unwrap();
    let loc_b = b.put(&obj.framed()).unwrap();
    let mut index = StorageIndex::new("sha256");
    index.record(&obj, vec![loc_a, loc_b]).unwrap();
    let pins = MultiPin::new(vec![&a, &b]).unwrap();
    let got = fetch_object(&pins, &index, &obj.oid).unwrap();
    assert!(got.payload.is_empty());
}

#[test]
fn large_bundle_car_round_trip() {
    let algo = HashAlgorithm::Sha256;
    let payload = vec![0xABu8; 64 * 1024];
    let blob = GitObject::from_payload(ObjectType::Blob, payload.clone(), algo);
    let mut tree = TreeBuilder::new(algo);
    tree.add(EntryMode::Regular, b"big.bin", blob.oid.clone())
        .unwrap();
    let tree_obj = GitObject::from_payload(ObjectType::Tree, tree.build_payload(), algo);
    let mut graph = ObjectGraph::new();
    graph.insert(blob.clone()).unwrap();
    graph.insert(tree_obj.clone()).unwrap();
    let car = graph.to_car(&tree_obj.oid).unwrap().encode();
    let restored = ObjectGraph::from_car(&car, algo).unwrap();
    assert_eq!(restored.get(&blob.oid).unwrap().payload, payload);
}

fn mock_bundler() -> MockTransport {
    let store = Mutex::new(HashMap::<String, Vec<u8>>::new());
    let seq = AtomicU64::new(1);
    MockTransport::new(move |req| {
        if req.url.ends_with("/tx") && matches!(req.method, HttpMethod::Post) {
            let bytes = match req.body {
                HttpBody::Raw(b) => b.to_vec(),
                _ => Vec::new(),
            };
            let id = format!("tx{}", seq.fetch_add(1, Ordering::SeqCst));
            store.lock().expect("store").insert(id.clone(), bytes);
            let body = serde_json::json!({ "id": id }).to_string().into_bytes();
            return Ok(HttpResponse { status: 200, body });
        }
        if matches!(req.method, HttpMethod::Get) {
            let id = req.url.rsplit('/').next().unwrap_or_default().to_string();
            if let Some(bytes) = store.lock().expect("store").get(&id) {
                return Ok(HttpResponse {
                    status: 200,
                    body: bytes.clone(),
                });
            }
            return Err(StorageError::Http("status 404".into()));
        }
        Err(StorageError::Http(format!("unexpected {}", req.url)))
    })
}

#[test]
fn arweave_mock_upload_and_chunking() {
    let client = ArweaveBundler::new("irys", "http://bundler.test", mock_bundler());
    let loc = client.upload(b"tag-checkpoint").unwrap();
    assert_eq!(client.get(&loc.id).unwrap(), b"tag-checkpoint");

    let chunked =
        ArweaveBundler::new("irys", "http://bundler.test", mock_bundler()).with_max_item_bytes(16);
    let payload = vec![1u8; 50];
    let loc = chunked.upload(&payload).unwrap();
    assert_eq!(chunked.get(&loc.id).unwrap(), payload);
}

#[test]
fn storage_hints_are_not_trusted() {
    let algo = HashAlgorithm::Sha256;
    let obj = GitObject::from_payload(ObjectType::Blob, b"real".to_vec(), algo);
    let other = GitObject::from_payload(ObjectType::Blob, b"fake".to_vec(), algo);
    let a = MemoryBackend::new("a");
    let b = MemoryBackend::new("b");
    let loc_a = a.put(&other.framed()).unwrap();
    let loc_b = b.put(&other.framed()).unwrap();
    let mut index = StorageIndex::new("sha256");
    index.record(&obj, vec![loc_a, loc_b]).unwrap();
    let pins = MultiPin::new(vec![&a, &b]).unwrap();
    let err = fetch_object(&pins, &index, &obj.oid).unwrap_err();
    assert!(matches!(
        err,
        StorageError::OidMismatch { .. } | StorageError::NotFound(_)
    ));
}

#[test]
fn gc_fixture_lists_unbacked_objects() {
    let dir = tempfile::tempdir().unwrap();
    let cas = forge_storage::FsBackend::open("cas", forge_storage::cas_path(dir.path())).unwrap();
    let obj = GitObject::from_payload(ObjectType::Blob, b"gone".to_vec(), HashAlgorithm::Sha256);
    let loc = cas.put(&obj.framed()).unwrap();
    let mut index = StorageIndex::new("sha256");
    index.record(&obj, vec![loc.clone()]).unwrap();
    index
        .save(forge_storage::storage_index_path(dir.path()))
        .unwrap();
    cas.delete(&loc.id).unwrap();
    let report = forge_storage::report_for_forge_dir(dir.path(), &[]).unwrap();
    assert_eq!(report.unbacked().count(), 1);
    assert!(report.render().contains("missing"));
}
