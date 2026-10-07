//! Git object store via the `git` CLI plus `forge-object` hashing (ADR 0007).
//!
//! SHA-256 repositories are created with `git init --object-format=sha256`.
//! Trees and commits are built with `forge-object` and written with
//! `git hash-object` so `git rev-parse HEAD` equals the Forge commit oid.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{anyhow, bail, Context, Result};
use forge_object::blob::blob_oid;
use forge_object::commit::{commit_tree_and_parents, Commit, Identity};
use forge_object::hash::{HashAlgorithm, Oid};
use forge_object::object::{self, ObjectType};
use forge_object::path;
use forge_object::tree::{parse_tree, EntryMode, TreeBuilder, TreeEntry};

use forge_storage::graph::ObjectSource;
use forge_storage::object::GitObject;
use forge_storage::StorageError;

use crate::config::ensure_inside;

/// A Git work tree with a `.git` directory.
#[derive(Debug, Clone)]
pub struct GitRepo {
    /// Work-tree root.
    pub root: PathBuf,
}

/// A staged index entry (`git ls-files -s`).
#[derive(Debug, Clone)]
pub struct IndexEntry {
    mode: EntryMode,
    oid: Oid,
    path: String,
}

impl GitRepo {
    /// Discovers the work-tree containing `start` (via `git rev-parse`).
    ///
    /// # Errors
    /// Not a Git repository, or object format is not SHA-256.
    pub fn discover(start: &Path) -> Result<Self> {
        let root = git_stdout(start, &["rev-parse", "--show-toplevel"], b"")?;
        let root = PathBuf::from(root);
        let format = git_stdout(&root, &["rev-parse", "--show-object-format"], b"")?;
        if format != "sha256" {
            bail!("Forge local repos must use SHA-256 (got {format}); re-init with `forge init`");
        }
        Ok(Self { root })
    }

    /// `git init --object-format=sha256 -b main` in `dir`.
    ///
    /// # Errors
    /// `git` failures.
    pub fn init(dir: &Path, default_branch: &str) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        git_stdout(
            dir,
            &["init", "-q", "--object-format=sha256", "-b", default_branch],
            b"",
        )?;
        Self::discover(dir)
    }

    /// `git add -- <paths>`. Paths are checked for work-tree escape and
    /// Forge-safe path components (§5.3).
    ///
    /// # Errors
    /// Unsafe paths or `git add` failure.
    pub fn add(&self, paths: &[String]) -> Result<()> {
        if paths.is_empty() {
            bail!("nothing specified, nothing added");
        }
        let mut args = vec!["add".to_string(), "--".to_string()];
        for raw in paths {
            let requested = Path::new(raw);
            ensure_inside(&self.root, requested)?;
            validate_repo_path(raw)?;
            args.push(raw.clone());
        }
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        git_stdout(&self.root, &arg_refs, b"")?;
        Ok(())
    }

    /// Staged + unstaged short status (`git status --short`).
    ///
    /// # Errors
    /// `git` failure.
    pub fn status_short(&self) -> Result<String> {
        git_stdout(&self.root, &["status", "--short"], b"")
    }

    /// Current branch short name, or `HEAD` if detached.
    pub fn current_branch(&self) -> String {
        git_stdout(
            &self.root,
            &["symbolic-ref", "--quiet", "--short", "HEAD"],
            b"",
        )
        .unwrap_or_else(|_| "HEAD".into())
    }

    /// Whether `HEAD` points at a commit.
    pub fn head_born(&self) -> bool {
        git_stdout(&self.root, &["rev-parse", "--verify", "-q", "HEAD"], b"").is_ok()
    }

    /// `HEAD` commit hex oid, if born.
    ///
    /// # Errors
    /// `git` failure when HEAD exists but cannot be parsed.
    pub fn head_oid(&self, algorithm: HashAlgorithm) -> Result<Option<Oid>> {
        if !self.head_born() {
            return Ok(None);
        }
        let hex = git_stdout(&self.root, &["rev-parse", "HEAD"], b"")?;
        Ok(Some(Oid::from_hex(algorithm, &hex)?))
    }

    /// List local branches (`git branch --format`).
    ///
    /// # Errors
    /// `git` failure.
    pub fn list_branches(&self) -> Result<String> {
        git_stdout(
            &self.root,
            &["branch", "--format=%(HEAD) %(refname:short)"],
            b"",
        )
    }

    /// Create `name` at `HEAD` (must be born).
    ///
    /// # Errors
    /// Duplicate name or unborn HEAD.
    pub fn create_branch(&self, name: &str) -> Result<()> {
        validate_repo_path(name)?;
        git_stdout(&self.root, &["branch", name], b"")?;
        Ok(())
    }

    /// `git checkout <ref>`.
    ///
    /// # Errors
    /// `git checkout` failure.
    pub fn checkout(&self, git_ref: &str) -> Result<()> {
        git_stdout(&self.root, &["checkout", "-q", git_ref], b"")?;
        Ok(())
    }

    /// Create a lightweight git tag `name` at `oid`.
    ///
    /// # Errors
    /// Invalid name or `git tag` failure.
    pub fn create_tag(&self, name: &str, oid: &Oid) -> Result<()> {
        validate_repo_path(name)?;
        git_stdout(&self.root, &["tag", name, &oid.to_hex()], b"")?;
        Ok(())
    }

    /// `git merge --no-ff <branch>` with an explicit committer identity.
    ///
    /// # Errors
    /// `git merge` failure (e.g. conflicts).
    pub fn merge_no_ff(&self, branch: &str, name: &str, email: &str, timestamp: i64) -> Result<()> {
        validate_repo_path(branch)?;
        let date = format!("{timestamp} +0000");
        let message = format!("Merge branch '{branch}'");
        let output = Command::new("git")
            .current_dir(&self.root)
            .args(["merge", "--no-ff", "-m", &message, branch])
            .env("GIT_AUTHOR_NAME", name)
            .env("GIT_AUTHOR_EMAIL", email)
            .env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_NAME", name)
            .env("GIT_COMMITTER_EMAIL", email)
            .env("GIT_COMMITTER_DATE", &date)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .context("spawn git merge")?;
        if !output.status.success() {
            bail!(
                "git merge failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(())
    }

    /// Walk first-parent history (`oid`, subject) from `branch` or HEAD.
    ///
    /// # Errors
    /// Unborn HEAD or `git log` failure.
    pub fn log(
        &self,
        branch: Option<&str>,
        algorithm: HashAlgorithm,
    ) -> Result<Vec<(Oid, String)>> {
        if !self.head_born() && branch.is_none() {
            return Ok(Vec::new());
        }
        let rev = branch.unwrap_or("HEAD");
        let out = git_stdout(&self.root, &["log", "--format=%H%x00%s", rev, "--"], b"")?;
        let mut rows = Vec::new();
        for line in out.lines() {
            if line.is_empty() {
                continue;
            }
            let (hex, subject) = line.split_once('\0').unwrap_or((line, ""));
            rows.push((Oid::from_hex(algorithm, hex)?, subject.to_string()));
        }
        Ok(rows)
    }

    /// Raw object payload (`git cat-file -t` + `git cat-file <type>`).
    ///
    /// # Errors
    /// Missing object.
    pub fn cat_payload(&self, oid: &Oid) -> Result<(ObjectType, Vec<u8>)> {
        let hex = oid.to_hex();
        let type_s = git_stdout(&self.root, &["cat-file", "-t", &hex], b"")?;
        let object_type = ObjectType::parse(&type_s)?;
        let payload = git_bytes(&self.root, &["cat-file", type_s.as_str(), &hex], b"")?;
        Ok((object_type, payload))
    }

    /// Writes a Git object from its payload; returns the oid git stored.
    ///
    /// # Errors
    /// `git hash-object` failure.
    pub fn write_payload(&self, object_type: ObjectType, payload: &[u8]) -> Result<Oid> {
        let hex = git_stdout(
            &self.root,
            &["hash-object", "-w", "-t", object_type.as_str(), "--stdin"],
            payload,
        )?;
        Oid::from_hex(HashAlgorithm::Sha256, &hex).map_err(Into::into)
    }

    /// Builds the index tree with `forge-object`, writes it, and returns the oid.
    /// Must match `git write-tree` (§5.3).
    ///
    /// # Errors
    /// Empty index, path-safety violations, or oid mismatch with Git.
    pub fn write_index_tree(&self) -> Result<Oid> {
        let entries = self.ls_files_stage()?;
        if entries.is_empty() {
            bail!("nothing to commit (empty index)");
        }
        for entry in &entries {
            validate_repo_path(&entry.path)?;
        }
        let our = build_tree_from_index(&entries, HashAlgorithm::Sha256)?;
        self.write_tree_recursive(&our)?;
        let git_tree = git_stdout(&self.root, &["write-tree"], b"")?;
        if git_tree != our.oid.to_hex() {
            bail!(
                "forge-object tree {} disagrees with git write-tree {git_tree}",
                our.oid.to_hex()
            );
        }
        Ok(our.oid)
    }

    fn write_tree_recursive(&self, node: &BuiltTree) -> Result<()> {
        for child in &node.children {
            self.write_tree_recursive(child)?;
        }
        let stored = self.write_payload(ObjectType::Tree, &node.payload)?;
        if stored != node.oid {
            bail!(
                "written tree oid {} != computed {}",
                stored.to_hex(),
                node.oid.to_hex()
            );
        }
        Ok(())
    }

    fn ls_files_stage(&self) -> Result<Vec<IndexEntry>> {
        let raw = git_bytes(&self.root, &["ls-files", "-s", "-z"], b"")?;
        let mut entries = Vec::new();
        for record in raw.split(|b| *b == 0) {
            if record.is_empty() {
                continue;
            }
            let text = std::str::from_utf8(record).context("index path is not UTF-8")?;
            let (meta, path) = text
                .split_once('\t')
                .ok_or_else(|| anyhow!("malformed ls-files line"))?;
            let mut parts = meta.split(' ');
            let mode_s = parts.next().ok_or_else(|| anyhow!("missing mode"))?;
            let oid_hex = parts.next().ok_or_else(|| anyhow!("missing oid"))?;
            let mode = EntryMode::from_ascii(mode_s)?;
            let oid = Oid::from_hex(HashAlgorithm::Sha256, oid_hex)?;
            entries.push(IndexEntry {
                mode,
                oid,
                path: path.to_string(),
            });
        }
        Ok(entries)
    }

    /// Writes `commit` and points `HEAD` at it.
    ///
    /// # Errors
    /// `git hash-object` / `update-ref` failure, or oid mismatch.
    pub fn commit_onto_head(&self, commit: &Commit) -> Result<Oid> {
        let payload = commit.payload();
        let computed = commit.oid();
        let stored = self.write_payload(ObjectType::Commit, &payload)?;
        if stored != computed {
            bail!(
                "written commit {} != forge-object {}",
                stored.to_hex(),
                computed.to_hex()
            );
        }
        git_stdout(&self.root, &["update-ref", "HEAD", &computed.to_hex()], b"")?;
        Ok(computed)
    }

    /// Writes every object in `graph` into `.git`.
    pub fn write_graph(&self, graph: &forge_storage::ObjectGraph) -> Result<()> {
        for obj in graph.objects() {
            let stored = self.write_payload(obj.object_type, &obj.payload)?;
            if stored != obj.oid {
                bail!("written {} != {}", stored.to_hex(), obj.oid.to_hex());
            }
        }
        Ok(())
    }

    /// Points `refs/heads/<branch>` at `oid` and force-checkouts.
    pub fn checkout_branch(&self, branch: &str, oid: &Oid) -> Result<()> {
        git_stdout(
            &self.root,
            &["update-ref", &format!("refs/heads/{branch}"), &oid.to_hex()],
            b"",
        )?;
        git_stdout(
            &self.root,
            &["symbolic-ref", "HEAD", &format!("refs/heads/{branch}")],
            b"",
        )?;
        git_stdout(&self.root, &["checkout", "-f", "-q", branch], b"")?;
        Ok(())
    }

    /// Whether `ancestor` is an ancestor of `desc` (or equal).
    pub fn is_ancestor(&self, ancestor: &Oid, desc: &Oid) -> bool {
        git_stdout(
            &self.root,
            &[
                "merge-base",
                "--is-ancestor",
                &ancestor.to_hex(),
                &desc.to_hex(),
            ],
            b"",
        )
        .is_ok()
    }
}

impl ObjectSource for GitRepo {
    fn get(&self, oid: &Oid) -> Result<GitObject, StorageError> {
        let (kind, payload) = self
            .cat_payload(oid)
            .map_err(|err| StorageError::NotFound(err.to_string()))?;
        let obj = GitObject::from_payload(kind, payload, oid.algorithm());
        obj.verify()?;
        if obj.oid != *oid {
            return Err(StorageError::OidMismatch {
                expected: oid.to_tagged_string(),
                actual: obj.oid.to_tagged_string(),
            });
        }
        Ok(obj)
    }
}

struct BuiltTree {
    oid: Oid,
    payload: Vec<u8>,
    children: Vec<BuiltTree>,
}

fn build_tree_from_index(entries: &[IndexEntry], algorithm: HashAlgorithm) -> Result<BuiltTree> {
    build_dir(entries, "", algorithm)
}

fn build_dir(entries: &[IndexEntry], prefix: &str, algorithm: HashAlgorithm) -> Result<BuiltTree> {
    let mut builder = TreeBuilder::new(algorithm);
    let mut subdirs: BTreeMap<String, Vec<IndexEntry>> = BTreeMap::new();
    let mut children = Vec::new();

    for entry in entries {
        let relative = if prefix.is_empty() {
            entry.path.as_str()
        } else {
            entry
                .path
                .strip_prefix(prefix)
                .unwrap_or(&entry.path)
                .trim_start_matches('/')
        };
        if let Some((dir, _)) = relative.split_once('/') {
            subdirs
                .entry(dir.to_string())
                .or_default()
                .push(entry.clone());
        } else {
            builder.add(entry.mode, relative.as_bytes(), entry.oid.clone())?;
        }
    }

    for (dir, kids) in subdirs {
        let child_prefix = if prefix.is_empty() {
            format!("{dir}/")
        } else {
            format!("{prefix}{dir}/")
        };
        let subtree = build_dir(&kids, &child_prefix, algorithm)?;
        builder.add(EntryMode::Tree, dir.as_bytes(), subtree.oid.clone())?;
        children.push(subtree);
    }

    let payload = builder.clone().build_payload();
    let oid = object::oid(ObjectType::Tree, &payload, algorithm);
    Ok(BuiltTree {
        oid,
        payload,
        children,
    })
}

/// Validates every path component with [`path::sanitize_name`].
///
/// # Errors
/// [`forge_object::ObjectError::InvalidName`].
pub fn validate_repo_path(repo_path: &str) -> Result<()> {
    if repo_path == "." {
        return Ok(());
    }
    if repo_path.is_empty() {
        bail!("empty path");
    }
    if repo_path.starts_with('/') {
        bail!("absolute path rejected: {repo_path}");
    }
    for component in repo_path.split('/') {
        path::sanitize_name(component.as_bytes())?;
    }
    Ok(())
}

/// Recomputes blob/tree oids under `tree` and compares to stored objects.
///
/// # Errors
/// OID mismatch or missing object.
pub fn verify_tree(repo: &GitRepo, tree: &Oid, algorithm: HashAlgorithm) -> Result<()> {
    let (kind, payload) = repo.cat_payload(tree)?;
    if kind != ObjectType::Tree {
        bail!("expected tree {}", tree.to_hex());
    }
    let recomputed = object::oid(ObjectType::Tree, &payload, algorithm);
    if recomputed != *tree {
        bail!(
            "tree oid mismatch: expected {}, actual {}",
            tree.to_tagged_string(),
            recomputed.to_tagged_string()
        );
    }
    for entry in parse_tree(&payload, algorithm)? {
        match entry.mode {
            EntryMode::Tree => verify_tree(repo, &entry.oid, algorithm)?,
            EntryMode::Gitlink => {}
            EntryMode::Regular | EntryMode::Executable | EntryMode::Symlink => {
                verify_blob(repo, &entry, algorithm)?;
            }
        }
    }
    Ok(())
}

fn verify_blob(repo: &GitRepo, entry: &TreeEntry, algorithm: HashAlgorithm) -> Result<()> {
    let (kind, payload) = repo.cat_payload(&entry.oid)?;
    if kind != ObjectType::Blob {
        bail!("{} is not a blob", entry.oid.to_hex());
    }
    let actual = blob_oid(&payload, algorithm);
    if actual != entry.oid {
        bail!(
            "blob oid mismatch for {}: expected {}, actual {}",
            String::from_utf8_lossy(&entry.name),
            entry.oid.to_tagged_string(),
            actual.to_tagged_string()
        );
    }
    Ok(())
}

/// Reads a commit and checks its oid against `expected`.
///
/// # Errors
/// Parse or oid mismatch.
pub fn load_commit(repo: &GitRepo, expected: &Oid) -> Result<Commit> {
    let (kind, payload) = repo.cat_payload(expected)?;
    if kind != ObjectType::Commit {
        bail!("expected commit {}", expected.to_hex());
    }
    let commit = Commit::from_payload(expected.algorithm(), &payload)?;
    if commit.oid() != *expected {
        bail!(
            "commit oid mismatch: expected {}, actual {}",
            expected.to_tagged_string(),
            commit.oid().to_tagged_string()
        );
    }
    let (tree, parents) = commit_tree_and_parents(&payload, expected.algorithm())?;
    if tree != commit.tree || parents != commit.parents {
        bail!("commit header round-trip mismatch");
    }
    Ok(commit)
}

fn git_stdout(dir: &Path, args: &[&str], stdin: &[u8]) -> Result<String> {
    let bytes = git_bytes(dir, args, stdin)?;
    Ok(String::from_utf8(bytes)
        .context("git stdout is not UTF-8")?
        .trim()
        .to_string())
}

fn git_bytes(dir: &Path, args: &[&str], stdin: &[u8]) -> Result<Vec<u8>> {
    let mut command = Command::new("git");
    command
        .current_dir(dir)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().context("spawn git")?;
    child
        .stdin
        .as_mut()
        .context("git stdin")?
        .write_all(stdin)?;
    drop(child.stdin.take());
    let output = child.wait_with_output().context("wait git")?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        bail!("git {args:?} failed: {err}");
    }
    Ok(output.stdout)
}

/// Display identity derived from the wallet (authorship is the signature).
pub fn display_identity(pubkey: &str, timestamp: i64) -> Result<Identity> {
    Identity::new("Forge", format!("{pubkey}@forge"), timestamp, 0).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal_components() {
        assert!(validate_repo_path("..").is_err());
        assert!(validate_repo_path("foo/../bar").is_err());
        assert!(validate_repo_path("/abs").is_err());
        assert!(validate_repo_path("ok/file.rs").is_ok());
    }
}
