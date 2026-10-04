//! Phase 8 onchain workflow against `LiteSVM` (local-validator stand-in).

#![allow(clippy::similar_names)]

use std::fs;
use std::path::Path;
use std::sync::Mutex;

use forge::chain::{self, Chain};
use forge::commands;
use forge::sync;
use forge::{Command, VerifyStatus};
use solana_keypair::Keypair;
use solana_signer::Signer;

static CWD: Mutex<()> = Mutex::new(());

fn program_so() -> std::path::PathBuf {
    chain::default_program_so()
}

fn setup() -> Option<(tempfile::TempDir, Chain, Keypair)> {
    let so = program_so();
    if !so.exists() {
        eprintln!("skipping: {} missing (run `anchor build`)", so.display());
        return None;
    }
    let tmp = tempfile::tempdir().unwrap();
    let payer = Keypair::new();
    let chain = Chain::litesvm(&so, &payer).unwrap();
    Some((tmp, chain, payer))
}

fn dispatch_in(dir: &Path, cmd: Command) -> anyhow::Result<()> {
    let _guard = CWD.lock().unwrap();
    std::env::set_current_dir(dir).unwrap();
    commands::dispatch(cmd)
}

fn write_wallet(dir: &Path, kp: &Keypair) {
    forge::wallet::save_keypair(&dir.join(".forge/id.json"), kp).unwrap();
}

#[test]
fn push_clone_verify_matches_history_root() {
    let Some((tmp, chain, owner)) = setup() else {
        return;
    };
    sync::set_test_chain(Some(chain.clone()));
    let storage = tmp.path().join("cas-root");
    fs::create_dir_all(&storage).unwrap();
    forge::storage::set_test_storage(Some(storage.clone()));

    let origin = tmp.path().join("origin");
    fs::create_dir_all(&origin).unwrap();
    dispatch_in(
        &origin,
        Command::Init {
            dir: Some(".".into()),
        },
    )
    .unwrap();
    write_wallet(&origin, &owner);

    fs::write(origin.join("README.md"), "hello\n").unwrap();
    dispatch_in(
        &origin,
        Command::Add {
            paths: vec!["README.md".into()],
        },
    )
    .unwrap();
    dispatch_in(
        &origin,
        Command::Commit {
            message: "init".into(),
        },
    )
    .unwrap();
    dispatch_in(
        &origin,
        Command::Push {
            remote: None,
            branch: None,
        },
    )
    .unwrap();

    let config = forge::config::ForgeConfig::load(&origin).unwrap();
    assert!(!config.repo_pda.is_empty());
    let repo = chain::parse_address(&config.repo_pda).unwrap();
    let repo_state = chain.fetch_repo(&repo).unwrap().unwrap();
    assert_eq!(repo_state.commit_count, 1);
    assert_ne!(repo_state.history_root, [0u8; 32]);

    let clone_dir = tmp.path().join("clone");
    dispatch_in(
        tmp.path(),
        Command::Clone {
            repo: config.repo_pda.clone(),
            dir: Some(clone_dir.display().to_string()),
        },
    )
    .unwrap();
    assert!(clone_dir.join("README.md").exists());

    dispatch_in(&clone_dir, Command::Verify { commit: None }).unwrap();
    dispatch_in(&origin, Command::Verify { commit: None }).unwrap();

    let log = {
        let _g = CWD.lock().unwrap();
        std::env::set_current_dir(&clone_dir).unwrap();
        let mut cmd = std::process::Command::new("git");
        cmd.args(["rev-parse", "HEAD"]);
        String::from_utf8(cmd.output().unwrap().stdout)
            .unwrap()
            .trim()
            .to_string()
    };
    assert!(!log.is_empty());
    sync::set_test_chain(None);
    forge::storage::set_test_storage(None);
}

#[test]
fn forged_author_cannot_push() {
    let Some((tmp, chain, owner)) = setup() else {
        return;
    };
    sync::set_test_chain(Some(chain.clone()));
    let storage = tmp.path().join("cas-root");
    fs::create_dir_all(&storage).unwrap();
    forge::storage::set_test_storage(Some(storage.clone()));

    let origin = tmp.path().join("origin");
    fs::create_dir_all(&origin).unwrap();
    dispatch_in(
        &origin,
        Command::Init {
            dir: Some(".".into()),
        },
    )
    .unwrap();
    write_wallet(&origin, &owner);
    fs::write(origin.join("a.txt"), "a\n").unwrap();
    dispatch_in(
        &origin,
        Command::Add {
            paths: vec!["a.txt".into()],
        },
    )
    .unwrap();
    dispatch_in(
        &origin,
        Command::Commit {
            message: "a".into(),
        },
    )
    .unwrap();
    dispatch_in(
        &origin,
        Command::Push {
            remote: None,
            branch: None,
        },
    )
    .unwrap();

    let attacker = Keypair::new();
    chain
        .airdrop(&attacker.pubkey(), 10 * 1_000_000_000)
        .unwrap();
    write_wallet(&origin, &attacker);
    fs::write(origin.join("b.txt"), "b\n").unwrap();
    dispatch_in(
        &origin,
        Command::Add {
            paths: vec!["b.txt".into()],
        },
    )
    .unwrap();
    dispatch_in(
        &origin,
        Command::Commit {
            message: "b".into(),
        },
    )
    .unwrap();
    let err = dispatch_in(
        &origin,
        Command::Push {
            remote: None,
            branch: None,
        },
    )
    .unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("Unauthorized") || msg.contains("failed") || msg.contains("6001"),
        "{msg}"
    );

    sync::set_test_chain(None);
    forge::storage::set_test_storage(None);
}

#[test]
fn stale_update_branch_is_rejected() {
    let Some((tmp, chain, owner)) = setup() else {
        return;
    };
    sync::set_test_chain(Some(chain.clone()));
    let storage = tmp.path().join("cas-root");
    fs::create_dir_all(&storage).unwrap();
    forge::storage::set_test_storage(Some(storage.clone()));

    let origin = tmp.path().join("origin");
    fs::create_dir_all(&origin).unwrap();
    dispatch_in(
        &origin,
        Command::Init {
            dir: Some(".".into()),
        },
    )
    .unwrap();
    write_wallet(&origin, &owner);
    fs::write(origin.join("a.txt"), "a\n").unwrap();
    dispatch_in(
        &origin,
        Command::Add {
            paths: vec!["a.txt".into()],
        },
    )
    .unwrap();
    dispatch_in(
        &origin,
        Command::Commit {
            message: "a".into(),
        },
    )
    .unwrap();
    dispatch_in(
        &origin,
        Command::Push {
            remote: None,
            branch: None,
        },
    )
    .unwrap();

    let config = forge::config::ForgeConfig::load(&origin).unwrap();
    let repo = chain::parse_address(&config.repo_pda).unwrap();
    let git = forge::git::GitRepo::discover(&origin).unwrap();
    let tip = git
        .head_oid(forge_object::hash::HashAlgorithm::Sha256)
        .unwrap()
        .unwrap();
    let name = chain::pad_name("main").unwrap();
    let msg = forge_object::branch::branch_update_message(&repo.to_bytes(), &name, &tip, 0);
    let sig = owner.sign_message(&msg);
    let sig_b: [u8; 64] = sig.as_ref().try_into().unwrap();
    let ed = chain::ed25519_verify_instruction(&msg, &sig_b, &owner.pubkey().to_bytes());
    let ix = chain::update_branch_instruction(
        &chain.program_id(),
        &owner.pubkey(),
        repo,
        name,
        chain::oid32(&tip),
        0,
    );
    let err = chain.send(&owner, &[ed, ix]).unwrap_err();
    let text = err.to_string();
    assert!(text.contains("stale") || text.contains("6015"), "{text}");

    sync::set_test_chain(None);
    forge::storage::set_test_storage(None);
}

#[test]
fn corrupt_cas_blob_fails_clone() {
    let Some((tmp, chain, owner)) = setup() else {
        return;
    };
    sync::set_test_chain(Some(chain.clone()));
    let storage = tmp.path().join("cas-root");
    fs::create_dir_all(&storage).unwrap();
    forge::storage::set_test_storage(Some(storage.clone()));

    let origin = tmp.path().join("origin");
    fs::create_dir_all(&origin).unwrap();
    dispatch_in(
        &origin,
        Command::Init {
            dir: Some(".".into()),
        },
    )
    .unwrap();
    write_wallet(&origin, &owner);
    fs::write(origin.join("a.txt"), "a\n").unwrap();
    dispatch_in(
        &origin,
        Command::Add {
            paths: vec!["a.txt".into()],
        },
    )
    .unwrap();
    dispatch_in(
        &origin,
        Command::Commit {
            message: "a".into(),
        },
    )
    .unwrap();
    dispatch_in(
        &origin,
        Command::Push {
            remote: None,
            branch: None,
        },
    )
    .unwrap();

    for dir in ["cas", "cas-2"] {
        let cas = storage.join(dir);
        if cas.is_dir() {
            for entry in fs::read_dir(&cas).unwrap() {
                let path = entry.unwrap().path();
                if path.is_file() {
                    let mut bytes = fs::read(&path).unwrap();
                    if !bytes.is_empty() {
                        let last = bytes.len() - 1;
                        bytes[last] ^= 0xff;
                        fs::write(&path, bytes).unwrap();
                    }
                }
            }
        }
    }

    let config = forge::config::ForgeConfig::load(&origin).unwrap();
    let clone_dir = tmp.path().join("clone-bad");
    let err = dispatch_in(
        tmp.path(),
        Command::Clone {
            repo: config.repo_pda,
            dir: Some(clone_dir.display().to_string()),
        },
    )
    .unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("mismatch")
            || msg.contains("cid")
            || msg.contains("oid")
            || msg.contains("CAR"),
        "{msg}"
    );

    sync::set_test_chain(None);
    forge::storage::set_test_storage(None);
}

#[test]
fn verify_without_chain_is_local() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    dispatch_in(
        dir,
        Command::Init {
            dir: Some(".".into()),
        },
    )
    .unwrap();
    fs::write(dir.join("a.txt"), "a\n").unwrap();
    dispatch_in(
        dir,
        Command::Add {
            paths: vec!["a.txt".into()],
        },
    )
    .unwrap();
    dispatch_in(
        dir,
        Command::Commit {
            message: "a".into(),
        },
    )
    .unwrap();
    dispatch_in(dir, Command::Verify { commit: None }).unwrap();
    let _ = VerifyStatus::ClaimOnly;
}
