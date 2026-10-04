//! Solana RPC / LiteSVM client, PDA derivation, and Ed25519-prepended
//! transactions (Phase 8, §9.4, §13).
//!
//! Instruction data matches the Phase 4/5 LiteSVM tests: Anchor
//! `global:<name>` discriminators and `emit_cpi!` trailing accounts.

use std::str::FromStr;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, bail, Context, Result};
use forge_object::hash::{HashAlgorithm, Oid};
use litesvm::LiteSVM;
use solana_account::Account;
use solana_keypair::Keypair;
use solana_message::{AccountMeta, Address, Instruction};
use solana_signer::Signer;
use solana_transaction::Transaction;

/// Canonical program id (`declare_id!`).
pub const PROGRAM_ID_STR: &str = "4smCAEoycSXSvVsyic8ircQmHmENPCHn17Fma83SYVbf";

const SYSTEM_PROGRAM: &str = "11111111111111111111111111111111";
const ED25519_PROGRAM: &str = "Ed25519SigVerify111111111111111111111111111";
const INSTRUCTIONS_SYSVAR: &str = "Sysvar1nstructions1111111111111111111111111";
const NATIVE_LOADER: &str = "NativeLoader1111111111111111111111111111111";

const REPO_SEED: &[u8] = b"repo";
const BRANCH_SEED: &[u8] = b"branch";
const COMMIT_SEED: &[u8] = b"commit";

/// Anchor custom error `StaleBranchHead` (declaration index 15 + 6000).
pub const STALE_BRANCH_HEAD: u32 = 6015;

/// Onchain repository snapshot.
#[derive(Debug, Clone)]
pub struct RepoAccount {
    /// Owner wallet.
    pub owner: Address,
    /// Repository PDA.
    pub repo_id: Address,
    /// NUL-padded name.
    pub name: [u8; 32],
    /// NUL-padded default branch.
    pub default_branch: [u8; 32],
    /// Append-only history root.
    pub history_root: [u8; 32],
    /// Number of anchored commits.
    pub commit_count: u64,
}

/// Onchain branch snapshot.
#[derive(Debug, Clone)]
pub struct BranchAccount {
    /// Repository PDA.
    pub repo: Address,
    /// NUL-padded name.
    pub name: [u8; 32],
    /// Tip commit oid (zero if empty).
    pub head_commit: [u8; 32],
    /// CAS token.
    pub head_seq: u64,
}

/// Onchain commit snapshot.
#[derive(Debug, Clone)]
pub struct CommitAccount {
    /// Repository PDA.
    pub repo: Address,
    /// Commit oid.
    pub commit_oid: [u8; 32],
    /// 0, 1, or 2.
    pub parent_count: u8,
    /// First parent or zeros.
    pub parent_a: [u8; 32],
    /// Second parent or zeros.
    pub parent_b: [u8; 32],
    /// Tree oid.
    pub tree_oid: [u8; 32],
    /// Author wallet.
    pub author: Address,
    /// Unix timestamp.
    pub authored_at: i64,
    /// Message hash.
    pub message_hash: [u8; 32],
    /// Attestation hash.
    pub attestation_hash: [u8; 32],
    /// History log index.
    pub seq: u64,
}

/// Chain transport: LiteSVM (tests) or JSON-RPC (local validator / devnet).
#[derive(Clone)]
pub struct Chain {
    program_id: Address,
    inner: Inner,
}

enum Inner {
    Svm(Arc<Mutex<LiteSVM>>),
    Rpc(Arc<solana_rpc_client::rpc_client::RpcClient>),
}

impl Clone for Inner {
    fn clone(&self) -> Self {
        match self {
            Self::Svm(svm) => Self::Svm(Arc::clone(svm)),
            Self::Rpc(rpc) => Self::Rpc(Arc::clone(rpc)),
        }
    }
}

impl Chain {
    /// Program id this client talks to.
    #[must_use]
    pub fn program_id(&self) -> Address {
        self.program_id
    }

    /// LiteSVM backend (program `.so` must already be built).
    ///
    /// # Errors
    /// Missing program file or airdrop failure.
    pub fn litesvm(program_so: &std::path::Path, payer: &Keypair) -> Result<Self> {
        if !program_so.exists() {
            bail!(
                "program .so not found at {} (run `anchor build`)",
                program_so.display()
            );
        }
        let mut svm = LiteSVM::new();
        svm.set_account(
            ed25519_program(),
            Account {
                lamports: 1,
                data: Vec::new(),
                owner: native_loader(),
                executable: true,
                rent_epoch: 0,
            },
        )
        .context("load ed25519 precompile")?;
        let pid = program_id();
        svm.add_program_from_file(pid, program_so)
            .context("load forge_repository")?;
        svm.airdrop(&payer.pubkey(), 100 * 1_000_000_000)
            .map_err(|err| anyhow!("airdrop: {err:?}"))?;
        Ok(Self {
            program_id: pid,
            inner: Inner::Svm(Arc::new(Mutex::new(svm))),
        })
    }

    /// JSON-RPC backend (`FORGE_RPC`, default localhost).
    #[must_use]
    pub fn rpc(url: &str) -> Self {
        Self {
            program_id: program_id_from_env(),
            inner: Inner::Rpc(Arc::new(solana_rpc_client::rpc_client::RpcClient::new(
                url.to_string(),
            ))),
        }
    }

    /// Default client: `FORGE_RPC` if set, else LiteSVM is **not** implied.
    ///
    /// # Errors
    /// Never; RPC construction is lazy.
    pub fn from_env() -> Result<Self> {
        let url = std::env::var("FORGE_RPC").unwrap_or_else(|_| "http://127.0.0.1:8899".into());
        Ok(Self::rpc(&url))
    }

    /// Airdrop on the LiteSVM backend (no-op for RPC).
    ///
    /// # Errors
    /// LiteSVM airdrop failure.
    pub fn airdrop(&self, pubkey: &Address, lamports: u64) -> Result<()> {
        match &self.inner {
            Inner::Svm(svm) => {
                svm.lock()
                    .expect("svm")
                    .airdrop(pubkey, lamports)
                    .map_err(|err| anyhow!("airdrop: {err:?}"))?;
                Ok(())
            }
            Inner::Rpc(_) => Ok(()),
        }
    }

    fn get_account_data(&self, address: &Address) -> Result<Option<Vec<u8>>> {
        match &self.inner {
            Inner::Svm(svm) => {
                let guard = svm.lock().expect("svm");
                Ok(guard.get_account(address).map(|a| a.data))
            }
            Inner::Rpc(rpc) => match rpc.get_account(address) {
                Ok(account) => Ok(Some(account.data)),
                Err(err) if format!("{err}").contains("AccountNotFound") => Ok(None),
                Err(err) => Err(err.into()),
            },
        }
    }

    /// Sends `instructions` signed by `payer`.
    ///
    /// # Errors
    /// Simulation / send failures. [`ChainError::StaleHead`] on CAS races.
    pub fn send(&self, payer: &Keypair, instructions: &[Instruction]) -> Result<String> {
        match &self.inner {
            Inner::Svm(svm) => {
                let mut guard = svm.lock().expect("svm");
                guard.expire_blockhash();
                let blockhash = guard.latest_blockhash();
                let tx = Transaction::new_signed_with_payer(
                    instructions,
                    Some(&payer.pubkey()),
                    &[payer],
                    blockhash,
                );
                match guard.send_transaction(tx) {
                    Ok(meta) => Ok(meta.signature.to_string()),
                    Err(failed) => {
                        if custom_error_code(&failed.err) == Some(STALE_BRANCH_HEAD) {
                            bail!(ChainError::StaleHead);
                        }
                        bail!("transaction failed: {:?}", failed.err)
                    }
                }
            }
            Inner::Rpc(rpc) => {
                let blockhash = rpc.get_latest_blockhash()?;
                let tx = Transaction::new_signed_with_payer(
                    instructions,
                    Some(&payer.pubkey()),
                    &[payer],
                    blockhash,
                );
                match rpc.send_and_confirm_transaction(&tx) {
                    Ok(sig) => Ok(sig.to_string()),
                    Err(err) => {
                        let text = err.to_string();
                        if text.contains("StaleBranchHead") || text.contains("6015") {
                            bail!(ChainError::StaleHead);
                        }
                        Err(err.into())
                    }
                }
            }
        }
    }

    /// Fetch a repository account.
    pub fn fetch_repo(&self, repo: &Address) -> Result<Option<RepoAccount>> {
        let Some(data) = self.get_account_data(repo)? else {
            return Ok(None);
        };
        Ok(Some(parse_repo(&data)?))
    }

    /// Fetch a branch account.
    pub fn fetch_branch(&self, branch: &Address) -> Result<Option<BranchAccount>> {
        let Some(data) = self.get_account_data(branch)? else {
            return Ok(None);
        };
        Ok(Some(parse_branch(&data)?))
    }

    /// Fetch a commit account.
    pub fn fetch_commit(&self, commit: &Address) -> Result<Option<CommitAccount>> {
        let Some(data) = self.get_account_data(commit)? else {
            return Ok(None);
        };
        Ok(Some(parse_commit(&data)?))
    }
}

/// Compare-and-swap lost the race.
#[derive(Debug, thiserror::Error)]
pub enum ChainError {
    /// `expected_head_seq` was stale; caller should `forge pull`.
    #[error("stale branch head; run `forge pull`, rebase/re-sign, and retry")]
    StaleHead,
}

/// SHA-256 of `global:<name>` truncated to 8 bytes (Anchor ix discriminator).
#[must_use]
pub fn ix_discriminator(name: &str) -> [u8; 8] {
    let digest = HashAlgorithm::Sha256.digest(format!("global:{name}").as_bytes());
    let mut out = [0u8; 8];
    out.copy_from_slice(&digest[..8]);
    out
}

/// NUL-padded 32-byte name (ADR 0002).
///
/// # Errors
/// Empty or longer than 32 bytes.
pub fn pad_name(s: &str) -> Result<[u8; 32]> {
    if s.is_empty() || s.len() > 32 {
        bail!("repository/branch name must be 1..=32 bytes");
    }
    let mut out = [0u8; 32];
    out[..s.len()].copy_from_slice(s.as_bytes());
    Ok(out)
}

/// Trim a NUL-padded name to a UTF-8 string.
#[must_use]
pub fn trim_name(buf: &[u8; 32]) -> String {
    let end = buf.iter().position(|b| *b == 0).unwrap_or(32);
    String::from_utf8_lossy(&buf[..end]).into_owned()
}

/// Program id from `FORGE_PROGRAM_ID` or the canonical constant.
#[must_use]
pub fn program_id_from_env() -> Address {
    std::env::var("FORGE_PROGRAM_ID")
        .ok()
        .and_then(|s| Address::from_str(&s).ok())
        .unwrap_or_else(program_id)
}

/// Canonical program id.
#[must_use]
pub fn program_id() -> Address {
    PROGRAM_ID_STR.parse().expect("program id")
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

fn native_loader() -> Address {
    NATIVE_LOADER.parse().unwrap()
}

fn event_authority(pid: &Address) -> Address {
    Address::find_program_address(&[b"__event_authority"], pid).0
}

/// Repository PDA `["repo", owner, name]`.
#[must_use]
pub fn repository_pda(pid: &Address, owner: &Address, name: &[u8; 32]) -> Address {
    Address::find_program_address(&[REPO_SEED, owner.as_ref(), name.as_ref()], pid).0
}

/// Branch PDA `["branch", repo, name]`.
#[must_use]
pub fn branch_pda(pid: &Address, repo: &Address, name: &[u8; 32]) -> Address {
    Address::find_program_address(&[BRANCH_SEED, repo.as_ref(), name.as_ref()], pid).0
}

/// Commit PDA `["commit", repo, oid]`.
#[must_use]
pub fn commit_pda(pid: &Address, repo: &Address, oid: &[u8; 32]) -> Address {
    Address::find_program_address(&[COMMIT_SEED, repo.as_ref(), oid.as_ref()], pid).0
}

/// Default program `.so` path relative to the CLI crate.
#[must_use]
pub fn default_program_so() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../target/deploy/forge_repository.so")
}

/// Native Ed25519 verify instruction (offsets after the 16-byte header).
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn ed25519_verify_instruction(
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

/// `initialize_repository` accounts + data.
pub fn initialize_instruction(
    pid: &Address,
    owner: &Address,
    repo_name: [u8; 32],
    default_branch: [u8; 32],
) -> Instruction {
    let repo = repository_pda(pid, owner, &repo_name);
    let branch = branch_pda(pid, &repo, &default_branch);
    let mut data = Vec::new();
    data.extend_from_slice(&ix_discriminator("initialize_repository"));
    data.extend_from_slice(&repo_name);
    data.extend_from_slice(&default_branch);
    data.push(2); // hybrid
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

/// `create_commit` (parents that are unused point at the system program).
#[allow(clippy::too_many_arguments)]
pub fn create_commit_instruction(
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
) -> Instruction {
    let commit = commit_pda(pid, &repository, &commit_oid);
    let parent_a_account = if parent_count >= 1 {
        commit_pda(pid, &repository, &parent_a)
    } else {
        system_program()
    };
    let parent_b_account = if parent_count == 2 {
        commit_pda(pid, &repository, &parent_b)
    } else {
        system_program()
    };
    let mut data = Vec::new();
    data.extend_from_slice(&ix_discriminator("create_commit"));
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

/// `update_branch`.
pub fn update_branch_instruction(
    pid: &Address,
    authority: &Address,
    repo: Address,
    branch_name: [u8; 32],
    new_head: [u8; 32],
    expected_seq: u64,
) -> Instruction {
    let branch = branch_pda(pid, &repo, &branch_name);
    let new_commit = commit_pda(pid, &repo, &new_head);
    let mut data = Vec::new();
    data.extend_from_slice(&ix_discriminator("update_branch"));
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

/// Convert an [`Oid`] to the 32-byte onchain form.
#[must_use]
pub fn oid32(oid: &Oid) -> [u8; 32] {
    oid.to_bytes32()
}

fn parse_repo(data: &[u8]) -> Result<RepoAccount> {
    let mut s = skip_disc(data)?;
    Ok(RepoAccount {
        owner: Address::new_from_array(take32(&mut s)?),
        repo_id: Address::new_from_array(take32(&mut s)?),
        name: take32(&mut s)?,
        default_branch: take32(&mut s)?,
        history_root: take32(&mut s)?,
        commit_count: take_u64(&mut s)?,
    })
}

fn parse_branch(data: &[u8]) -> Result<BranchAccount> {
    let mut s = skip_disc(data)?;
    Ok(BranchAccount {
        repo: Address::new_from_array(take32(&mut s)?),
        name: take32(&mut s)?,
        head_commit: take32(&mut s)?,
        head_seq: take_u64(&mut s)?,
    })
}

fn parse_commit(data: &[u8]) -> Result<CommitAccount> {
    let mut s = skip_disc(data)?;
    Ok(CommitAccount {
        repo: Address::new_from_array(take32(&mut s)?),
        commit_oid: take32(&mut s)?,
        parent_count: take_u8(&mut s)?,
        parent_a: take32(&mut s)?,
        parent_b: take32(&mut s)?,
        tree_oid: take32(&mut s)?,
        author: Address::new_from_array(take32(&mut s)?),
        authored_at: take_i64(&mut s)?,
        message_hash: take32(&mut s)?,
        attestation_hash: take32(&mut s)?,
        seq: take_u64(&mut s)?,
    })
}

fn skip_disc(data: &[u8]) -> Result<&[u8]> {
    if data.len() < 8 {
        bail!("account too small");
    }
    Ok(&data[8..])
}

fn take32(buf: &mut &[u8]) -> Result<[u8; 32]> {
    if buf.len() < 32 {
        bail!("truncated account");
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&buf[..32]);
    *buf = &buf[32..];
    Ok(out)
}

fn take_u8(buf: &mut &[u8]) -> Result<u8> {
    let b = *buf.first().ok_or_else(|| anyhow!("truncated account"))?;
    *buf = &buf[1..];
    Ok(b)
}

fn take_u64(buf: &mut &[u8]) -> Result<u64> {
    if buf.len() < 8 {
        bail!("truncated account");
    }
    let mut raw = [0u8; 8];
    raw.copy_from_slice(&buf[..8]);
    *buf = &buf[8..];
    Ok(u64::from_le_bytes(raw))
}

fn take_i64(buf: &mut &[u8]) -> Result<i64> {
    Ok(i64::from_le_bytes(take_u64(buf)?.to_le_bytes()))
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

/// Parse a base58 address.
pub fn parse_address(s: &str) -> Result<Address> {
    Address::from_str(s).map_err(|err| anyhow!("invalid address {s}: {err}"))
}

/// 32-byte oid as a Forge tagged string.
pub fn oid_from_bytes32(bytes: &[u8; 32]) -> Result<Oid> {
    Oid::new(HashAlgorithm::Sha256, bytes.to_vec()).map_err(Into::into)
}
