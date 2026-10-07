//! Phase 9 integration tests: tags, permissions, ownership transfer, and
//! program source provenance.
//!
//! Spec traceability: §4.5 (tags), §4.6 (permissions), §9.2, §12.2/§12.4
//! (provenance), §15 (ownership), §16.4.

use anchor_lang::prelude::Pubkey;
use anchor_lang::{AccountDeserialize, AccountSerialize, AnchorDeserialize, Discriminator};
use forge_object::{tag::tag_message, Attestation, HashAlgorithm, Oid};
use forge_repository::constants::{
    BRANCH_SEED, COMMIT_SEED, PERM_SEED, PROG_SEED, REPO_SEED, TAG_SEED,
};
use forge_repository::events::{
    PermissionChanged, ProgramSourceAnchored, RepositoryTransferred, TagCreated,
};
use forge_repository::state::permission::{ROLE_MAINTAINER, ROLE_READER, ROLE_WRITER};
use forge_repository::state::{
    CommitAccount, PermissionAccount, ProgramSourceAttestation, RepositoryAccount, TagAccount,
};
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
        eprintln!("skipping LiteSVM test: {} not found", so.display());
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
    .expect("ed25519 precompile");
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

fn perm_address(pid: &Address, repo: &Address, contributor: &Address) -> Address {
    Address::find_program_address(&[PERM_SEED, repo.as_ref(), contributor.as_ref()], pid).0
}

fn tag_address(pid: &Address, repo: &Address, tag_name: &[u8; 32]) -> Address {
    Address::find_program_address(&[TAG_SEED, repo.as_ref(), tag_name.as_ref()], pid).0
}

fn prog_address(pid: &Address, program_id_arg: &Address) -> Address {
    Address::find_program_address(&[PROG_SEED, program_id_arg.as_ref()], pid).0
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
    data.push(2);
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

fn create_tag_instruction(
    pid: &Address,
    tagger: &Address,
    repo: Address,
    tag_name: [u8; 32],
    target_commit: [u8; 32],
    message_hash: [u8; 32],
) -> Instruction {
    let tag = tag_address(pid, &repo, &tag_name);
    let commit = commit_address(pid, &repo, &target_commit);
    let mut data = Vec::new();
    data.extend_from_slice(&disc("create_tag"));
    data.extend_from_slice(&tag_name);
    data.extend_from_slice(&target_commit);
    data.extend_from_slice(&message_hash);
    Instruction {
        program_id: *pid,
        accounts: vec![
            AccountMeta::new(*tagger, true),
            AccountMeta::new_readonly(repo, false),
            AccountMeta::new(tag, false),
            AccountMeta::new_readonly(commit, false),
            AccountMeta::new_readonly(instructions_sysvar(), false),
            AccountMeta::new_readonly(ed25519_program(), false),
            AccountMeta::new_readonly(system_program(), false),
            AccountMeta::new_readonly(event_authority(pid), false),
            AccountMeta::new_readonly(*pid, false),
        ],
        data,
    }
}

fn update_permissions_instruction(
    pid: &Address,
    admin: &Address,
    repo: Address,
    contributor: Address,
    role: u8,
    expires_slot: u64,
) -> Instruction {
    let permission = perm_address(pid, &repo, &contributor);
    let mut data = Vec::new();
    data.extend_from_slice(&disc("update_permissions"));
    data.extend_from_slice(contributor.as_ref());
    data.push(role);
    data.extend_from_slice(&expires_slot.to_le_bytes());
    Instruction {
        program_id: *pid,
        accounts: vec![
            AccountMeta::new(*admin, true),
            AccountMeta::new(repo, false),
            AccountMeta::new(permission, false),
            AccountMeta::new_readonly(system_program(), false),
            AccountMeta::new_readonly(event_authority(pid), false),
            AccountMeta::new_readonly(*pid, false),
        ],
        data,
    }
}

fn transfer_instruction(
    pid: &Address,
    owner: &Address,
    repo: Address,
    new_owner: Address,
) -> Instruction {
    let mut data = Vec::new();
    data.extend_from_slice(&disc("transfer_repository"));
    data.extend_from_slice(new_owner.as_ref());
    Instruction {
        program_id: *pid,
        accounts: vec![
            AccountMeta::new_readonly(*owner, true),
            AccountMeta::new(repo, false),
            AccountMeta::new_readonly(event_authority(pid), false),
            AccountMeta::new_readonly(*pid, false),
        ],
        data,
    }
}

#[allow(clippy::too_many_arguments)]
fn anchor_program_source_instruction(
    pid: &Address,
    attester: &Address,
    repo: Address,
    program_id_arg: Address,
    commit_oid: [u8; 32],
    artifact_hash: [u8; 32],
    build_metadata_hash: [u8; 32],
) -> Instruction {
    let attestation = prog_address(pid, &program_id_arg);
    let commit = commit_address(pid, &repo, &commit_oid);
    let mut data = Vec::new();
    data.extend_from_slice(&disc("anchor_program_source"));
    data.extend_from_slice(program_id_arg.as_ref());
    data.extend_from_slice(&commit_oid);
    data.extend_from_slice(&artifact_hash);
    data.extend_from_slice(&build_metadata_hash);
    Instruction {
        program_id: *pid,
        accounts: vec![
            AccountMeta::new(*attester, true),
            AccountMeta::new_readonly(repo, false),
            AccountMeta::new(attestation, false),
            AccountMeta::new_readonly(commit, false),
            AccountMeta::new_readonly(program_id_arg, false),
            AccountMeta::new_readonly(system_program(), false),
            AccountMeta::new_readonly(event_authority(pid), false),
            AccountMeta::new_readonly(*pid, false),
        ],
        data,
    }
}

#[allow(clippy::too_many_arguments)]
fn create_commit_instruction(
    pid: &Address,
    author: &Address,
    repo: Address,
    commit_oid: [u8; 32],
    tree_oid: [u8; 32],
    attestation_hash: [u8; 32],
    extra: &[AccountMeta],
) -> Instruction {
    let commit = commit_address(pid, &repo, &commit_oid);
    let mut data = Vec::new();
    data.extend_from_slice(&disc("create_commit"));
    data.extend_from_slice(&commit_oid);
    data.push(0); // parent_count = 0
    data.extend_from_slice(&[0u8; 32]);
    data.extend_from_slice(&[0u8; 32]);
    data.extend_from_slice(&tree_oid);
    data.extend_from_slice(&1_700_000_000i64.to_le_bytes());
    data.extend_from_slice(&[0u8; 32]); // message_hash
    data.extend_from_slice(&attestation_hash);
    let mut accounts = vec![
        AccountMeta::new(*author, true),
        AccountMeta::new(repo, false),
        AccountMeta::new(commit, false),
        AccountMeta::new_readonly(system_program(), false),
        AccountMeta::new_readonly(system_program(), false),
        AccountMeta::new_readonly(instructions_sysvar(), false),
        AccountMeta::new_readonly(ed25519_program(), false),
        AccountMeta::new_readonly(system_program(), false),
        AccountMeta::new_readonly(event_authority(pid), false),
        AccountMeta::new_readonly(*pid, false),
    ];
    accounts.extend_from_slice(extra);
    Instruction {
        program_id: *pid,
        accounts,
        data,
    }
}

#[allow(clippy::cast_possible_truncation)]
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

fn oid(oid: [u8; 32]) -> Oid {
    Oid::new(HashAlgorithm::Sha256, oid.to_vec()).expect("oid")
}

// ---------------------------------------------------------------------------
// State / plumbing
// ---------------------------------------------------------------------------

fn init_repo(svm: &mut LiteSVM, pid: &Address, owner: &Keypair) -> Address {
    let ix = initialize_instruction(pid, &owner.pubkey(), name("forge"), name("main"));
    send(svm, owner, &[ix]).expect("initialize");
    repo_address(pid, &owner.pubkey(), &name("forge"))
}

fn install_commit(
    svm: &mut LiteSVM,
    pid: &Address,
    template: &Address,
    repo: Address,
    oid: [u8; 32],
) -> Address {
    let addr = commit_address(pid, &repo, &oid);
    let commit = CommitAccount {
        repo: Pubkey::new_from_array(repo.to_bytes()),
        commit_oid: oid,
        parent_count: 0,
        parent_a: [0u8; 32],
        parent_b: [0u8; 32],
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
    let mut acct = svm.get_account(template).expect("template");
    acct.lamports = 10_000_000;
    acct.data = data;
    acct.owner = *pid;
    acct.executable = false;
    acct.rent_epoch = 0;
    svm.set_account(addr, acct).expect("install commit");
    addr
}

fn install_program(svm: &mut LiteSVM, program_addr: Address) {
    // Deploy a real upgradeable program (reusing the Forge .so bytes) at an
    // arbitrary address so the provenance check sees a loader-owned executable.
    let bytes = std::fs::read(program_so()).expect("read program so");
    svm.add_program(program_addr, &bytes)
        .expect("install program account");
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
        "expected {code}, got {err:?}"
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

fn sample_attestation(
    repo: &Address,
    author: &Address,
    commit_oid: [u8; 32],
    tree_oid: [u8; 32],
) -> [u8; 32] {
    let attestation = Attestation::new(
        1,
        repo.to_string(),
        oid(commit_oid),
        vec![],
        oid(tree_oid),
        author.to_string(),
        1_700_000_000,
        oid([5u8; 32]),
        "11111111111111111111111111111112",
    )
    .expect("attestation");
    attestation.attestation_hash().to_bytes32()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn create_tag_creates_signed_tag_and_rejects_duplicate() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    let template = owner.pubkey();
    let target = [0xC1u8; 32];
    install_commit(&mut svm, &pid, &template, repo, target);
    let tag_name = name("v1.0.0");
    let message_hash = [0x42u8; 32];

    let message = tag_message(
        &repo.to_bytes(),
        &tag_name,
        &oid(target),
        &oid(message_hash),
    );
    let ed = ed25519_ix(&message, &owner);
    let ix = create_tag_instruction(&pid, &owner.pubkey(), repo, tag_name, target, message_hash);
    let meta = send(&mut svm, &owner, &[ed, ix]).expect("create_tag");
    let events: Vec<TagCreated> = decode_events(&meta);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].signed, 1);

    let tag_addr = tag_address(&pid, &repo, &tag_name);
    let tag =
        TagAccount::try_deserialize(&mut svm.get_account(&tag_addr).expect("tag").data.as_slice())
            .expect("tag");
    assert_eq!(tag.target_commit, target);
    assert_eq!(tag.signed, 1);

    // Duplicate tag name fails at init.
    let message = tag_message(
        &repo.to_bytes(),
        &tag_name,
        &oid(target),
        &oid(message_hash),
    );
    let ed = ed25519_ix(&message, &owner);
    let ix = create_tag_instruction(&pid, &owner.pubkey(), repo, tag_name, target, message_hash);
    assert!(
        send(&mut svm, &owner, &[ed, ix]).is_err(),
        "duplicate tag must fail"
    );
}

#[test]
fn create_tag_rejects_non_owner_without_permission() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    install_commit(&mut svm, &pid, &owner.pubkey(), repo, [0xC1u8; 32]);
    let mallory = Keypair::new();
    svm.airdrop(&mallory.pubkey(), 10_000_000_000).unwrap();

    let message = tag_message(
        &repo.to_bytes(),
        &name("v9"),
        &oid([0xC1u8; 32]),
        &oid([0x42u8; 32]),
    );
    let ed = ed25519_ix(&message, &mallory);
    let ix = create_tag_instruction(
        &pid,
        &mallory.pubkey(),
        repo,
        name("v9"),
        [0xC1u8; 32],
        [0x42u8; 32],
    );
    let err = send(&mut svm, &mallory, &[ed, ix]).expect_err("non-owner");
    assert_custom(6001, &err.err); // Unauthorized
}

#[test]
fn update_permissions_creates_then_updates() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    let bob = Keypair::new();

    let ix =
        update_permissions_instruction(&pid, &owner.pubkey(), repo, bob.pubkey(), ROLE_WRITER, 0);
    let meta = send(&mut svm, &owner, &[ix]).expect("grant writer");
    let events: Vec<PermissionChanged> = decode_events(&meta);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].role, ROLE_WRITER);

    let perm_addr = perm_address(&pid, &repo, &bob.pubkey());
    let perm = PermissionAccount::try_deserialize(
        &mut svm.get_account(&perm_addr).expect("perm").data.as_slice(),
    )
    .expect("perm");
    assert_eq!(perm.role, ROLE_WRITER);

    // Update to maintainer.
    let ix = update_permissions_instruction(
        &pid,
        &owner.pubkey(),
        repo,
        bob.pubkey(),
        ROLE_MAINTAINER,
        999,
    );
    send(&mut svm, &owner, &[ix]).expect("promote");
    let perm = PermissionAccount::try_deserialize(
        &mut svm.get_account(&perm_addr).expect("perm").data.as_slice(),
    )
    .expect("perm");
    assert_eq!(perm.role, ROLE_MAINTAINER);
    assert_eq!(perm.expires_slot, 999);
}

#[test]
fn update_permissions_rejects_invalid_role() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    let bob = Keypair::new();
    let ix = update_permissions_instruction(&pid, &owner.pubkey(), repo, bob.pubkey(), 9, 0);
    let err = send(&mut svm, &owner, &[ix]).expect_err("role 9");
    assert_custom(6019, &err.err); // InvalidRole
}

#[test]
fn transfer_repository_changes_owner() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    let new_owner = Keypair::new();
    let ix = transfer_instruction(&pid, &owner.pubkey(), repo, new_owner.pubkey());
    let meta = send(&mut svm, &owner, &[ix]).expect("transfer");
    let events: Vec<RepositoryTransferred> = decode_events(&meta);
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].new_owner.to_bytes(),
        new_owner.pubkey().to_bytes()
    );

    let repo_state = RepositoryAccount::try_deserialize(
        &mut svm.get_account(&repo).expect("repo").data.as_slice(),
    )
    .expect("repo");
    assert_eq!(repo_state.owner.to_bytes(), new_owner.pubkey().to_bytes());
}

#[test]
fn transfer_repository_rejects_non_owner() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    let mallory = Keypair::new();
    svm.airdrop(&mallory.pubkey(), 10_000_000_000).unwrap();
    let ix = transfer_instruction(&pid, &mallory.pubkey(), repo, mallory.pubkey());
    let err = send(&mut svm, &mallory, &[ix]).expect_err("non-owner");
    assert_custom(6001, &err.err);
}

#[test]
fn anchor_program_source_creates_claim_and_rejects_duplicate() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    let commit_oid = [0xC1u8; 32];
    install_commit(&mut svm, &pid, &owner.pubkey(), repo, commit_oid);

    let deployed = Address::new_from_array([0xABu8; 32]);
    install_program(&mut svm, deployed);

    let artifact = [0x77u8; 32];
    let build_meta = [0x88u8; 32];
    let ix = anchor_program_source_instruction(
        &pid,
        &owner.pubkey(),
        repo,
        deployed,
        commit_oid,
        artifact,
        build_meta,
    );
    let meta = send(&mut svm, &owner, &[ix]).expect("anchor claim");
    let events: Vec<ProgramSourceAnchored> = decode_events(&meta);
    assert_eq!(events.len(), 1);

    let addr = prog_address(&pid, &deployed);
    let claim = ProgramSourceAttestation::try_deserialize(
        &mut svm.get_account(&addr).expect("claim").data.as_slice(),
    )
    .expect("claim");
    assert_eq!(claim.commit_oid, commit_oid);
    assert_eq!(claim.artifact_hash, artifact);
    assert_eq!(claim.verified, 0);

    // Duplicate claim for the same program fails at init.
    let ix = anchor_program_source_instruction(
        &pid,
        &owner.pubkey(),
        repo,
        deployed,
        commit_oid,
        artifact,
        build_meta,
    );
    assert!(send(&mut svm, &owner, &[ix]).is_err(), "duplicate claim");
}

#[test]
fn anchor_program_source_rejects_non_upgradeable_program() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    let commit_oid = [0xC1u8; 32];
    install_commit(&mut svm, &pid, &owner.pubkey(), repo, commit_oid);

    // A plain system-owned account is not a deployed program.
    let not_program = Address::new_from_array([0xCDu8; 32]);
    let ix = anchor_program_source_instruction(
        &pid,
        &owner.pubkey(),
        repo,
        not_program,
        commit_oid,
        [0x77u8; 32],
        [0x88u8; 32],
    );
    let err = send(&mut svm, &owner, &[ix]).expect_err("not a program");
    assert_custom(6021, &err.err); // ProgramNotUpgradeable
}

#[test]
fn allowlist_writer_can_commit_but_reader_cannot() {
    let Some((mut svm, pid, owner)) = setup() else {
        return;
    };
    let repo = init_repo(&mut svm, &pid, &owner);
    let bob = Keypair::new();
    svm.airdrop(&bob.pubkey(), 50_000_000_000).unwrap();

    // Owner grants Bob writer.
    let ix =
        update_permissions_instruction(&pid, &owner.pubkey(), repo, bob.pubkey(), ROLE_WRITER, 0);
    send(&mut svm, &owner, &[ix]).expect("grant writer");

    let commit_oid = [0xC1u8; 32];
    let tree_oid = [0xD1u8; 32];
    let attestation_hash = sample_attestation(&repo, &bob.pubkey(), commit_oid, tree_oid);
    let ed = ed25519_ix(&attestation_hash, &bob);
    let perm_meta = AccountMeta::new_readonly(perm_address(&pid, &repo, &bob.pubkey()), false);

    // Without the permission account -> Unauthorized.
    let ix = create_commit_instruction(
        &pid,
        &bob.pubkey(),
        repo,
        commit_oid,
        tree_oid,
        attestation_hash,
        &[],
    );
    let err = send(&mut svm, &bob, &[ed.clone(), ix]).expect_err("no permission account");
    assert_custom(6001, &err.err); // Unauthorized

    // With writer permission -> succeeds.
    let ix = create_commit_instruction(
        &pid,
        &bob.pubkey(),
        repo,
        commit_oid,
        tree_oid,
        attestation_hash,
        std::slice::from_ref(&perm_meta),
    );
    send(&mut svm, &bob, &[ed, ix]).expect("writer may commit");

    // Downgrade Bob to reader; a fresh commit is now insufficient role.
    let ix =
        update_permissions_instruction(&pid, &owner.pubkey(), repo, bob.pubkey(), ROLE_READER, 0);
    send(&mut svm, &owner, &[ix]).expect("downgrade");

    let commit_oid2 = [0xC2u8; 32];
    let ah2 = sample_attestation(&repo, &bob.pubkey(), commit_oid2, tree_oid);
    let ed2 = ed25519_ix(&ah2, &bob);
    let ix = create_commit_instruction(
        &pid,
        &bob.pubkey(),
        repo,
        commit_oid2,
        tree_oid,
        ah2,
        &[perm_meta],
    );
    let err = send(&mut svm, &bob, &[ed2, ix]).expect_err("reader cannot commit");
    assert_custom(6020, &err.err); // InsufficientRole
}

#[allow(dead_code)]
fn _message_import() {
    let _ = std::mem::size_of::<Message>();
}
