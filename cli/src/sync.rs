//! Onchain sync: push / clone / pull / verify (Phase 8, §13).

use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use forge_object::branch::branch_update_message;
use forge_object::commit::Commit;
use forge_object::hash::{HashAlgorithm, Oid};
use forge_object::history::history_root_chain;
use forge_object::object::ObjectType;
use forge_storage::graph::ObjectGraph;
use solana_keypair::Keypair;
use solana_message::Address;
use solana_signer::Signer;

use crate::attest;
use crate::chain::{
    self, oid32, oid_from_bytes32, pad_name, parse_address, trim_name, BranchAccount, Chain,
    ChainError, RepoAccount,
};
use crate::config::{self, ForgeConfig, OnchainBranchRef, OnchainRefs};
use crate::git::{self, GitRepo};
use crate::storage;
use crate::wallet;
use crate::VerifyStatus;

thread_local! {
    static TEST_CHAIN: RefCell<Option<Chain>> = const { RefCell::new(None) };
}

/// Inject a LiteSVM chain for in-process tests.
pub fn set_test_chain(chain: Option<Chain>) {
    TEST_CHAIN.with(|slot| *slot.borrow_mut() = chain);
}

/// The active chain (injected test chain, else `FORGE_RPC`).
///
/// # Errors
/// RPC construction errors.
pub fn active_chain() -> Result<Chain> {
    TEST_CHAIN
        .with(|slot| slot.borrow().clone())
        .map_or_else(Chain::from_env, Ok)
}

fn resolve_pda(config: &ForgeConfig, remote: Option<&str>) -> Result<Option<Address>> {
    if let Some(name) = remote {
        if let Some(r) = config.remotes.get(name) {
            if r.repo.is_empty() || r.repo == "local" {
                return Ok(None);
            }
            return Ok(Some(parse_address(&r.repo)?));
        }
        return Ok(Some(parse_address(name)?));
    }
    if !config.repo_pda.is_empty() {
        return Ok(Some(parse_address(&config.repo_pda)?));
    }
    if let Some(origin) = config.remotes.get("origin") {
        if !origin.repo.is_empty() && origin.repo != "local" {
            return Ok(Some(parse_address(&origin.repo)?));
        }
    }
    Ok(None)
}

fn cache_refs(
    root: &Path,
    repo: &Address,
    repo_state: &RepoAccount,
    branch_name: &str,
    branch: &BranchAccount,
) -> Result<()> {
    let mut refs = OnchainRefs::load(root).unwrap_or_default();
    refs.version = 1;
    refs.repository = repo.to_string();
    refs.commit_count = repo_state.commit_count;
    refs.history_root = oid_from_bytes32(&repo_state.history_root)?.to_tagged_string();
    refs.branches.insert(
        branch_name.to_string(),
        OnchainBranchRef {
            head: if branch.head_commit == [0u8; 32] {
                String::new()
            } else {
                oid_from_bytes32(&branch.head_commit)?.to_tagged_string()
            },
            head_seq: branch.head_seq,
        },
    );
    refs.save(root)?;
    Ok(())
}

fn ensure_attestation(
    root: &Path,
    repo_pda: &str,
    commit: &Commit,
    wallet: &Keypair,
) -> Result<([u8; 32], [u8; 64])> {
    if let Ok((att, sig)) = attest::load_sidecar(root, &commit.oid()) {
        if att.repo == repo_pda && att.commit == commit.oid() {
            let hash = att.attestation_hash().to_bytes32();
            let sig_bytes: [u8; 64] = sig
                .as_ref()
                .try_into()
                .map_err(|_| anyhow!("signature length"))?;
            return Ok((hash, sig_bytes));
        }
    }
    let nonce = attest::random_nonce(wallet);
    let (att, sig) = attest::write_sidecar(root, repo_pda, commit, wallet, &nonce)?;
    let hash = att.attestation_hash().to_bytes32();
    let sig_bytes: [u8; 64] = sig
        .as_ref()
        .try_into()
        .map_err(|_| anyhow!("signature length"))?;
    Ok((hash, sig_bytes))
}

/// Upload missing objects, `create_commit` each, then `update_branch` (§13).
pub fn push(remote: Option<String>, branch: Option<String>) -> Result<()> {
    let git = GitRepo::discover(&std::env::current_dir()?)?;
    let mut config = ForgeConfig::load(&git.root)?;
    let wallet = wallet::load_repo_wallet(&git.root, &config.wallet)?;
    let chain = active_chain()?;
    let pid = chain.program_id();
    let branch_name = branch.unwrap_or_else(|| git.current_branch());
    let padded_branch = pad_name(&branch_name)?;
    let padded_repo_name = pad_name(&config.name)?;

    let mut repo_pda = resolve_pda(&config, remote.as_deref())?;
    if repo_pda.is_none() {
        let derived = chain::repository_pda(&pid, &wallet.pubkey(), &padded_repo_name);
        let ix = chain::initialize_instruction(
            &pid,
            &wallet.pubkey(),
            padded_repo_name,
            pad_name(&config.default_branch)?,
        );
        chain.send(&wallet, &[ix])?;
        repo_pda = Some(derived);
        config.repo_pda = derived.to_string();
        config
            .remotes
            .entry("origin".into())
            .or_insert(config::Remote {
                repo: derived.to_string(),
            });
        config.save(&git.root)?;
        println!("initialized repository {derived}");
    }
    let repo = repo_pda.expect("pda");

    let Some(repo_state) = chain.fetch_repo(&repo)? else {
        bail!("repository account {repo} not found");
    };
    let branch_addr = chain::branch_pda(&pid, &repo, &padded_branch);
    let Some(branch_state) = chain.fetch_branch(&branch_addr)? else {
        bail!("branch {branch_name} not found onchain");
    };

    let local_tip = git
        .head_oid(HashAlgorithm::Sha256)?
        .ok_or_else(|| anyhow!("nothing to push (unborn HEAD)"))?;

    let rows = git.log(Some(&branch_name), HashAlgorithm::Sha256)?;
    let mut to_push: Vec<Oid> = Vec::new();
    for (oid, _) in rows.into_iter().rev() {
        let acct = chain::commit_pda(&pid, &repo, &oid32(&oid));
        if chain.fetch_commit(&acct)?.is_none() {
            to_push.push(oid);
        }
    }
    if to_push.is_empty() && branch_state.head_commit == oid32(&local_tip) {
        println!("everything up-to-date");
        cache_refs(&git.root, &repo, &repo_state, &branch_name, &branch_state)?;
        return Ok(());
    }

    let mut pins = storage::open_pins(&git.root)?;
    let graph = ObjectGraph::collect_reachable(&git, &local_tip, HashAlgorithm::Sha256)?;
    let locators = pins.upload(&graph, &local_tip)?;
    println!(
        "uploaded bundle ({} objects, {} locators)",
        graph.len(),
        locators.len()
    );

    for oid in &to_push {
        let commit = git::load_commit(&git, oid)?;
        storage::publish_sidecars(&git.root, &pins, oid)?;
        let (att_hash, sig) = ensure_attestation(&git.root, &repo.to_string(), &commit, &wallet)?;
        storage::publish_sidecars(&git.root, &pins, oid)?;
        let parent_count = u8::try_from(commit.parents.len())?;
        let parent_a = commit.parents.first().map_or([0u8; 32], oid32);
        let parent_b = commit.parents.get(1).map_or([0u8; 32], oid32);
        let message_hash = Oid::new(
            commit.algorithm(),
            commit.algorithm().digest(&commit.message),
        )?;
        let ed = chain::ed25519_verify_instruction(&att_hash, &sig, &wallet.pubkey().to_bytes());
        let ix = chain::create_commit_instruction(
            &pid,
            &wallet.pubkey(),
            repo,
            oid32(&commit.oid()),
            parent_count,
            parent_a,
            parent_b,
            oid32(&commit.tree),
            commit.author.timestamp,
            message_hash.to_bytes32(),
            att_hash,
        );
        let sig_tx = chain.send(&wallet, &[ed, ix])?;
        println!("create_commit {}  tx {sig_tx}", oid.to_tagged_string());
    }

    let expected = branch_state.head_seq;
    let update_msg = branch_update_message(&repo.to_bytes(), &padded_branch, &local_tip, expected);
    let branch_sig = wallet.sign_message(&update_msg);
    let branch_sig_bytes: [u8; 64] = branch_sig
        .as_ref()
        .try_into()
        .map_err(|_| anyhow!("signature length"))?;
    let ed = chain::ed25519_verify_instruction(
        &update_msg,
        &branch_sig_bytes,
        &wallet.pubkey().to_bytes(),
    );
    let ix = chain::update_branch_instruction(
        &pid,
        &wallet.pubkey(),
        repo,
        padded_branch,
        oid32(&local_tip),
        expected,
    );
    let tx = match chain.send(&wallet, &[ed, ix]) {
        Ok(s) => s,
        Err(err) => {
            if err.downcast_ref::<ChainError>().is_some()
                || err.to_string().contains("stale branch head")
            {
                bail!(ChainError::StaleHead);
            }
            return Err(err);
        }
    };
    let Some(repo_after) = chain.fetch_repo(&repo)? else {
        bail!("repository disappeared");
    };
    let Some(branch_after) = chain.fetch_branch(&branch_addr)? else {
        bail!("branch disappeared");
    };
    println!(
        "update_branch {branch_name} (seq {} → {})  tx {tx}",
        expected, branch_after.head_seq
    );
    cache_refs(&git.root, &repo, &repo_after, &branch_name, &branch_after)?;
    Ok(())
}

/// Clone `repo` PDA into `dir`.
pub fn clone(repo_s: String, dir: Option<String>) -> Result<()> {
    let dest = PathBuf::from(dir.unwrap_or_else(|| repo_s.clone()));
    let chain = active_chain()?;
    let pid = chain.program_id();
    let repo = parse_address(&repo_s)?;
    let Some(repo_state) = chain.fetch_repo(&repo)? else {
        bail!("repository {repo} not found");
    };
    let branch_name = trim_name(&repo_state.default_branch);
    let branch_addr = chain::branch_pda(&pid, &repo, &repo_state.default_branch);
    let Some(branch_state) = chain.fetch_branch(&branch_addr)? else {
        bail!("default branch missing");
    };

    let git = GitRepo::init(&dest, &branch_name)?;
    let wallet_rel = wallet::DEFAULT_WALLET;
    let wallet_path = git.root.join(wallet_rel);
    if !wallet_path.exists() {
        wallet::save_keypair(&wallet_path, &Keypair::new())?;
    }
    let mut config = ForgeConfig::new_local(wallet_rel, &branch_name);
    config.name = trim_name(&repo_state.name);
    config.repo_pda = repo.to_string();
    config.remotes.insert(
        "origin".into(),
        config::Remote {
            repo: repo.to_string(),
        },
    );
    config.save(&git.root)?;
    config::write_empty_onchain_refs(&git.root)?;

    if branch_state.head_commit == [0u8; 32] {
        cache_refs(&git.root, &repo, &repo_state, &branch_name, &branch_state)?;
        println!("Cloned empty repository {repo} into {}", git.root.display());
        return Ok(());
    }

    let tip = oid_from_bytes32(&branch_state.head_commit)?;
    let pins = storage::open_pins(&git.root)?;
    let graph = ObjectGraph::collect_reachable(
        &storage::PinSource::new(&pins),
        &tip,
        HashAlgorithm::Sha256,
    )?;
    git.write_graph(&graph)?;
    for obj in graph.objects() {
        if obj.object_type == ObjectType::Commit {
            storage::import_sidecars(&git.root, &pins, &obj.oid)?;
        }
    }
    git.checkout_branch(&branch_name, &tip)?;
    cache_refs(&git.root, &repo, &repo_state, &branch_name, &branch_state)?;
    println!(
        "Cloned {} into {}  HEAD {}",
        repo,
        git.root.display(),
        tip.to_tagged_string()
    );
    Ok(())
}

/// Fast-forward from the onchain default/current branch.
pub fn pull(remote: Option<String>) -> Result<()> {
    let git = GitRepo::discover(&std::env::current_dir()?)?;
    let config = ForgeConfig::load(&git.root)?;
    let chain = active_chain()?;
    let pid = chain.program_id();
    let Some(repo) = resolve_pda(&config, remote.as_deref())? else {
        bail!("no onchain remote; `forge remote add` or push first");
    };
    let Some(repo_state) = chain.fetch_repo(&repo)? else {
        bail!("repository {repo} not found");
    };
    let branch_name = git.current_branch();
    let padded = pad_name(&branch_name)?;
    let branch_addr = chain::branch_pda(&pid, &repo, &padded);
    let Some(branch_state) = chain.fetch_branch(&branch_addr)? else {
        bail!("branch {branch_name} not found onchain");
    };
    cache_refs(&git.root, &repo, &repo_state, &branch_name, &branch_state)?;
    if branch_state.head_commit == [0u8; 32] {
        println!("onchain branch {branch_name} is empty");
        return Ok(());
    }
    let tip = oid_from_bytes32(&branch_state.head_commit)?;
    let pins = storage::open_pins(&git.root)?;
    let graph = ObjectGraph::collect_reachable(
        &storage::PinSource::new(&pins),
        &tip,
        HashAlgorithm::Sha256,
    )?;
    git.write_graph(&graph)?;
    for obj in graph.objects() {
        if obj.object_type == ObjectType::Commit {
            storage::import_sidecars(&git.root, &pins, &obj.oid)?;
        }
    }
    if let Some(local) = git.head_oid(HashAlgorithm::Sha256)? {
        if local == tip {
            println!("already up to date");
            return Ok(());
        }
        if git.is_ancestor(&local, &tip) {
            git.checkout_branch(&branch_name, &tip)?;
            println!("fast-forwarded to {}", tip.to_tagged_string());
            return Ok(());
        }
        if git.is_ancestor(&tip, &local) {
            println!("local is ahead of onchain; `forge push` to publish");
            return Ok(());
        }
        bail!("histories have diverged; rebase or reset locally, then push");
    }
    git.checkout_branch(&branch_name, &tip)?;
    println!("checked out {}", tip.to_tagged_string());
    Ok(())
}

fn verify_history_inclusion(
    chain: &Chain,
    repo: &Address,
    repo_state: &RepoAccount,
    start: &Oid,
) -> Result<()> {
    let pid = chain.program_id();
    let mut stack = vec![start.clone()];
    let mut seen = HashSet::new();
    let mut by_seq: BTreeMap<u64, Oid> = BTreeMap::new();
    while let Some(oid) = stack.pop() {
        if !seen.insert(oid.to_tagged_string()) {
            continue;
        }
        let addr = chain::commit_pda(&pid, repo, &oid32(&oid));
        let Some(acct) = chain.fetch_commit(&addr)? else {
            return Err(VerifyStatus::MissingData
                .with(format!("commit {} not onchain", oid.to_tagged_string()))
                .into());
        };
        if acct.commit_oid != oid32(&oid) {
            return Err(VerifyStatus::Mismatch
                .with("onchain commit oid does not match")
                .into());
        }
        by_seq.insert(acct.seq, oid.clone());
        if acct.parent_count >= 1 {
            stack.push(oid_from_bytes32(&acct.parent_a)?);
        }
        if acct.parent_count == 2 {
            stack.push(oid_from_bytes32(&acct.parent_b)?);
        }
    }
    if u64::try_from(by_seq.len())? != repo_state.commit_count {
        return Err(VerifyStatus::Mismatch
            .with(format!(
                "walked {} commits but onchain commit_count is {}",
                by_seq.len(),
                repo_state.commit_count
            ))
            .into());
    }
    let pairs: Vec<(Oid, u64)> = by_seq.into_iter().map(|(seq, oid)| (oid, seq)).collect();
    let expected = history_root_chain(HashAlgorithm::Sha256, &repo.to_bytes(), &pairs)?;
    if expected.to_bytes32() != repo_state.history_root {
        return Err(VerifyStatus::Mismatch
            .with(format!(
                "history_root mismatch: client {} onchain {}",
                expected.to_tagged_string(),
                oid_from_bytes32(&repo_state.history_root)?.to_tagged_string()
            ))
            .into());
    }
    Ok(())
}

/// Local OIDs + signature, plus history inclusion when a PDA is configured.
pub fn verify(git: &GitRepo, config: &ForgeConfig, commit: Option<String>) -> Result<()> {
    let algorithm = HashAlgorithm::Sha256;
    let oid = match commit {
        Some(s) => {
            if let Ok(tagged) = Oid::parse_tagged(&s) {
                tagged
            } else {
                Oid::from_hex(algorithm, &s)?
            }
        }
        None => match git.head_oid(algorithm)? {
            Some(oid) => oid,
            None => {
                return Err(VerifyStatus::MissingData.with("HEAD has no commit").into());
            }
        },
    };
    let commit = match git::load_commit(git, &oid) {
        Ok(c) => c,
        Err(err) => return Err(VerifyStatus::Mismatch.with(err.to_string()).into()),
    };
    git::verify_tree(git, &commit.tree, algorithm)
        .map_err(|err| -> anyhow::Error { VerifyStatus::Mismatch.with(err.to_string()).into() })?;
    let (attestation, signature) = match attest::load_sidecar(&git.root, &oid) {
        Ok(pair) => pair,
        Err(err) => {
            return Err(VerifyStatus::MissingData.with(err.to_string()).into());
        }
    };
    attest::verify_local(&commit, &attestation, &signature)
        .map_err(|err| -> anyhow::Error { VerifyStatus::Mismatch.with(err.to_string()).into() })?;

    let pda = resolve_pda(config, None)?;
    if pda.is_none() {
        println!(
            "LOCAL_VERIFIED  commit={}  author={}  tree={}",
            oid.to_tagged_string(),
            attestation.author,
            commit.tree.to_tagged_string()
        );
        return Ok(());
    }
    let repo = pda.expect("pda");
    let chain = active_chain()?;
    let Some(repo_state) = chain.fetch_repo(&repo)? else {
        return Err(VerifyStatus::MissingData
            .with("repository account missing")
            .into());
    };
    let commit_acct = chain::commit_pda(&chain.program_id(), &repo, &oid32(&oid));
    let Some(onchain) = chain.fetch_commit(&commit_acct)? else {
        return Err(VerifyStatus::MissingData
            .with("commit is not anchored onchain")
            .into());
    };
    if onchain.attestation_hash != attestation.attestation_hash().to_bytes32() {
        return Err(VerifyStatus::Mismatch
            .with("onchain attestation_hash does not match sidecar")
            .into());
    }
    if onchain.tree_oid != oid32(&commit.tree) {
        return Err(VerifyStatus::Mismatch
            .with("onchain tree_oid mismatch")
            .into());
    }
    verify_history_inclusion(&chain, &repo, &repo_state, &oid)?;
    println!(
        "VERIFIED  commit={}  author={}  tree={}  history_root={}  commit_count={}",
        oid.to_tagged_string(),
        attestation.author,
        commit.tree.to_tagged_string(),
        oid_from_bytes32(&repo_state.history_root)?.to_tagged_string(),
        repo_state.commit_count
    );
    Ok(())
}

/// Refresh `.forge/onchain-refs` and print divergence.
pub fn status_onchain(git: &GitRepo, config: &ForgeConfig) -> Result<()> {
    let refs = OnchainRefs::load(&git.root)?;
    let local = git.head_oid(HashAlgorithm::Sha256)?;
    let branch = git.current_branch();
    if let Some(cached) = refs.branches.get(&branch) {
        match (local, Oid::parse_tagged(&cached.head).ok()) {
            (Some(l), Some(r)) if l == r => {
                println!("onchain {branch}: in sync (seq {})", cached.head_seq);
            }
            (Some(l), Some(r)) => println!(
                "onchain {branch}: local {}  remote {}  seq {}",
                l.to_tagged_string(),
                r.to_tagged_string(),
                cached.head_seq
            ),
            (Some(l), None) => println!(
                "onchain {branch}: local {}  remote empty  seq {}",
                l.to_tagged_string(),
                cached.head_seq
            ),
            (None, Some(r)) => println!(
                "onchain {branch}: unborn local  remote {}  seq {}",
                r.to_tagged_string(),
                cached.head_seq
            ),
            (None, None) => println!("onchain {branch}: empty (seq {})", cached.head_seq),
        }
    } else if config.repo_pda.is_empty() {
        println!("onchain: no cached refs (push to initialize)");
    } else {
        println!("onchain-refs: no entry for {branch}");
    }
    if let Ok(chain) = active_chain() {
        if let Ok(Some(repo)) = resolve_pda(config, None) {
            if let Ok(Some(state)) = chain.fetch_repo(&repo) {
                println!(
                    "onchain commit_count={} history_root={}",
                    state.commit_count,
                    oid_from_bytes32(&state.history_root)
                        .map(|o| o.to_tagged_string())
                        .unwrap_or_default()
                );
            }
        }
    }
    Ok(())
}
