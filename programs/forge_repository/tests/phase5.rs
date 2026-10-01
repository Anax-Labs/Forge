//! Phase 5 integration tests: branch advancement, merge, reset, and delete.
//!
//! Spec traceability: §6.6 (merge), §6.8 (signed update / replay), §7.2 (branch
//! ops), §7.3 (non-fast-forward reset), §7.4 (optimistic-concurrency race),
//! §9.4 (Ed25519 introspection), §11.
//!
//! Commit accounts are fabricated with `set_account` (their production comes
//! from `create_commit`, already covered by Phase 4) so these tests isolate
//! branch-ref logic.

use anchor_lang::prelude::Pubkey;
use anchor_lang::{AccountDeserialize, AccountSerialize, AnchorDeserialize, Discriminator};
use forge_object::{branch, HashAlgorithm, Oid};
use forge_repository::constants::{BRANCH_SEED, COMMIT_SEED, REPO_SEED};
use forge_repository::events::{BranchDeleted, BranchReset, BranchUpdated};
use forge_repository::state::{BranchAccount, CommitAccount, RepositoryAccount};
use litesvm::LiteSVM;
use solana_account::Account;
use solana_keypair::Keypair;
use solana_message::{AccountMeta, Address, Instruction, Message};
use solana_signer::Signer;
use solana_transaction::Transaction;

const SYSTEM_PROGRAM: &str = "11111111111111111111111111111111";
const ED25519_PROGRAM: &str = "Ed25519SigVerify111111111111111111111111111";
const INSTRUCTIONS_SYSVAR: &str = "Sysvar1nstructions1111111111111111111111111";
const NATIVE_LOADER: &str = "NativeLoader1111111111111111111111111111111";

fn program_so() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/deploy/forge_repository.so")
}

fn program_id() -> Address {
    Address::new_from_array(forge_repository::id().to_bytes())
}

fn system_program() -> Address {
    SYSTEM_PROGRAM.parse().unwrap()
}

fn ed25519_program() -> Address {
    ED25519_PROGRAM.parse().unwrap()
}

fn instructions_sysvar() -> Address {
    INSTRUCTIONS_SYSVAR.parse().unwrap()
}

fn event_authority(pid: &Address) -> Address {
    Address::find_program_address(&[b"__event_authority"], pid).0
}

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
    svm.set_account(
        ed25519_program(),
        Account {
            lamports: 1,
            data: Vec::new(),
            owner: NATIVE_LOADER.parse().unwrap(),
            executable: true,
            rent_epoch: 0,
        },
    )
    .expect("load ed25519 precompile");
    let pid = program_id();
    svm.add_program_from_file(pid, &so).expect("load program");
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 100 * 1_000_000_000)
        .expect("airdrop");
    Some((svm, pid, payer))
}

fn disc(name: &str) -> [u8; 8] {
    let digest = HashAlgorithm::Sha256.digest(format!("global:{name}").as_bytes());
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

fn repo_address(pid: &Address, owner: &Address, repo_name: &[u8; 32]) -> Address {
    Address::find_program_address(&[REPO_SEED, owner.as_ref(), repo_name.as_ref()], pid).0
}

fn branch_address(pid: &Address, repo: &Address, branch_name: &[u8; 32]) -> Address {
    Address::find_program_address(&[BRANCH_SEED, repo.as_ref(), branch_name.as_ref()], pid).0
}

fn commit_address(pid: &Address, repo: &Address, oid: &[u8; 32]) -> Address {
    Address::find_program_address(&[COMMIT_SEED, repo.as_ref(), oid.as_ref()], pid).0
}

// ---------------------------------------------------------------------------
// Instruction builders
// ---------------------------------------------------------------------------

fn initialize_instruction(
    pid: &Address,
    owner: &Address,
    repo_name: [u8; 32],
    default_branch: [u8; 32],
) -> Instruction {
    let repo = repo_address(pid, owner, &repo_name);
    let branch = branch_address(pid, &repo, &default_branch);
    let mut data = Vec::new();
    data.extend_from_slice(&disc("initialize_repository"));
    data.extend_from_slice(&repo_name);
    data.extend_from_slice(&default_branch);
    data.push(2); // hybrid storage
    data.extend_from_slice(&0u16.to_le_bytes());
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
    authority: &Address,
    repo: Address,
    branch_name: [u8; 32],
) -> Instruction {
    let branch = branch_address(pid, &repo, &branch_name);
    let mut data = Vec::new();
    data.extend_from_slice(&disc("create_branch"));
    data.extend_from_slice(&branch_name);
    data.extend_from_slice(&[0u8; 32]); // from_commit
    Instruction {
        program_id: *pid,
        accounts: vec![
            AccountMeta::new(*authority, true),
            AccountMeta::new_readonly(repo, false),
            AccountMeta::new(branch, false),
            AccountMeta::new_readonly(system_program(), false),
            AccountMeta::new_readonly(event_authority(pid), false),
            AccountMeta::new_readonly(*pid, false),
        ],
        data,
    }
}

#[allow(clippy::too_many_arguments)]
fn update_branch_instruction(
    pid: &Address,
    authority: &Address,
    repo: Address,
    branch_name: [u8; 32],
    new_head: [u8; 32],
    expected_seq: u64,
    new_commit: Address,
) -> Instruction {
    let branch = branch_address(pid, &repo, &branch_name);
    let mut data = Vec::new();
    data.extend_from_slice(&disc("update_branch"));
    data.extend_from_slice(&new_head);
    data.extend_from_slice(&expected_seq.to_le_bytes());
    Instruction {
        program_id: *pid,
        accounts: vec![
            AccountMeta::new(*authority, true),
            AccountMeta::new_readonly(repo, false),
            AccountMeta::new(branch, false),
            AccountMeta::new_readonly(new_commit, false),
            AccountMeta::new_readonly(instructions_sysvar(), false),
            AccountMeta::new_readonly(ed25519_program(), false),
            AccountMeta::new_readonly(event_authority(pid), false),
            AccountMeta::new_readonly(*pid, false),
        ],
        data,
    }
}

#[allow(clippy::too_many_arguments)]
fn reset_branch_instruction(
    pid: &Address,
    authority: &Address,
    repo: Address,
    branch_name: [u8; 32],
    new_head: [u8; 32],
    expected_seq: u64,
    new_commit: Address,
) -> Instruction {
    let branch = branch_address(pid, &repo, &branch_name);
    let mut data = Vec::new();
    data.extend_from_slice(&disc("reset_branch"));
    data.extend_from_slice(&new_head);
    data.extend_from_slice(&expected_seq.to_le_bytes());
    Instruction {
        program_id: *pid,
        accounts: vec![
            AccountMeta::new(*authority, true),
            AccountMeta::new_readonly(repo, false),
            AccountMeta::new(branch, false),
            AccountMeta::new_readonly(new_commit, false),
            AccountMeta::new_readonly(instructions_sysvar(), false),
            AccountMeta::new_readonly(ed25519_program(), false),
            AccountMeta::new_readonly(event_authority(pid), false),
            AccountMeta::new_readonly(*pid, false),
        ],
        data,
    }
}

fn delete_branch_instruction(
    pid: &Address,
    authority: &Address,
    repo: Address,
    branch_name: [u8; 32],
) -> Instruction {
    let branch = branch_address(pid, &repo, &branch_name);
    let mut data = Vec::new();
    data.extend_from_slice(&disc("delete_branch"));
    Instruction {
        program_id: *pid,
        accounts: vec![
            AccountMeta::new(*authority, true),
            AccountMeta::new_readonly(repo, false),
            AccountMeta::new(branch, false),
            AccountMeta::new_readonly(event_authority(pid), false),
            AccountMeta::new_readonly(*pid, false),
        ],
        data,
    }
}

#[allow(clippy::too_many_arguments)]
fn merge_instruction(
    pid: &Address,
    authority: &Address,
    repo: Address,
    target_name: [u8; 32],
    source_name: [u8; 32],
    merge_commit_oid: [u8; 32],
    expected_target_seq: u64,
    merge_commit_addr: Address,
) -> Instruction {
    let target = branch_address(pid, &repo, &target_name);
    let source = branch_address(pid, &repo, &source_name);
    let mut data = Vec::new();
    data.extend_from_slice(&disc("merge"));
    data.extend_from_slice(&merge_commit_oid);
    data.extend_from_slice(&expected_target_seq.to_le_bytes());
    Instruction {
        program_id: *pid,
        accounts: vec![
            AccountMeta::new(*authority, true),
            AccountMeta::new_readonly(repo, false),
            AccountMeta::new(target, false),
            AccountMeta::new_readonly(source, false),
            AccountMeta::new_readonly(merge_commit_addr, false),
            AccountMeta::new_readonly(instructions_sysvar(), false),
            AccountMeta::new_readonly(ed25519_program(), false),
            AccountMeta::new_readonly(event_authority(pid), false),
            AccountMeta::new_readonly(*pid, false),
        ],
        data,
    }
}

#[allow(clippy::cast_possible_truncation)] // Ed25519 offsets are tiny
fn ed25519_verify_instruction(
    message: &[u8],
    signature: &[u8; 64],
    pubkey: &[u8; 32],
) -> Instruction {
    const DATA_START: usize = 16;
    let mut data = Vec::new();
    data.push(1);
    data.push(0);
    data.extend_from_slice(&(DATA_START as u16).to_le_bytes());
    data.extend_from_slice(&u16::MAX.to_le_bytes());
    data.extend_from_slice(&((DATA_START + 64) as u16).to_le_bytes());
    data.extend_from_slice(&u16::MAX.to_le_bytes());
    data.extend_from_slice(&((DATA_START + 64 + 32) as u16).to_le_bytes());
    data.extend_from_slice(&(message.len() as u16).to_le_bytes());
    data.extend_from_slice(&u16::MAX.to_le_bytes());
    data.extend_from_slice(signature);
    data.extend_from_slice(pubkey);
    data.extend_from_slice(message);
    Instruction {
        program_id: ed25519_program(),
        accounts: vec![],
        data,
    }
}

fn ed25519_ix(message: &[u8], signer: &Keypair) -> Instruction {
    let sig: [u8; 64] = signer
        .sign_message(message)
        .as_ref()
        .try_into()
        .expect("signature length");
    ed25519_verify_instruction(message, &sig, &signer.pubkey().to_bytes())
}

fn branch_oid(oid: [u8; 32]) -> Oid {
    Oid::new(HashAlgorithm::Sha256, oid.to_vec()).expect("oid")
}

// ---------------------------------------------------------------------------
// State helpers
// ---------------------------------------------------------------------------

fn init_repo(svm: &mut LiteSVM, pid: &Address, owner: &Keypair) -> Address {
    let repo_name = name("forge");
    let ix = initialize_instruction(pid, &owner.pubkey(), repo_name, name("main"));
    send(svm, owner, &[ix]).expect("initialize");
    repo_address(pid, &owner.pubkey(), &repo_name)
}

fn add_branch(
    svm: &mut LiteSVM,
    pid: &Address,
    owner: &Keypair,
    repo: Address,
    branch_name: [u8; 32],
) {
    let ix = create_branch_instruction(pid, &owner.pubkey(), repo, branch_name);
    send(svm, owner, &[ix]).expect("create_branch");
}

#[allow(clippy::too_many_arguments, clippy::similar_names)]
fn install_commit(
    svm: &mut LiteSVM,
    pid: &Address,
    template: &Address,
    repo: Address,
    oid: [u8; 32],
    parent_count: u8,
    parent_a: [u8; 32],
    parent_b: [u8; 32],
) -> Address {
    let addr = commit_address(pid, &repo, &oid);
    let commit = CommitAccount {
        repo: Pubkey::new_from_array(repo.to_bytes()),
        commit_oid: oid,
        parent_count,
        parent_a,
        parent_b,
        tree_oid: [0x11; 32],
        author: Pubkey::new_from_array([0x22; 32]),
        authored_at: 0,
        message_hash: [0u8; 32],
        attestation_hash: [0u8; 32],
        seq: 0,
        bump: 0,
    };
    let mut data = Vec::new();
    commit.try_serialize(&mut data).expect("serialize commit");
    let mut acct = svm.get_account(template).expect("template account");
    acct.lamports = 10_000_000;
    acct.data = data;
    acct.owner = *pid;
    acct.executable = false;
    acct.rent_epoch = 0;
    svm.set_account(addr, acct).expect("install commit");
    addr
}

fn branch_state(
    svm: &LiteSVM,
    pid: &Address,
    repo: &Address,
    branch_name: &[u8; 32],
) -> BranchAccount {
    let addr = branch_address(pid, repo, branch_name);
    let acct = svm.get_account(&addr).expect("branch account");
    BranchAccount::try_deserialize(&mut acct.data.as_slice()).expect("deserialize branch")
}

fn repo_state(svm: &LiteSVM, repo: &Address) -> RepositoryAccount {
    let acct = svm.get_account(repo).expect("repo account");
    RepositoryAccount::try_deserialize(&mut acct.data.as_slice()).expect("deserialize repo")
}

// ---------------------------------------------------------------------------
// Transaction plumbing
// ---------------------------------------------------------------------------

#[allow(clippy::result_large_err)]
fn send(
    svm: &mut LiteSVM,
    payer: &Keypair,
    instructions: &[Instruction],
) -> std::result::Result<
    litesvm::types::TransactionMetadata,
    litesvm::types::FailedTransactionMetadata,
> {
    svm.expire_blockhash();
    let blockhash = svm.latest_blockhash();
    let tx = Transaction::new_signed_with_payer(
        instructions,
        Some(&payer.pubkey()),
        &[payer],
        blockhash,
    );
    svm.send_transaction(tx)
}

fn custom_error_code(err: &solana_transaction::TransactionError) -> Option<u32> {
    match err {
        solana_transaction::TransactionError::InstructionError(
            _,
            solana_transaction::InstructionError::Custom(code),
        ) => Some(*code),
        _ => None,
    }
}

fn assert_custom(code: u32, err: &solana_transaction::TransactionError) {
    assert_eq!(
        custom_error_code(err),
        Some(code),
        "expected error {code}, got {err:?}"
    );
}

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
// Helpers that pair an authorization signature with the instruction
// ---------------------------------------------------------------------------

fn update_pair(
    pid: &Address,
    signer: &Keypair,
    repo: Address,
    branch_name: [u8; 32],
    new_head: [u8; 32],
    expected_seq: u64,
    new_commit: Address,
) -> [Instruction; 2] {
    let message = branch::branch_update_message(
        &repo.to_bytes(),
        &branch_name,
        &branch_oid(new_head),
        expected_seq,
    );
    let ed = ed25519_ix(&message, signer);
    let ix = update_branch_instruction(
        pid,
        &signer.pubkey(),
        repo,
        branch_name,
        new_head,
        expected_seq,
        new_commit,
    );
    [ed, ix]
}

fn reset_pair(
    pid: &Address,
    signer: &Keypair,
    repo: Address,
    branch_name: [u8; 32],
    new_head: [u8; 32],
    expected_seq: u64,
    new_commit: Address,
) -> [Instruction; 2] {
    let message = branch::branch_reset_message(
        &repo.to_bytes(),
        &branch_name,
        &branch_oid(new_head),
        expected_seq,
    );
    let ed = ed25519_ix(&message, signer);
    let ix = reset_branch_instruction(
        pid,
        &signer.pubkey(),
        repo,
        branch_name,
        new_head,
        expected_seq,
        new_commit,
    );
    [ed, ix]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn update_branch_first_set_and_fast_forward() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    let template = owner.pubkey();

    // Empty branch -> root commit C1.
    let c1 = [0xC1u8; 32];
    let c1_addr = install_commit(&mut svm, &pid, &template, repo, c1, 0, [0u8; 32], [0u8; 32]);
    let meta = send(
        &mut svm,
        &owner,
        &update_pair(&pid, &owner, repo, name("main"), c1, 0, c1_addr),
    )
    .expect("first set");
    let events: Vec<BranchUpdated> = decode_events(&meta);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].new_head, c1);
    assert_eq!(events[0].head_seq, 1);
    let branch = branch_state(&svm, &pid, &repo, &name("main"));
    assert_eq!(branch.head_commit, c1);
    assert_eq!(branch.head_seq, 1);

    // Fast-forward C1 -> C2 (C2 first parent is C1).
    let c2 = [0xC2u8; 32];
    let c2_addr = install_commit(&mut svm, &pid, &template, repo, c2, 1, c1, [0u8; 32]);
    send(
        &mut svm,
        &owner,
        &update_pair(&pid, &owner, repo, name("main"), c2, 1, c2_addr),
    )
    .expect("fast-forward");
    let branch = branch_state(&svm, &pid, &repo, &name("main"));
    assert_eq!(branch.head_commit, c2);
    assert_eq!(branch.head_seq, 2);
}

#[test]
fn update_branch_rejects_stale_head_and_replay() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    let template = owner.pubkey();
    let c1 = [0xC1u8; 32];
    let c1_addr = install_commit(&mut svm, &pid, &template, repo, c1, 0, [0u8; 32], [0u8; 32]);

    let pair = update_pair(&pid, &owner, repo, name("main"), c1, 0, c1_addr);
    send(&mut svm, &owner, &pair).expect("first set");

    // Stale: expected_seq still 0 while the branch is at 1.
    let stale = update_pair(&pid, &owner, repo, name("main"), c1, 0, c1_addr);
    let err = send(&mut svm, &owner, &stale).expect_err("stale head");
    assert_custom(6015, &err.err); // StaleBranchHead
}

#[test]
fn update_branch_rejects_non_fast_forward() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    let template = owner.pubkey();
    let c1 = [0xC1u8; 32];
    let c1_addr = install_commit(&mut svm, &pid, &template, repo, c1, 0, [0u8; 32], [0u8; 32]);
    send(
        &mut svm,
        &owner,
        &update_pair(&pid, &owner, repo, name("main"), c1, 0, c1_addr),
    )
    .expect("first set");

    // C3 is unrelated to C1 (root), so the update is not a fast-forward.
    let c3 = [0xC3u8; 32];
    let c3_addr = install_commit(&mut svm, &pid, &template, repo, c3, 0, [0u8; 32], [0u8; 32]);
    let err = send(
        &mut svm,
        &owner,
        &update_pair(&pid, &owner, repo, name("main"), c3, 1, c3_addr),
    )
    .expect_err("non-fast-forward");
    assert_custom(6016, &err.err); // NonFastForward
}

#[test]
fn update_branch_rejects_unauthorized_author() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    let template = owner.pubkey();
    let c1 = [0xC1u8; 32];
    let c1_addr = install_commit(&mut svm, &pid, &template, repo, c1, 0, [0u8; 32], [0u8; 32]);

    let mallory = Keypair::new();
    svm.airdrop(&mallory.pubkey(), 10_000_000_000)
        .expect("airdrop");
    let err = send(
        &mut svm,
        &mallory,
        &update_pair(&pid, &mallory, repo, name("main"), c1, 0, c1_addr),
    )
    .expect_err("non-owner");
    assert_custom(6001, &err.err); // Unauthorized
}

#[test]
fn update_branch_rejects_forged_signature() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    let template = owner.pubkey();
    let c1 = [0xC1u8; 32];
    let c1_addr = install_commit(&mut svm, &pid, &template, repo, c1, 0, [0u8; 32], [0u8; 32]);

    let mallory = Keypair::new();
    // Sign the correct message with the wrong key, claiming the owner as authority.
    let message =
        branch::branch_update_message(&repo.to_bytes(), &name("main"), &branch_oid(c1), 0);
    let ed = ed25519_ix(&message, &mallory);
    let ix = update_branch_instruction(&pid, &owner.pubkey(), repo, name("main"), c1, 0, c1_addr);
    let err = send(&mut svm, &owner, &[ed, ix]).expect_err("forged signature");
    assert_custom(6012, &err.err); // BadSignature
}

#[test]
fn reset_branch_allows_non_fast_forward_and_preserves_history() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    let template = owner.pubkey();
    let genesis = repo_state(&svm, &repo).history_root;

    let c1 = [0xC1u8; 32];
    let c1_addr = install_commit(&mut svm, &pid, &template, repo, c1, 0, [0u8; 32], [0u8; 32]);
    send(
        &mut svm,
        &owner,
        &update_pair(&pid, &owner, repo, name("main"), c1, 0, c1_addr),
    )
    .expect("first set");

    // Unrelated root commit X.
    let x = [0xDDu8; 32];
    let x_addr = install_commit(&mut svm, &pid, &template, repo, x, 0, [0u8; 32], [0u8; 32]);
    let meta = send(
        &mut svm,
        &owner,
        &reset_pair(&pid, &owner, repo, name("main"), x, 1, x_addr),
    )
    .expect("reset");
    let events: Vec<BranchReset> = decode_events(&meta);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].old_head, c1);
    assert_eq!(events[0].new_head, x);

    let branch = branch_state(&svm, &pid, &repo, &name("main"));
    assert_eq!(branch.head_commit, x);
    assert_eq!(branch.head_seq, 2);
    // The append-only repository history root is untouched by a reset (§7.3).
    assert_eq!(repo_state(&svm, &repo).history_root, genesis);
}

#[test]
fn reset_branch_rejects_unauthorized_author() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    let template = owner.pubkey();
    let x = [0xDDu8; 32];
    let x_addr = install_commit(&mut svm, &pid, &template, repo, x, 0, [0u8; 32], [0u8; 32]);
    let mallory = Keypair::new();
    svm.airdrop(&mallory.pubkey(), 10_000_000_000)
        .expect("airdrop");
    let err = send(
        &mut svm,
        &mallory,
        &reset_pair(&pid, &mallory, repo, name("main"), x, 0, x_addr),
    )
    .expect_err("non-owner reset");
    assert_custom(6001, &err.err);
}

#[test]
fn merge_advances_target_to_two_parent_commit() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    let template = owner.pubkey();
    add_branch(&mut svm, &pid, &owner, repo, name("feature"));

    let a = [0xA1u8; 32];
    let a_addr = install_commit(&mut svm, &pid, &template, repo, a, 0, [0u8; 32], [0u8; 32]);
    send(
        &mut svm,
        &owner,
        &update_pair(&pid, &owner, repo, name("main"), a, 0, a_addr),
    )
    .expect("main -> A");

    let b = [0xB1u8; 32];
    let b_addr = install_commit(&mut svm, &pid, &template, repo, b, 0, [0u8; 32], [0u8; 32]);
    send(
        &mut svm,
        &owner,
        &update_pair(&pid, &owner, repo, name("feature"), b, 0, b_addr),
    )
    .expect("feature -> B");

    let m = [0x12u8; 32];
    let m_addr = install_commit(&mut svm, &pid, &template, repo, m, 2, a, b);
    let message = branch::branch_update_message(&repo.to_bytes(), &name("main"), &branch_oid(m), 1);
    let ed = ed25519_ix(&message, &owner);
    let ix = merge_instruction(
        &pid,
        &owner.pubkey(),
        repo,
        name("main"),
        name("feature"),
        m,
        1,
        m_addr,
    );
    let meta = send(&mut svm, &owner, &[ed, ix]).expect("merge");
    let events: Vec<BranchUpdated> = decode_events(&meta);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].new_head, m);
    let target = branch_state(&svm, &pid, &repo, &name("main"));
    assert_eq!(target.head_commit, m);
    assert_eq!(target.head_seq, 2);
}

#[test]
fn merge_rejects_mismatched_parents() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    let template = owner.pubkey();
    add_branch(&mut svm, &pid, &owner, repo, name("feature"));

    let a = [0xA1u8; 32];
    let a_addr = install_commit(&mut svm, &pid, &template, repo, a, 0, [0u8; 32], [0u8; 32]);
    send(
        &mut svm,
        &owner,
        &update_pair(&pid, &owner, repo, name("main"), a, 0, a_addr),
    )
    .expect("main -> A");
    let b = [0xB1u8; 32];
    let b_addr = install_commit(&mut svm, &pid, &template, repo, b, 0, [0u8; 32], [0u8; 32]);
    send(
        &mut svm,
        &owner,
        &update_pair(&pid, &owner, repo, name("feature"), b, 0, b_addr),
    )
    .expect("feature -> B");

    // Merge commit's second parent is Z, not the source head B.
    let z = [0xEEu8; 32];
    let m = [0x12u8; 32];
    let m_addr = install_commit(&mut svm, &pid, &template, repo, m, 2, a, z);
    let message = branch::branch_update_message(&repo.to_bytes(), &name("main"), &branch_oid(m), 1);
    let ed = ed25519_ix(&message, &owner);
    let ix = merge_instruction(
        &pid,
        &owner.pubkey(),
        repo,
        name("main"),
        name("feature"),
        m,
        1,
        m_addr,
    );
    let err = send(&mut svm, &owner, &[ed, ix]).expect_err("bad merge parents");
    assert_custom(6017, &err.err); // InvalidMerge
}

#[test]
fn delete_branch_removes_and_protects_default() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    add_branch(&mut svm, &pid, &owner, repo, name("dev"));

    // Deleting the default branch is forbidden.
    let err = send(
        &mut svm,
        &owner,
        &[delete_branch_instruction(
            &pid,
            &owner.pubkey(),
            repo,
            name("main"),
        )],
    )
    .expect_err("cannot delete default");
    assert_custom(6018, &err.err); // CannotDeleteDefaultBranch

    // Deleting a normal branch succeeds and closes the account.
    let meta = send(
        &mut svm,
        &owner,
        &[delete_branch_instruction(
            &pid,
            &owner.pubkey(),
            repo,
            name("dev"),
        )],
    )
    .expect("delete dev");
    let events: Vec<BranchDeleted> = decode_events(&meta);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].name, name("dev"));

    let addr = branch_address(&pid, &repo, &name("dev"));
    let closed = svm
        .get_account(&addr)
        .is_none_or(|a| a.data.is_empty() || a.lamports == 0);
    assert!(closed, "deleted branch account must be closed");
}

#[allow(dead_code)]
fn _message_import() {
    let _ = std::mem::size_of::<Message>();
}
