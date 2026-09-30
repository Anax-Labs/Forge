//! Phase 3 integration tests: onchain state model, PDAs, and the
//! `initialize_repository` / `create_branch` lifecycle.
//!
//! These tests execute the compiled program in `LiteSVM` against the SBF
//! artifact produced by `anchor build`. When the artifact is absent (for
//! example in the Rust-only CI job, which does not install the Solana
//! toolchain) the `LiteSVM` tests skip with a clear message; the anchor CI job
//! runs them after `anchor build`.
//!
//! Spec traceability: §4 (state/PDA), §5.6 (genesis history root), §9.1
//! (events), §9.2 (`initialize_repository`/`create_branch`), §7.2 (branch
//! creation), §16.4 (owner-only MVP auth).

use anchor_lang::{AccountDeserialize, AnchorDeserialize, Discriminator};
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_message::{AccountMeta, Address, Instruction, Message};
use solana_signer::Signer;
use solana_transaction::Transaction;

use forge_repository::constants::{BRANCH_SEED, PERMISSIONS_MODE_OWNER_ONLY, REPO_SEED};
use forge_repository::events::{BranchCreated, RepositoryInitialized};
use forge_repository::state::{BranchAccount, RepositoryAccount};

const SYSTEM_PROGRAM: &str = "11111111111111111111111111111111";

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

fn program_so() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/deploy/forge_repository.so")
}

fn program_id() -> Address {
    Address::new_from_array(forge_repository::id().to_bytes())
}

fn system_program() -> Address {
    SYSTEM_PROGRAM.parse().unwrap()
}

/// Returns a fresh VM with the program loaded, or `None` if the artifact is
/// missing (so the test can skip instead of failing spuriously).
fn setup() -> Option<(LiteSVM, Address, Keypair)> {
    let so = program_so();
    if !so.exists() {
        eprintln!(
            "skipping LiteSVM test: {} not found (run `anchor build` first)",
            so.display()
        );
        return None;
    }
    let mut svm = LiteSVM::new();
    let pid = program_id();
    svm.add_program_from_file(pid, &so).expect("load program");
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 100 * 1_000_000_000)
        .expect("airdrop");
    Some((svm, pid, payer))
}

/// Anchor's instruction discriminator: `sha256("global:<name>")[..8]`.
fn disc(name: &str) -> [u8; 8] {
    let digest = forge_object::HashAlgorithm::Sha256.digest(format!("global:{name}").as_bytes());
    let mut out = [0u8; 8];
    out.copy_from_slice(&digest[..8]);
    out
}

fn name(s: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    assert!(s.len() <= 32);
    out[..s.len()].copy_from_slice(s.as_bytes());
    out
}

fn event_authority(pid: &Address) -> Address {
    Address::find_program_address(&[b"__event_authority"], pid).0
}

fn initialize_instruction(
    pid: &Address,
    owner: &Address,
    repo_name: [u8; 32],
    default_branch: [u8; 32],
    storage_backend: u8,
    flags: u16,
) -> Instruction {
    let (repo, _) =
        Address::find_program_address(&[REPO_SEED, owner.as_ref(), repo_name.as_ref()], pid);
    let (branch, _) =
        Address::find_program_address(&[BRANCH_SEED, repo.as_ref(), default_branch.as_ref()], pid);
    let mut data = Vec::new();
    data.extend_from_slice(&disc("initialize_repository"));
    data.extend_from_slice(&repo_name);
    data.extend_from_slice(&default_branch);
    data.push(storage_backend);
    data.extend_from_slice(&flags.to_le_bytes());

    Instruction {
        program_id: *pid,
        accounts: vec![
            AccountMeta::new(*owner, true),
            AccountMeta::new(repo, false),
            AccountMeta::new(branch, false),
            AccountMeta::new_readonly(system_program(), false),
            AccountMeta::new_readonly(event_authority(pid), false),
            AccountMeta::new_readonly(*pid, false),
        ],
        data,
    }
}

fn create_branch_instruction(
    pid: &Address,
    signer: &Address,
    repository: Address,
    branch_name: [u8; 32],
    from_commit: [u8; 32],
    authority: Address,
) -> Instruction {
    let (branch, _) = Address::find_program_address(
        &[BRANCH_SEED, repository.as_ref(), branch_name.as_ref()],
        pid,
    );
    let mut data = Vec::new();
    data.extend_from_slice(&disc("create_branch"));
    data.extend_from_slice(&branch_name);
    data.extend_from_slice(&from_commit);
    data.extend_from_slice(authority.as_ref());

    Instruction {
        program_id: *pid,
        accounts: vec![
            AccountMeta::new(*signer, true),
            AccountMeta::new_readonly(repository, false),
            AccountMeta::new(branch, false),
            AccountMeta::new_readonly(system_program(), false),
            AccountMeta::new_readonly(event_authority(pid), false),
            AccountMeta::new_readonly(*pid, false),
        ],
        data,
    }
}

fn repo_address(pid: &Address, owner: &Address, repo_name: &[u8; 32]) -> Address {
    Address::find_program_address(&[REPO_SEED, owner.as_ref(), repo_name.as_ref()], pid).0
}

fn branch_address(pid: &Address, repository: &Address, branch_name: &[u8; 32]) -> Address {
    Address::find_program_address(
        &[BRANCH_SEED, repository.as_ref(), branch_name.as_ref()],
        pid,
    )
    .0
}

#[allow(clippy::result_large_err)]
fn send(
    svm: &mut LiteSVM,
    payer: &Keypair,
    instruction: Instruction,
) -> std::result::Result<
    litesvm::types::TransactionMetadata,
    litesvm::types::FailedTransactionMetadata,
> {
    let blockhash = svm.latest_blockhash();
    let tx = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&payer.pubkey()),
        &[payer],
        blockhash,
    );
    svm.send_transaction(tx)
}

/// Extracts the Anchor custom error code from a failed transaction result.
fn custom_error_code(err: &solana_transaction::TransactionError) -> Option<u32> {
    match err {
        solana_transaction::TransactionError::InstructionError(
            _,
            solana_transaction::InstructionError::Custom(code),
        ) => Some(*code),
        _ => None,
    }
}

/// Finds and decodes event-CPI payloads of type `T` (`emit_cpi!`, §9.1).
fn decode_events<T>(meta: &litesvm::types::TransactionMetadata) -> Vec<T>
where
    T: Discriminator + AnchorDeserialize,
{
    let mut out = Vec::new();
    for inner in meta.inner_instructions.iter().flatten() {
        let data = &inner.instruction.data;
        if data.len() < 16 || &data[..8] != anchor_lang::event::EVENT_IX_TAG_LE {
            continue;
        }
        if &data[8..16] != T::DISCRIMINATOR {
            continue;
        }
        let mut rest: &[u8] = &data[16..];
        if let Ok(event) = T::deserialize(&mut rest) {
            out.push(event);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Happy paths
// ---------------------------------------------------------------------------

#[test]
fn initialize_repository_creates_repo_and_default_branch() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo_name = name("forge");
    let default_branch = name("main");

    let ix = initialize_instruction(&pid, &owner.pubkey(), repo_name, default_branch, 2, 0);
    let meta = send(&mut svm, &owner, ix).expect("initialize should succeed");

    let repo_addr = repo_address(&pid, &owner.pubkey(), &repo_name);
    let branch_addr = branch_address(&pid, &repo_addr, &default_branch);

    // Repository account.
    let repo_acct = svm.get_account(&repo_addr).expect("repo account exists");
    assert_eq!(repo_acct.owner, pid);
    assert_eq!(repo_acct.data.len(), 8 + RepositoryAccount::LEN);
    let repo =
        RepositoryAccount::try_deserialize(&mut repo_acct.data.as_slice()).expect("deserialize");
    assert_eq!(repo.owner.to_bytes(), owner.pubkey().to_bytes());
    assert_eq!(repo.repo_id.to_bytes(), repo_addr.to_bytes());
    assert_eq!(repo.name, repo_name);
    assert_eq!(repo.default_branch, default_branch);
    assert_eq!(repo.commit_count, 0);
    assert_eq!(repo.contributor_count, 0);
    assert_eq!(repo.storage_backend, 2);
    assert_eq!(repo.flags, 0);

    // Canonical bump is stored (§11).
    let (_, expected_bump) = Address::find_program_address(
        &[REPO_SEED, owner.pubkey().as_ref(), repo_name.as_ref()],
        &pid,
    );
    assert_eq!(repo.bump, expected_bump);

    // §5.6 genesis history root matches the canonical engine.
    let expected_history = forge_object::history::genesis_history_root(
        &repo_addr.to_bytes(),
        forge_object::HashAlgorithm::Sha256,
    )
    .to_bytes32();
    assert_eq!(repo.history_root, expected_history);

    // Default branch account.
    let branch_acct = svm
        .get_account(&branch_addr)
        .expect("branch account exists");
    assert_eq!(branch_acct.data.len(), 8 + BranchAccount::LEN);
    let branch =
        BranchAccount::try_deserialize(&mut branch_acct.data.as_slice()).expect("deserialize");
    assert_eq!(branch.repo.to_bytes(), repo_addr.to_bytes());
    assert_eq!(branch.name, default_branch);
    assert_eq!(branch.head_commit, [0u8; 32]);
    assert_eq!(branch.head_seq, 0);
    assert_eq!(branch.authority.to_bytes(), owner.pubkey().to_bytes());
    assert_eq!(branch.permissions_mode, PERMISSIONS_MODE_OWNER_ONLY);

    // Event emitted via event-CPI (§9.1).
    let events: Vec<RepositoryInitialized> = decode_events(&meta);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].repository.to_bytes(), repo_addr.to_bytes());
    assert_eq!(events[0].history_root, expected_history);
}

#[test]
fn create_branch_creates_named_branch() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo_name = name("forge");
    let default_branch = name("main");
    let ix = initialize_instruction(&pid, &owner.pubkey(), repo_name, default_branch, 2, 0);
    send(&mut svm, &owner, ix).expect("initialize");
    let repo_addr = repo_address(&pid, &owner.pubkey(), &repo_name);

    let feature = name("feature/x");
    let ix = create_branch_instruction(
        &pid,
        &owner.pubkey(),
        repo_addr,
        feature,
        [0u8; 32],
        owner.pubkey(),
    );
    let meta = send(&mut svm, &owner, ix).expect("create_branch should succeed");

    let branch_addr = branch_address(&pid, &repo_addr, &feature);
    let acct = svm.get_account(&branch_addr).expect("branch exists");
    let branch = BranchAccount::try_deserialize(&mut acct.data.as_slice()).unwrap();
    assert_eq!(branch.name, feature);
    assert_eq!(branch.head_commit, [0u8; 32]);
    assert_eq!(branch.head_seq, 0);
    assert_eq!(branch.authority.to_bytes(), owner.pubkey().to_bytes());

    let events: Vec<BranchCreated> = decode_events(&meta);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].name, feature);
}

// ---------------------------------------------------------------------------
// Negative paths (§9.2 task 8, §11)
// ---------------------------------------------------------------------------

#[test]
fn initialize_rejects_empty_name() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let ix = initialize_instruction(&pid, &owner.pubkey(), [0u8; 32], name("main"), 2, 0);
    let err = send(&mut svm, &owner, ix).expect_err("empty name must fail");
    assert_eq!(
        custom_error_code(&err.err),
        Some(6000),
        "expected ForgeError::InvalidName (6000), got {:?}",
        err.err
    );
}

#[test]
fn initialize_rejects_unknown_storage_backend() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let ix = initialize_instruction(&pid, &owner.pubkey(), name("forge"), name("main"), 9, 0);
    let err = send(&mut svm, &owner, ix).expect_err("bad backend must fail");
    assert_eq!(
        custom_error_code(&err.err),
        Some(6006),
        "expected ForgeError::InvalidStorageBackend (6006), got {:?}",
        err.err
    );
}

#[test]
fn initialize_rejects_duplicate_repository() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let ix = initialize_instruction(&pid, &owner.pubkey(), name("forge"), name("main"), 2, 0);
    send(&mut svm, &owner, ix).expect("first init");

    let ix = initialize_instruction(&pid, &owner.pubkey(), name("forge"), name("main"), 2, 0);
    let result = send(&mut svm, &owner, ix);
    assert!(result.is_err(), "second init at same PDA must fail");
}

#[test]
fn create_branch_rejects_non_owner() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let ix = initialize_instruction(&pid, &owner.pubkey(), name("forge"), name("main"), 2, 0);
    send(&mut svm, &owner, ix).expect("initialize");
    let repo_addr = repo_address(&pid, &owner.pubkey(), &name("forge"));

    let mallory = Keypair::new();
    svm.airdrop(&mallory.pubkey(), 10 * 1_000_000_000).unwrap();
    let ix = create_branch_instruction(
        &pid,
        &mallory.pubkey(),
        repo_addr,
        name("steal"),
        [0u8; 32],
        mallory.pubkey(),
    );
    let err = send(&mut svm, &mallory, ix).expect_err("non-owner must be rejected");
    assert_eq!(
        custom_error_code(&err.err),
        Some(6001),
        "expected ForgeError::Unauthorized (6001), got {:?}",
        err.err
    );
}

#[test]
fn create_branch_rejects_duplicate_name() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let ix = initialize_instruction(&pid, &owner.pubkey(), name("forge"), name("main"), 2, 0);
    send(&mut svm, &owner, ix).expect("initialize");
    let repo_addr = repo_address(&pid, &owner.pubkey(), &name("forge"));

    // The default branch already occupies ("branch", repo, "main").
    let ix = create_branch_instruction(
        &pid,
        &owner.pubkey(),
        repo_addr,
        name("main"),
        [0u8; 32],
        owner.pubkey(),
    );
    assert!(
        send(&mut svm, &owner, ix).is_err(),
        "duplicate branch PDA must fail to init"
    );
}

#[test]
fn create_branch_requires_commit_account_when_from_commit_nonzero() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let ix = initialize_instruction(&pid, &owner.pubkey(), name("forge"), name("main"), 2, 0);
    send(&mut svm, &owner, ix).expect("initialize");
    let repo_addr = repo_address(&pid, &owner.pubkey(), &name("forge"));

    // Non-zero from_commit with no remaining account supplied.
    let ix = create_branch_instruction(
        &pid,
        &owner.pubkey(),
        repo_addr,
        name("dev"),
        [9u8; 32],
        owner.pubkey(),
    );
    let err = send(&mut svm, &owner, ix).expect_err("missing commit account must fail");
    assert_eq!(
        custom_error_code(&err.err),
        Some(6002),
        "expected ForgeError::UnknownCommit (6002), got {:?}",
        err.err
    );
}

#[test]
fn create_branch_rejects_wrong_commit_account() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let ix = initialize_instruction(&pid, &owner.pubkey(), name("forge"), name("main"), 2, 0);
    send(&mut svm, &owner, ix).expect("initialize");
    let repo_addr = repo_address(&pid, &owner.pubkey(), &name("forge"));

    // Non-zero from_commit with an account that is not the canonical commit PDA.
    let mut ix = create_branch_instruction(
        &pid,
        &owner.pubkey(),
        repo_addr,
        name("dev"),
        [9u8; 32],
        owner.pubkey(),
    );
    ix.accounts.push(AccountMeta::new_readonly(
        Address::new_from_array([42u8; 32]),
        false,
    ));
    let err = send(&mut svm, &owner, ix).expect_err("wrong commit PDA must fail");
    assert_eq!(
        custom_error_code(&err.err),
        Some(6004),
        "expected ForgeError::InvalidPda (6004), got {:?}",
        err.err
    );
}

// ---------------------------------------------------------------------------
// Cross-check: account sizes are the frozen §4 layouts
// ---------------------------------------------------------------------------

#[test]
fn account_length_constants_match_declared_field_sums() {
    assert_eq!(RepositoryAccount::LEN, 248);
    assert_eq!(BranchAccount::LEN, 179);
    assert_eq!(
        forge_repository::state::CommitAccount::LEN,
        274,
        "CommitAccount layout is frozen in Phase 3"
    );
    assert_eq!(forge_repository::state::TagAccount::LEN, 170);
    assert_eq!(forge_repository::state::PermissionAccount::LEN, 82);
    assert_eq!(forge_repository::state::ProgramSourceAttestation::LEN, 202);
}

// Keep `Message` imported for its `Address`/`Instruction` re-exports; silence
// the unused-import lint when the LiteSVM tests are compiled but not run.
#[allow(dead_code)]
fn _assert_message_reexports() {
    let _ = std::mem::size_of::<Message>();
}
