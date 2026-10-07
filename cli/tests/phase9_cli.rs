//! Phase 9 CLI integration tests: permissions, tag, and verify-program.

#![allow(clippy::similar_names)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use forge::chain::{self, Chain};
use forge::commands;
use forge::config::ForgeConfig;
use forge::sync;
use forge::{CliExit, Command, PermissionsAction, VerifyStatus};
use solana_keypair::Keypair;
use solana_signer::Signer;

static CWD: Mutex<()> = Mutex::new(());

fn setup() -> Option<(tempfile::TempDir, Chain, Keypair)> {
    let so = chain::default_program_so();
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

fn init_and_push(tmp: &Path, chain: &Chain, owner: &Keypair) -> PathBuf {
    sync::set_test_chain(Some(chain.clone()));
    let storage = tmp.join("cas-root");
    fs::create_dir_all(&storage).unwrap();
    forge::storage::set_test_storage(Some(storage));

    let origin = tmp.join("origin");
    fs::create_dir_all(&origin).unwrap();
    dispatch_in(
        &origin,
        Command::Init {
            dir: Some(".".into()),
        },
    )
    .unwrap();
    write_wallet(&origin, owner);
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
    origin
}

fn repo_pda(origin: &Path) -> solana_message::Address {
    let config = ForgeConfig::load(origin).unwrap();
    chain::parse_address(&config.repo_pda).unwrap()
}

#[test]
fn permissions_set_and_get() {
    let Some((tmp, chain, owner)) = setup() else {
        return;
    };
    let origin = init_and_push(tmp.path(), &chain, &owner);
    let bob = Keypair::new();
    let bob_s = bob.pubkey().to_string();

    dispatch_in(
        &origin,
        Command::Permissions {
            action: PermissionsAction::Set {
                contributor: bob_s.clone(),
                role: "writer".into(),
                expires_slot: 0,
            },
        },
    )
    .unwrap();
    dispatch_in(
        &origin,
        Command::Permissions {
            action: PermissionsAction::Get { contributor: bob_s },
        },
    )
    .unwrap();

    let pid = chain.program_id();
    let repo = repo_pda(&origin);
    let perm = chain::permission_pda(&pid, &repo, &bob.pubkey());
    let state = chain.fetch_permission(&perm).unwrap().unwrap();
    assert_eq!(state.role, chain::ROLE_WRITER);
}

#[test]
fn tag_creates_signed_onchain_tag_and_checkpoint() {
    let Some((tmp, chain, owner)) = setup() else {
        return;
    };
    let origin = init_and_push(tmp.path(), &chain, &owner);
    dispatch_in(
        &origin,
        Command::Tag {
            name: "v1.0.0".into(),
            checkpoint: true,
        },
    )
    .unwrap();

    let pid = chain.program_id();
    let repo = repo_pda(&origin);
    let tag_addr = chain::tag_pda(&pid, &repo, &chain::pad_name("v1.0.0").unwrap());
    let tag = chain.fetch_tag(&tag_addr).unwrap().unwrap();
    assert_eq!(tag.signed, 1);
    assert!(origin.join(".forge/checkpoints/v1.0.0.json").exists());
}

#[test]
fn verify_program_without_claim_is_missing_data() {
    let Some((tmp, chain, owner)) = setup() else {
        return;
    };
    let origin = init_and_push(tmp.path(), &chain, &owner);
    let unknown = Keypair::new().pubkey().to_string();
    let err = dispatch_in(
        &origin,
        Command::VerifyProgram {
            program_id: unknown,
        },
    )
    .unwrap_err();
    let exit = err.downcast_ref::<CliExit>().expect("CliExit");
    assert_eq!(exit.code, VerifyStatus::MissingData as u8);
}
