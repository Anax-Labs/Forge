//! Phase 4 integration tests: `create_commit` and Ed25519 attestation verification.
//!
//! Spec traceability: §5.5 (attestation hash), §5.6 (history append), §6.4
//! (parents), §9.2 (`create_commit`), §9.4 (Ed25519 introspection), §11.

use anchor_lang::{AccountDeserialize, AnchorDeserialize, Discriminator};
use forge_object::{Attestation, HashAlgorithm, Oid};
use forge_repository::constants::{COMMIT_SEED, REPO_SEED};
use forge_repository::events::CommitCreated;
use forge_repository::state::{CommitAccount, RepositoryAccount};
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
    // Ed25519 is a precompile. LiteSVM only installs those accounts when the
    // optional `precompiles` crate feature is on (pulls OpenSSL). Register the
    // program account ourselves so `invoke_context.is_precompile` can run it.
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

fn repo_address(pid: &Address, owner: &Address, repo_name: &[u8; 32]) -> Address {
    Address::find_program_address(&[REPO_SEED, owner.as_ref(), repo_name.as_ref()], pid).0
}

fn commit_address(pid: &Address, repo: &Address, commit_oid: &[u8; 32]) -> Address {
    Address::find_program_address(&[COMMIT_SEED, repo.as_ref(), commit_oid.as_ref()], pid).0
}

fn initialize_instruction(
    pid: &Address,
    owner: &Address,
    repo_name: [u8; 32],
    default_branch: [u8; 32],
) -> Instruction {
    let (repo, _) =
        Address::find_program_address(&[REPO_SEED, owner.as_ref(), repo_name.as_ref()], pid);
    let default_branch_name = default_branch;
    let (branch, _) = Address::find_program_address(
        &[
            forge_repository::constants::BRANCH_SEED,
            repo.as_ref(),
            default_branch_name.as_ref(),
        ],
        pid,
    );
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

#[allow(clippy::cast_possible_truncation)] // Ed25519 instruction offsets are tiny
fn ed25519_verify_instruction(
    message: &[u8],
    signature: &[u8; 64],
    pubkey: &[u8; 32],
) -> Instruction {
    const DATA_START: usize = 16;
    let mut data = Vec::new();
    data.push(1);
    data.push(0);
    let sig_off = DATA_START as u16;
    let pk_off = (DATA_START + 64) as u16;
    let msg_off = (DATA_START + 64 + 32) as u16;
    let msg_len = message.len() as u16;
    data.extend_from_slice(&sig_off.to_le_bytes());
    data.extend_from_slice(&u16::MAX.to_le_bytes());
    data.extend_from_slice(&pk_off.to_le_bytes());
    data.extend_from_slice(&u16::MAX.to_le_bytes());
    data.extend_from_slice(&msg_off.to_le_bytes());
    data.extend_from_slice(&msg_len.to_le_bytes());
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

#[allow(clippy::too_many_arguments, clippy::similar_names)]
fn create_commit_instruction(
    pid: &Address,
    author: &Address,
    repository: Address,
    commit_oid: [u8; 32],
    parent_count: u8,
    parent_a: [u8; 32],
    parent_b: [u8; 32],
    tree_oid: [u8; 32],
    authored_at: i64,
    message_hash: [u8; 32],
    attestation_hash: [u8; 32],
    parent_a_account: Address,
    parent_b_account: Address,
) -> Instruction {
    let (commit, _) = Address::find_program_address(
        &[COMMIT_SEED, repository.as_ref(), commit_oid.as_ref()],
        pid,
    );
    let mut data = Vec::new();
    data.extend_from_slice(&disc("create_commit"));
    data.extend_from_slice(&commit_oid);
    data.push(parent_count);
    data.extend_from_slice(&parent_a);
    data.extend_from_slice(&parent_b);
    data.extend_from_slice(&tree_oid);
    data.extend_from_slice(&authored_at.to_le_bytes());
    data.extend_from_slice(&message_hash);
    data.extend_from_slice(&attestation_hash);

    Instruction {
        program_id: *pid,
        accounts: vec![
            AccountMeta::new(*author, true),
            AccountMeta::new(repository, false),
            AccountMeta::new(commit, false),
            AccountMeta::new_readonly(parent_a_account, false),
            AccountMeta::new_readonly(parent_b_account, false),
            AccountMeta::new_readonly(instructions_sysvar(), false),
            AccountMeta::new_readonly(ed25519_program(), false),
            AccountMeta::new_readonly(system_program(), false),
            AccountMeta::new_readonly(event_authority(pid), false),
            AccountMeta::new_readonly(*pid, false),
        ],
        data,
    }
}

fn sample_attestation(
    repo: &Address,
    author: &Address,
    commit_oid: [u8; 32],
    tree_oid: [u8; 32],
    message_hash: [u8; 32],
) -> ([u8; 32], Attestation) {
    let commit = Oid::new(HashAlgorithm::Sha256, commit_oid.to_vec()).unwrap();
    let tree = Oid::new(HashAlgorithm::Sha256, tree_oid.to_vec()).unwrap();
    let msg = Oid::new(HashAlgorithm::Sha256, message_hash.to_vec()).unwrap();
    let attestation = Attestation::new(
        1,
        repo.to_string(),
        commit,
        vec![],
        tree,
        author.to_string(),
        1_700_000_000,
        msg,
        "11111111111111111111111111111112",
    )
    .expect("attestation");
    let hash = attestation.attestation_hash().to_bytes32();
    (hash, attestation)
}

#[allow(clippy::result_large_err)]
fn send(
    svm: &mut LiteSVM,
    payer: &Keypair,
    instructions: &[Instruction],
) -> std::result::Result<
    litesvm::types::TransactionMetadata,
    litesvm::types::FailedTransactionMetadata,
> {
    // Fresh blockhash per send so repeated/identical instructions are not
    // rejected as AlreadyProcessed (e.g. the duplicate-commit test).
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

fn init_repo(svm: &mut LiteSVM, pid: &Address, owner: &Keypair) -> Address {
    let repo_name = name("forge");
    let ix = initialize_instruction(pid, &owner.pubkey(), repo_name, name("main"));
    send(svm, owner, &[ix]).expect("initialize");
    repo_address(pid, &owner.pubkey(), &repo_name)
}

#[test]
fn create_commit_root_advances_history_root() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);

    let commit_oid = [3u8; 32];
    let tree_oid = [4u8; 32];
    let message_hash = [5u8; 32];
    let (attestation_hash, _att) =
        sample_attestation(&repo, &owner.pubkey(), commit_oid, tree_oid, message_hash);
    let sig = owner
        .sign_message(&attestation_hash)
        .as_ref()
        .try_into()
        .expect("signature length");
    let ed_ix = ed25519_verify_instruction(&attestation_hash, &sig, &owner.pubkey().to_bytes());
    let forge_ix = create_commit_instruction(
        &pid,
        &owner.pubkey(),
        repo,
        commit_oid,
        0,
        [0u8; 32],
        [0u8; 32],
        tree_oid,
        1_700_000_000,
        message_hash,
        attestation_hash,
        system_program(),
        system_program(),
    );
    let meta = send(&mut svm, &owner, &[ed_ix, forge_ix]).expect("create_commit");

    let repo_acct = svm.get_account(&repo).expect("repo");
    let repo_state =
        RepositoryAccount::try_deserialize(&mut repo_acct.data.as_slice()).expect("repo");
    assert_eq!(repo_state.commit_count, 1);
    let expected_history = forge_object::history::append_history_root(
        HashAlgorithm::Sha256,
        &forge_object::history::genesis_history_root(&repo.to_bytes(), HashAlgorithm::Sha256),
        &Oid::new(HashAlgorithm::Sha256, commit_oid.to_vec()).unwrap(),
        0,
    )
    .unwrap()
    .to_bytes32();
    assert_eq!(repo_state.history_root, expected_history);

    let commit_addr = commit_address(&pid, &repo, &commit_oid);
    let commit_acct = svm.get_account(&commit_addr).expect("commit");
    let commit_state =
        CommitAccount::try_deserialize(&mut commit_acct.data.as_slice()).expect("commit");
    assert_eq!(commit_state.seq, 0);
    assert_eq!(commit_state.author.to_bytes(), owner.pubkey().to_bytes());
    assert_eq!(commit_state.attestation_hash, attestation_hash);

    let events: Vec<CommitCreated> = decode_events(&meta);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].seq, 0);
    assert_eq!(events[0].history_root, expected_history);
}

#[test]
fn create_commit_rejects_bad_signature() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    let commit_oid = [3u8; 32];
    let tree_oid = [4u8; 32];
    let message_hash = [5u8; 32];
    let (attestation_hash, _) =
        sample_attestation(&repo, &owner.pubkey(), commit_oid, tree_oid, message_hash);
    let sig = owner
        .sign_message(&[0u8; 32])
        .as_ref()
        .try_into()
        .expect("signature length");
    let ed_ix = ed25519_verify_instruction(&[0u8; 32], &sig, &owner.pubkey().to_bytes());
    let forge_ix = create_commit_instruction(
        &pid,
        &owner.pubkey(),
        repo,
        commit_oid,
        0,
        [0u8; 32],
        [0u8; 32],
        tree_oid,
        1_700_000_000,
        message_hash,
        attestation_hash,
        system_program(),
        system_program(),
    );
    let err = send(&mut svm, &owner, &[ed_ix, forge_ix]).expect_err("bad sig");
    assert_eq!(
        custom_error_code(&err.err),
        Some(6012),
        "expected BadSignature (6012), got {:?}",
        err.err
    );
}

#[test]
fn create_commit_rejects_root_on_nonempty_repo() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    let commit_oid = [3u8; 32];
    let tree_oid = [4u8; 32];
    let message_hash = [5u8; 32];
    let (attestation_hash, _) =
        sample_attestation(&repo, &owner.pubkey(), commit_oid, tree_oid, message_hash);
    let sig = owner
        .sign_message(&attestation_hash)
        .as_ref()
        .try_into()
        .expect("signature length");
    let ed_ix = ed25519_verify_instruction(&attestation_hash, &sig, &owner.pubkey().to_bytes());
    let forge_ix = create_commit_instruction(
        &pid,
        &owner.pubkey(),
        repo,
        commit_oid,
        0,
        [0u8; 32],
        [0u8; 32],
        tree_oid,
        1_700_000_000,
        message_hash,
        attestation_hash,
        system_program(),
        system_program(),
    );
    send(&mut svm, &owner, &[ed_ix.clone(), forge_ix.clone()]).expect("first commit");

    let commit_oid2 = [6u8; 32];
    let (attestation_hash2, _) =
        sample_attestation(&repo, &owner.pubkey(), commit_oid2, tree_oid, message_hash);
    let sig2 = owner
        .sign_message(&attestation_hash2)
        .as_ref()
        .try_into()
        .expect("signature length");
    let ed_ix2 = ed25519_verify_instruction(&attestation_hash2, &sig2, &owner.pubkey().to_bytes());
    let forge_ix2 = create_commit_instruction(
        &pid,
        &owner.pubkey(),
        repo,
        commit_oid2,
        0,
        [0u8; 32],
        [0u8; 32],
        tree_oid,
        1_700_000_000,
        message_hash,
        attestation_hash2,
        system_program(),
        system_program(),
    );
    let err = send(&mut svm, &owner, &[ed_ix2, forge_ix2]).expect_err("second root");
    assert_eq!(
        custom_error_code(&err.err),
        Some(6011),
        "expected RootOnNonemptyRepo (6011), got {:?}",
        err.err
    );
}

// ---------------------------------------------------------------------------
// Additional negative paths (§6.9, §11) and compute-budget check (§9.5)
// ---------------------------------------------------------------------------

/// Builds a valid `[ed25519_verify, create_commit]` pair signed by `author`.
/// `tree_oid`/`message_hash` are fixed to keep the tests terse.
#[allow(clippy::too_many_arguments, clippy::similar_names)]
fn signed_commit(
    pid: &Address,
    repo: Address,
    author: &Keypair,
    commit_oid: [u8; 32],
    parent_count: u8,
    parent_a: [u8; 32],
    parent_b: [u8; 32],
    parent_a_account: Address,
    parent_b_account: Address,
) -> [Instruction; 2] {
    let tree_oid = [4u8; 32];
    let message_hash = [5u8; 32];
    let (attestation_hash, _) =
        sample_attestation(&repo, &author.pubkey(), commit_oid, tree_oid, message_hash);
    let sig = author
        .sign_message(&attestation_hash)
        .as_ref()
        .try_into()
        .expect("signature length");
    let ed_ix = ed25519_verify_instruction(&attestation_hash, &sig, &author.pubkey().to_bytes());
    let forge_ix = create_commit_instruction(
        pid,
        &author.pubkey(),
        repo,
        commit_oid,
        parent_count,
        parent_a,
        parent_b,
        tree_oid,
        1_700_000_000,
        message_hash,
        attestation_hash,
        parent_a_account,
        parent_b_account,
    );
    [ed_ix, forge_ix]
}

fn assert_custom(code: u32, err: &solana_transaction::TransactionError) {
    assert_eq!(
        custom_error_code(err),
        Some(code),
        "expected error {code}, got {err:?}"
    );
}

#[test]
fn create_commit_rejects_duplicate_commit() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    let pair = signed_commit(
        &pid,
        repo,
        &owner,
        [3u8; 32],
        0,
        [0u8; 32],
        [0u8; 32],
        system_program(),
        system_program(),
    );
    send(&mut svm, &owner, &pair).expect("first commit");
    // Second create_commit for the same commit oid must fail at `init`.
    assert!(
        send(&mut svm, &owner, &pair).is_err(),
        "duplicate commit must be rejected"
    );
}

#[test]
fn create_commit_rejects_invalid_parent_count() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    let pair = signed_commit(
        &pid,
        repo,
        &owner,
        [3u8; 32],
        3,
        [0u8; 32],
        [0u8; 32],
        system_program(),
        system_program(),
    );
    let err = send(&mut svm, &owner, &pair).expect_err("parent_count 3");
    assert_custom(6008, &err.err); // InvalidParentCount
}

#[test]
fn create_commit_rejects_non_root_without_parent() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    // parent_count == 1 but parent_a is all-zero -> InvalidParent.
    let pair = signed_commit(
        &pid,
        repo,
        &owner,
        [3u8; 32],
        1,
        [0u8; 32],
        [0u8; 32],
        system_program(),
        system_program(),
    );
    let err = send(&mut svm, &owner, &pair).expect_err("missing parent");
    assert_custom(6014, &err.err); // InvalidParent
}

#[test]
fn create_commit_rejects_self_parent() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    let commit_oid = [3u8; 32];
    let pair = signed_commit(
        &pid,
        repo,
        &owner,
        commit_oid,
        1,
        commit_oid,
        [0u8; 32],
        system_program(),
        system_program(),
    );
    let err = send(&mut svm, &owner, &pair).expect_err("self parent");
    assert_custom(6010, &err.err); // SelfParent
}

#[test]
fn create_commit_rejects_unknown_parent_account() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    // Parent oid is non-zero, but the supplied account is not the commit PDA.
    let pair = signed_commit(
        &pid,
        repo,
        &owner,
        [3u8; 32],
        1,
        [7u8; 32],
        [0u8; 32],
        system_program(),
        system_program(),
    );
    let err = send(&mut svm, &owner, &pair).expect_err("unknown parent");
    assert_custom(6004, &err.err); // InvalidPda
}

#[test]
fn create_commit_rejects_zero_commit_oid() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    let pair = signed_commit(
        &pid,
        repo,
        &owner,
        [0u8; 32],
        0,
        [0u8; 32],
        [0u8; 32],
        system_program(),
        system_program(),
    );
    let err = send(&mut svm, &owner, &pair).expect_err("zero commit oid");
    assert_custom(6009, &err.err); // InvalidCommitOid
}

#[test]
fn create_commit_rejects_unauthorized_author() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    let mallory = Keypair::new();
    svm.airdrop(&mallory.pubkey(), 10_000_000_000)
        .expect("airdrop");
    let pair = signed_commit(
        &pid,
        repo,
        &mallory,
        [3u8; 32],
        0,
        [0u8; 32],
        [0u8; 32],
        system_program(),
        system_program(),
    );
    let err = send(&mut svm, &mallory, &pair).expect_err("non-owner author");
    assert_custom(6001, &err.err); // Unauthorized
}

#[test]
fn create_commit_rejects_forged_author_signature() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    let mallory = Keypair::new();
    let commit_oid = [3u8; 32];
    let tree_oid = [4u8; 32];
    let message_hash = [5u8; 32];
    // Attestation + Ed25519 signed by Mallory, but the author account is owner.
    let (attestation_hash, _) =
        sample_attestation(&repo, &owner.pubkey(), commit_oid, tree_oid, message_hash);
    let sig = mallory
        .sign_message(&attestation_hash)
        .as_ref()
        .try_into()
        .expect("signature length");
    let ed_ix = ed25519_verify_instruction(&attestation_hash, &sig, &mallory.pubkey().to_bytes());
    let forge_ix = create_commit_instruction(
        &pid,
        &owner.pubkey(),
        repo,
        commit_oid,
        0,
        [0u8; 32],
        [0u8; 32],
        tree_oid,
        1_700_000_000,
        message_hash,
        attestation_hash,
        system_program(),
        system_program(),
    );
    let err = send(&mut svm, &owner, &[ed_ix, forge_ix]).expect_err("forged author");
    assert_custom(6012, &err.err); // BadSignature
}

#[test]
fn create_commit_rejects_multiple_ed25519_instructions() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    let commit_oid = [3u8; 32];
    let tree_oid = [4u8; 32];
    let message_hash = [5u8; 32];
    let (attestation_hash, _) =
        sample_attestation(&repo, &owner.pubkey(), commit_oid, tree_oid, message_hash);
    let sig = owner
        .sign_message(&attestation_hash)
        .as_ref()
        .try_into()
        .expect("signature length");
    let ed_ix = ed25519_verify_instruction(&attestation_hash, &sig, &owner.pubkey().to_bytes());
    let other = [9u8; 32];
    let sig2 = owner
        .sign_message(&other)
        .as_ref()
        .try_into()
        .expect("signature length");
    let ed_ix2 = ed25519_verify_instruction(&other, &sig2, &owner.pubkey().to_bytes());
    let forge_ix = create_commit_instruction(
        &pid,
        &owner.pubkey(),
        repo,
        commit_oid,
        0,
        [0u8; 32],
        [0u8; 32],
        tree_oid,
        1_700_000_000,
        message_hash,
        attestation_hash,
        system_program(),
        system_program(),
    );
    let err = send(&mut svm, &owner, &[ed_ix, ed_ix2, forge_ix]).expect_err("two ed25519 ixs");
    assert_custom(6013, &err.err); // InvalidEd25519Instruction
}

#[test]
fn create_commit_compute_units_within_default_limit() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    let pair = signed_commit(
        &pid,
        repo,
        &owner,
        [3u8; 32],
        0,
        [0u8; 32],
        [0u8; 32],
        system_program(),
        system_program(),
    );
    let meta = send(&mut svm, &owner, &pair).expect("create_commit");
    let cu = meta.compute_units_consumed;
    eprintln!("create_commit compute units (incl. Ed25519 precompile): {cu}");
    assert!(
        cu < 200_000,
        "create_commit exceeded the default 200k CU limit: {cu}"
    );
}

/// Regression: a commit whose parent already exists must be accepted, which
/// exercises `validate_parent_account` deserializing an existing `CommitAccount`.
#[test]
fn create_commit_accepts_parent_chain() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    let tree_oid = [4u8; 32];
    let message_hash = [5u8; 32];

    let c1 = [3u8; 32];
    let (ah1, _) = sample_attestation(&repo, &owner.pubkey(), c1, tree_oid, message_hash);
    let sig1 = owner
        .sign_message(&ah1)
        .as_ref()
        .try_into()
        .expect("signature");
    let ed1 = ed25519_verify_instruction(&ah1, &sig1, &owner.pubkey().to_bytes());
    let forge1 = create_commit_instruction(
        &pid,
        &owner.pubkey(),
        repo,
        c1,
        0,
        [0u8; 32],
        [0u8; 32],
        tree_oid,
        1_700_000_000,
        message_hash,
        ah1,
        system_program(),
        system_program(),
    );
    send(&mut svm, &owner, &[ed1, forge1]).expect("root commit");

    let c2 = [6u8; 32];
    let (ah2, _) = sample_attestation(&repo, &owner.pubkey(), c2, tree_oid, message_hash);
    let sig2 = owner
        .sign_message(&ah2)
        .as_ref()
        .try_into()
        .expect("signature");
    let ed2 = ed25519_verify_instruction(&ah2, &sig2, &owner.pubkey().to_bytes());
    let c1_addr = commit_address(&pid, &repo, &c1);
    let forge2 = create_commit_instruction(
        &pid,
        &owner.pubkey(),
        repo,
        c2,
        1,
        c1,
        [0u8; 32],
        tree_oid,
        1_700_000_001,
        message_hash,
        ah2,
        c1_addr,
        system_program(),
    );
    send(&mut svm, &owner, &[ed2, forge2]).expect("child commit");

    let repo_state = RepositoryAccount::try_deserialize(
        &mut svm.get_account(&repo).expect("repo").data.as_slice(),
    )
    .expect("repo");
    assert_eq!(repo_state.commit_count, 2);

    let c2_addr = commit_address(&pid, &repo, &c2);
    let c2_state =
        CommitAccount::try_deserialize(&mut svm.get_account(&c2_addr).expect("c2").data.as_slice())
            .expect("c2");
    assert_eq!(c2_state.parent_count, 1);
    assert_eq!(c2_state.parent_a, c1);
    assert_eq!(c2_state.seq, 1);
}

#[allow(dead_code)]
fn _message_import() {
    let _ = std::mem::size_of::<Message>();
}
