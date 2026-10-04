//! Phase 7 local workflow: init → add → commit → log → verify.

use std::fs;
use std::path::Path;
use std::process::Command;

fn forge() -> Command {
    Command::new(env!("CARGO_BIN_EXE_forge"))
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_ok(dir: &Path, args: &[&str]) -> String {
    let out = forge().current_dir(dir).args(args).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "forge {args:?} failed status={:?} stdout={stdout} stderr={stderr}",
        out.status.code()
    );
    stdout.trim().to_string()
}

fn run_status(dir: &Path, args: &[&str]) -> (i32, String, String) {
    let out = forge().current_dir(dir).args(args).output().unwrap();
    (
        out.status.code().unwrap_or(1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn local_init_add_commit_log_verify() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    run_ok(dir, &["init", "."]);
    assert!(dir.join(".git").exists());
    assert!(dir.join(".forge/config").exists());
    assert!(dir.join(".forge/id.json").exists());

    fs::write(dir.join("README.md"), "hello\n").unwrap();
    run_ok(dir, &["add", "README.md"]);
    let tagged = run_ok(dir, &["commit", "-m", "x"]);
    assert!(tagged.starts_with("sha256:"), "{tagged}");
    let hex = tagged.rsplit(':').next().unwrap();
    let git_head = git(dir, &["rev-parse", "HEAD"]);
    assert_eq!(git_head, hex, "commit oid must equal git rev-parse HEAD");

    let log = run_ok(dir, &["log"]);
    assert!(log.contains(&tagged), "{log}");
    assert!(log.contains("author="), "{log}");

    let status = run_ok(dir, &["status"]);
    assert!(status.contains("On branch main"), "{status}");

    let (code, stdout, stderr) = run_status(dir, &["verify"]);
    assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");
    assert!(stdout.contains("LOCAL_VERIFIED"), "{stdout}");
}

#[test]
fn sidecar_tamper_fails_verify() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    run_ok(dir, &["init", "."]);
    fs::write(dir.join("a.txt"), "a\n").unwrap();
    run_ok(dir, &["add", "a.txt"]);
    let tagged = run_ok(dir, &["commit", "-m", "a"]);
    let hex = tagged.rsplit(':').next().unwrap();
    let cbor = dir.join(".forge/attestations").join(format!("{hex}.cbor"));
    let mut bytes = fs::read(&cbor).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    fs::write(&cbor, bytes).unwrap();
    let (code, stdout, stderr) = run_status(dir, &["verify"]);
    assert_ne!(
        code, 0,
        "expected verify failure stdout={stdout} stderr={stderr}"
    );
    assert!(
        code == 1 || code == 3,
        "expected mismatch or missing data, got {code} stdout={stdout} stderr={stderr}"
    );
}

#[test]
fn path_traversal_add_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    run_ok(dir, &["init", "."]);
    let (code, _, stderr) = run_status(dir, &["add", "../outside"]);
    assert_ne!(code, 0);
    assert!(
        stderr.contains("escape") || stderr.contains("inside") || stderr.contains("rejected"),
        "{stderr}"
    );
}

#[test]
fn remote_add_list_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    run_ok(dir, &["init", "."]);
    run_ok(
        dir,
        &[
            "remote",
            "add",
            "origin",
            "LocalRepoPlaceholder111111111111111111111",
        ],
    );
    let listed = run_ok(dir, &["remote", "list"]);
    assert!(listed.contains("origin"), "{listed}");
}

#[test]
fn branch_and_checkout() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    run_ok(dir, &["init", "."]);
    fs::write(dir.join("a.txt"), "a\n").unwrap();
    run_ok(dir, &["add", "a.txt"]);
    run_ok(dir, &["commit", "-m", "a"]);
    run_ok(dir, &["branch", "feature"]);
    let listed = run_ok(dir, &["branch"]);
    assert!(listed.contains("feature"), "{listed}");
    run_ok(dir, &["checkout", "feature"]);
    let status = run_ok(dir, &["status"]);
    assert!(status.contains("On branch feature"), "{status}");
}
