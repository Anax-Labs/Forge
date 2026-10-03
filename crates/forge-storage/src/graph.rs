//! Object graph walk and bundle construction from a commit/tree root (§8.3).

use std::collections::{BTreeMap, HashSet};

use forge_object::commit::commit_tree_and_parents;
use forge_object::hash::{HashAlgorithm, Oid};
use forge_object::object::ObjectType;
use forge_object::tree::{parse_tree, EntryMode};

use crate::car::Car;
use crate::error::StorageError;
use crate::object::GitObject;

/// Looks up a Git object by oid. Implementations must return **bytes they
/// have not yet authenticated**; [`ObjectGraph::insert`] always re-hashes.
pub trait ObjectSource {
    /// Fetch one object. The oid in the result is recomputed by the graph.
    ///
    /// # Errors
    /// Returns [`StorageError::NotFound`] or codec errors.
    fn get(&self, oid: &Oid) -> Result<GitObject, StorageError>;
}

/// A content-addressed set of Git objects, keyed by tagged oid.
#[derive(Debug, Clone, Default)]
pub struct ObjectGraph {
    objects: BTreeMap<String, GitObject>,
}

impl ObjectGraph {
    /// Empty graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of stored objects.
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    /// Whether the graph has no objects.
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    /// Inserts `obj` after recomputing its oid.
    ///
    /// # Errors
    /// Returns [`StorageError::OidMismatch`] if verification fails.
    pub fn insert(&mut self, obj: GitObject) -> Result<(), StorageError> {
        obj.verify()?;
        self.objects.insert(obj.oid.to_tagged_string(), obj);
        Ok(())
    }

    /// Looks up an object by oid.
    pub fn get(&self, oid: &Oid) -> Option<&GitObject> {
        self.objects.get(&oid.to_tagged_string())
    }

    /// All objects in tagged-oid order.
    pub fn objects(&self) -> impl Iterator<Item = &GitObject> {
        self.objects.values()
    }

    /// Walks `start` (commit or tree) through `source`, inserting every
    /// reachable blob/tree/commit. Gitlinks are recorded as tree entries only
    /// and are not followed (§5.3).
    ///
    /// # Errors
    /// Propagates missing objects and codec/hash mismatches.
    pub fn collect_reachable<S: ObjectSource>(
        source: &S,
        start: &Oid,
        algorithm: HashAlgorithm,
    ) -> Result<Self, StorageError> {
        let mut graph = Self::new();
        let mut seen = HashSet::new();
        walk(source, start, algorithm, &mut graph, &mut seen)?;
        Ok(graph)
    }

    /// Encodes the graph as a CARv1 whose root is `root`.
    ///
    /// # Errors
    /// Returns [`StorageError::NotFound`] if `root` is missing.
    pub fn to_car(&self, root: &Oid) -> Result<Car, StorageError> {
        let objects: Vec<GitObject> = self.objects.values().cloned().collect();
        Car::from_objects(&objects, root)
    }

    /// Decodes a CAR and inserts every Git object after OID verification.
    ///
    /// # Errors
    /// Returns CAR, CID, or OID errors.
    pub fn from_car(bytes: &[u8], algorithm: HashAlgorithm) -> Result<Self, StorageError> {
        let car = Car::decode(bytes)?;
        let mut graph = Self::new();
        for obj in car.unpack(algorithm)? {
            graph.insert(obj)?;
        }
        Ok(graph)
    }
}

impl ObjectSource for ObjectGraph {
    fn get(&self, oid: &Oid) -> Result<GitObject, StorageError> {
        self.objects
            .get(&oid.to_tagged_string())
            .cloned()
            .ok_or_else(|| StorageError::NotFound(oid.to_tagged_string()))
    }
}

fn walk<S: ObjectSource>(
    source: &S,
    oid: &Oid,
    algorithm: HashAlgorithm,
    graph: &mut ObjectGraph,
    seen: &mut HashSet<String>,
) -> Result<(), StorageError> {
    let key = oid.to_tagged_string();
    if !seen.insert(key) {
        return Ok(());
    }
    let obj = source.get(oid)?;
    if obj.oid != *oid {
        return Err(StorageError::OidMismatch {
            expected: oid.to_tagged_string(),
            actual: obj.oid.to_tagged_string(),
        });
    }
    obj.verify()?;
    match obj.object_type {
        ObjectType::Commit => {
            let (tree, parents) = commit_tree_and_parents(&obj.payload, algorithm)?;
            graph.insert(obj)?;
            walk(source, &tree, algorithm, graph, seen)?;
            for parent in parents {
                walk(source, &parent, algorithm, graph, seen)?;
            }
        }
        ObjectType::Tree => {
            let entries = parse_tree(&obj.payload, algorithm)?;
            graph.insert(obj)?;
            for entry in entries {
                if entry.mode == EntryMode::Gitlink {
                    continue;
                }
                walk(source, &entry.oid, algorithm, graph, seen)?;
            }
        }
        ObjectType::Blob | ObjectType::Tag => {
            graph.insert(obj)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_object::blob::blob_oid;
    use forge_object::commit::{Commit, Identity};
    use forge_object::tree::{EntryMode, TreeBuilder};

    fn identity() -> Identity {
        Identity::new("Alice", "a@x", 1, 0).unwrap()
    }

    #[test]
    fn walk_collects_commit_tree_and_blobs() {
        let algo = HashAlgorithm::Sha256;
        let blob = GitObject::from_payload(ObjectType::Blob, b"hi\n".to_vec(), algo);
        let mut tree = TreeBuilder::new(algo);
        tree.add(EntryMode::Regular, b"README", blob.oid.clone())
            .unwrap();
        let tree_payload = tree.build_payload();
        let tree_obj = GitObject::from_payload(ObjectType::Tree, tree_payload, algo);
        let commit = Commit::new(
            algo,
            tree_obj.oid.clone(),
            vec![],
            identity(),
            identity(),
            b"init\n".to_vec(),
        )
        .unwrap();
        let commit_obj = GitObject::from_payload(ObjectType::Commit, commit.payload(), algo);

        let mut source = ObjectGraph::new();
        source.insert(blob.clone()).unwrap();
        source.insert(tree_obj.clone()).unwrap();
        source.insert(commit_obj.clone()).unwrap();

        let collected = ObjectGraph::collect_reachable(&source, &commit_obj.oid, algo).unwrap();
        assert_eq!(collected.len(), 3);
        assert_eq!(
            collected.get(&blob.oid).unwrap().oid,
            blob_oid(b"hi\n", algo)
        );

        let car = collected.to_car(&commit_obj.oid).unwrap().encode();
        let restored = ObjectGraph::from_car(&car, algo).unwrap();
        assert_eq!(restored.len(), 3);
        restored.get(&commit_obj.oid).unwrap();
    }
}
