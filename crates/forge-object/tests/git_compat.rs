//! Cross-compatibility test: the Forge engine must produce byte-identical
//! object ids to the real `git` binary, for both SHA-1 and SHA-256 repositories.
//!
//! This is the strongest determinism check in Phase 2: `git` independently
//! frames blobs, sorts and serializes trees, and serializes commits.

mod common;

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use forge_object::hash::HashAlgorithm;
use forge_object::tree::EntryMode;
use tempfile::TempDir;

struct GitRepo {
    dir: PathBuf,
    _tmp: TempDir,
}

impl GitRepo {
    fn init(algorithm: HashAlgorithm) -> Self {
        let tmp = TempDir::new().expect("tempdir");
        let dir = tmp.path().to_path_buf();
        let mut args = vec!["init", "-q"];
        if algorithm == HashAlgorithm::Sha256 {
            args.push("--object-format=sha256");
        }
        run_git(&dir, &args, b"", &[]);
        Self { dir, _tmp: tmp }
    }

    fn hash_blob(&self, content: &[u8]) -> String {
        run_git(
            &self.dir,
            &["hash-object", "-w", "-t", "blob", "--stdin"],
            content,
            &[],
        )
    }

    fn mktree(&self, input: &[u8]) -> String {
        run_git(&self.dir, &["mktree"], input, &[])
    }

    fn commit_tree(&self, tree: &str, message: &[u8]) -> String {
        run_git(
            &self.dir,
            &["commit-tree", tree],
            message,
            &[
                ("GIT_AUTHOR_NAME", "Alice"),
                ("GIT_AUTHOR_EMAIL", "alice@example.com"),
                ("GIT_AUTHOR_DATE", "@1700000000 +0000"),
                ("GIT_COMMITTER_NAME", "Alice"),
                ("GIT_COMMITTER_EMAIL", "alice@example.com"),
                ("GIT_COMMITTER_DATE", "@1700000000 +0000"),
            ],
        )
    }
}

fn run_git(dir: &Path, args: &[&str], stdin: &[u8], envs: &[(&str, &str)]) -> String {
    let mut command = Command::new("git");
    command
        .current_dir(dir)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in envs {
        command.env(key, value);
    }
    let mut child = command.spawn().expect("spawn git");
    child
        .stdin
        .as_mut()
        .expect("piped stdin")
        .write_all(stdin)
        .expect("write git stdin");
    drop(child.stdin.take());
    let output = child.wait_with_output().expect("wait git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git stdout is utf-8")
        .trim()
        .to_string()
}

fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn build_git_tree(repo: &GitRepo, files: &[common::FileSpec], prefix: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut subdirs: BTreeMap<String, Vec<common::FileSpec>> = BTreeMap::new();

    for file in files {
        let relative = if prefix.is_empty() {
            file.path.clone()
        } else {
            file.path
                .strip_prefix(prefix)
                .expect("well formed fixture path")
                .trim_start_matches('/')
                .to_string()
        };
        if let Some((dir, _rest)) = relative.split_once('/') {
            subdirs
                .entry(dir.to_string())
                .or_default()
                .push(file.clone());
        } else {
            let oid = repo.hash_blob(&file.content);
            let mode = match file.mode {
                EntryMode::Regular => "100644",
                EntryMode::Executable => "100755",
                EntryMode::Symlink => "120000",
                EntryMode::Gitlink => "160000",
                EntryMode::Tree => unreachable!("files never carry tree mode"),
            };
            lines.push(format!("{mode} blob {oid}\t{relative}"));
        }
    }

    for (dir, children) in subdirs {
        let child_prefix = if prefix.is_empty() {
            dir.clone()
        } else {
            format!("{prefix}/{dir}")
        };
        let subtree = build_git_tree(repo, &children, &child_prefix);
        lines.push(format!("040000 tree {subtree}\t{dir}"));
    }

    // Feed entries in reverse order so `git mktree` must sort them itself.
    lines.reverse();
    let mut input = lines.join("\n");
    input.push('\n');
    repo.mktree(input.as_bytes())
}

fn assert_git_agrees(algorithm: HashAlgorithm) {
    if !git_available() {
        eprintln!("git not available; skipping compatibility check");
        return;
    }

    let repo = GitRepo::init(algorithm);
    let files = common::fixture();
    let built = common::build_tree_files(&files, algorithm).expect("engine builds fixture tree");

    for file in &files {
        let our_oid = built.blobs.get(&file.path).expect("fixture blob was built");
        let git_oid = repo.hash_blob(&file.content);
        assert_eq!(
            git_oid,
            our_oid.to_hex(),
            "blob oid mismatch for {} ({})",
            file.path,
            algorithm.tag()
        );
    }

    let git_root = build_git_tree(&repo, &files, "");
    assert_eq!(
        git_root,
        built.root.to_hex(),
        "root tree oid mismatch ({})",
        algorithm.tag()
    );

    let git_commit = repo.commit_tree(&git_root, common::COMMIT_MSG);
    let our_commit = common::commit_for(built.root.clone(), algorithm).expect("engine commit");
    assert_eq!(
        git_commit,
        our_commit.oid().to_hex(),
        "commit oid mismatch ({})",
        algorithm.tag()
    );
}

#[test]
fn engine_matches_git_sha1() {
    assert_git_agrees(HashAlgorithm::Sha1);
}

#[test]
fn engine_matches_git_sha256() {
    assert_git_agrees(HashAlgorithm::Sha256);
}
