//! CLI command dispatch (Phases 6–7).

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use forge_object::commit::Commit;
use forge_object::hash::HashAlgorithm;
use forge_object::tag::tag_message;
use forge_object::Oid;
use solana_keypair::Keypair;
use solana_signer::Signer;

use crate::attest;
use crate::chain::{self, parse_address};
use crate::config::{self, ForgeConfig};
use crate::git::{self, GitRepo};
use crate::sync;
use crate::wallet;
use crate::{Command, PermissionsAction, RemoteAction, VerifyStatus};

/// Dispatch a parsed command.
#[allow(clippy::needless_pass_by_value)]
pub fn dispatch(command: Command) -> Result<()> {
    match command {
        Command::Init { dir } => init(dir),
        Command::Add { paths } => add(paths),
        Command::Commit { message } => commit(&message),
        Command::Status => status(),
        Command::Log { branch } => log(branch),
        Command::Branch { name } => branch(name),
        Command::Checkout { r#ref } => checkout(&r#ref),
        Command::Remote { action } => remote(action),
        Command::Verify { commit } => verify(commit),
        Command::Push { remote, branch } => sync::push(remote, branch),
        Command::Pull { remote } => sync::pull(remote),
        Command::Clone { repo, dir } => sync::clone(repo, dir),
        Command::Tag { name, checkpoint } => tag(&name, checkpoint),
        Command::Merge { branch } => merge(&branch),
        Command::VerifyProgram { program_id } => verify_program(&program_id),
        Command::Permissions { action } => permissions(action),
        Command::Gc {
            verify_availability,
        } => gc(verify_availability),
    }
}

fn init(dir: Option<String>) -> Result<()> {
    let root = match dir {
        Some(d) => PathBuf::from(d),
        None => std::env::current_dir()?,
    };
    let repo = GitRepo::init(&root, "main")?;
    let _opened = gix::open(&repo.root).context("gix failed to open the new repository")?;
    let wallet_rel = wallet::DEFAULT_WALLET;
    let wallet_path = repo.root.join(wallet_rel);
    if !wallet_path.exists() {
        wallet::save_keypair(&wallet_path, &Keypair::new())?;
    }
    let config = ForgeConfig::new_local(wallet_rel, "main");
    config.save(&repo.root)?;
    config::write_empty_onchain_refs(&repo.root)?;
    ensure_wallet_gitignored(&repo.root)?;
    println!(
        "Initialized empty Forge repository in {} (SHA-256, branch main)",
        repo.root.display()
    );
    Ok(())
}

fn ensure_wallet_gitignored(root: &Path) -> Result<()> {
    let gi = root.join(".gitignore");
    let line = ".forge/id.json";
    if gi.exists() {
        let existing = std::fs::read_to_string(&gi)?;
        if existing.lines().any(|l| l.trim() == line) {
            return Ok(());
        }
        let mut next = existing;
        if !next.ends_with('\n') && !next.is_empty() {
            next.push('\n');
        }
        next.push_str(line);
        next.push('\n');
        std::fs::write(gi, next)?;
    } else {
        std::fs::write(gi, format!("{line}\n"))?;
    }
    Ok(())
}

#[allow(clippy::needless_pass_by_value)]
fn add(paths: Vec<String>) -> Result<()> {
    let repo = GitRepo::discover(&std::env::current_dir()?)?;
    let _ = ForgeConfig::load(&repo.root)?;
    repo.add(&paths)?;
    Ok(())
}

fn commit(message: &str) -> Result<()> {
    let repo = GitRepo::discover(&std::env::current_dir()?)?;
    let config = ForgeConfig::load(&repo.root)?;
    let wallet = wallet::load_repo_wallet(&repo.root, &config.wallet)?;
    let mut message = message.as_bytes().to_vec();
    if !message.ends_with(b"\n") {
        message.push(b'\n');
    }
    let tree = repo.write_index_tree()?;
    let algorithm = HashAlgorithm::Sha256;
    let parents = match repo.head_oid(algorithm)? {
        Some(head) => vec![head],
        None => Vec::new(),
    };
    let ts = i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock")?
            .as_secs(),
    )?;
    let identity = git::display_identity(&wallet.pubkey().to_string(), ts)?;
    let commit = Commit::new(
        algorithm,
        tree,
        parents,
        identity.clone(),
        identity,
        message,
    )?;
    let oid = repo.commit_onto_head(&commit)?;
    let nonce = attest::random_nonce(&wallet);
    attest::write_sidecar(
        &repo.root,
        config.attestation_repo(),
        &commit,
        &wallet,
        &nonce,
    )?;
    println!("{}", oid.to_tagged_string());
    Ok(())
}

fn status() -> Result<()> {
    let repo = GitRepo::discover(&std::env::current_dir()?)?;
    let _ = ForgeConfig::load(&repo.root)?;
    let branch = repo.current_branch();
    println!("On branch {branch}");
    let short = repo.status_short()?;
    if short.is_empty() {
        println!("nothing to commit, working tree clean");
    } else {
        println!("{short}");
    }
    let refs = config::onchain_refs_path(&repo.root);
    if refs.exists() {
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(refs)?)?;
        let empty = v
            .get("branches")
            .and_then(serde_json::Value::as_object)
            .is_none_or(serde_json::Map::is_empty);
        if empty {
            println!("onchain: no cached refs (push to initialize)");
        }
    }
    let config = ForgeConfig::load(&repo.root)?;
    sync::status_onchain(&repo, &config)?;
    Ok(())
}

#[allow(clippy::needless_pass_by_value)]
fn log(branch: Option<String>) -> Result<()> {
    let repo = GitRepo::discover(&std::env::current_dir()?)?;
    let _ = ForgeConfig::load(&repo.root)?;
    let rows = repo.log(branch.as_deref(), HashAlgorithm::Sha256)?;
    if rows.is_empty() {
        println!("no commits");
        return Ok(());
    }
    for (oid, subject) in rows {
        let author = match attest::load_sidecar(&repo.root, &oid) {
            Ok((att, _)) => att.author,
            Err(_) => "unsigned".into(),
        };
        println!("{}  {subject}  author={author}", oid.to_tagged_string());
    }
    Ok(())
}

fn branch(name: Option<String>) -> Result<()> {
    let repo = GitRepo::discover(&std::env::current_dir()?)?;
    let _ = ForgeConfig::load(&repo.root)?;
    match name {
        None => {
            let listed = repo.list_branches()?;
            if listed.is_empty() {
                println!("* {} (unborn)", repo.current_branch());
            } else {
                println!("{listed}");
            }
            Ok(())
        }
        Some(name) => {
            repo.create_branch(&name)?;
            println!("Created branch {name}");
            Ok(())
        }
    }
}

fn checkout(git_ref: &str) -> Result<()> {
    let repo = GitRepo::discover(&std::env::current_dir()?)?;
    let _ = ForgeConfig::load(&repo.root)?;
    repo.checkout(git_ref)?;
    println!("Checked out {git_ref}");
    Ok(())
}

fn remote(action: RemoteAction) -> Result<()> {
    let repo = GitRepo::discover(&std::env::current_dir()?)?;
    let mut config = ForgeConfig::load(&repo.root)?;
    match action {
        RemoteAction::Add { name, repo: target } => {
            if config.remotes.contains_key(&name) {
                bail!("remote {name} already exists");
            }
            if config.repo_pda.is_empty() {
                config.repo_pda.clone_from(&target);
            }
            config
                .remotes
                .insert(name.clone(), config::Remote { repo: target });
            config.save(&repo.root)?;
            println!("Added remote {name}");
        }
        RemoteAction::List => {
            if config.remotes.is_empty() {
                println!("(no remotes)");
            } else {
                for (name, remote) in &config.remotes {
                    println!("{name}  {}", remote.repo);
                }
            }
        }
        RemoteAction::Remove { name } => {
            if config.remotes.remove(&name).is_none() {
                bail!("unknown remote {name}");
            }
            config.save(&repo.root)?;
            println!("Removed remote {name}");
        }
    }
    Ok(())
}

fn verify(commit: Option<String>) -> Result<()> {
    let repo = GitRepo::discover(&std::env::current_dir()?)?;
    let config = ForgeConfig::load(&repo.root)?;
    sync::verify(&repo, &config, commit)
}

fn gc(verify_availability: bool) -> Result<()> {
    if !verify_availability {
        bail!(
            "object pruning is not implemented; pass --verify-availability to report unbacked objects (§8.3)"
        );
    }
    let dir = std::env::current_dir()?;
    let report = crate::storage::report_for_forge_dir(&dir, &[])?;
    print!("{}", report.render());
    if !report.fully_available() {
        bail!("one or more objects are unbacked, degraded, or corrupt");
    }
    Ok(())
}

fn tag(name: &str, checkpoint: bool) -> Result<()> {
    let repo = GitRepo::discover(&std::env::current_dir()?)?;
    let config = ForgeConfig::load(&repo.root)?;
    let wallet = wallet::load_repo_wallet(&repo.root, &config.wallet)?;
    let chain = sync::active_chain()?;
    let pid = chain.program_id();
    if config.repo_pda.is_empty() {
        bail!("no onchain repository configured; run `forge push` first");
    }
    let repo_pda = parse_address(&config.repo_pda)?;
    let target = repo
        .head_oid(HashAlgorithm::Sha256)?
        .ok_or_else(|| anyhow!("nothing to tag (unborn HEAD)"))?;
    let padded = chain::pad_name(name)?;
    repo.create_tag(name, &target)?;

    let message = format!("forge release {name}\n");
    let message_hash = Oid::new(
        HashAlgorithm::Sha256,
        HashAlgorithm::Sha256.digest(message.as_bytes()),
    )?;
    let msg = tag_message(&repo_pda.to_bytes(), &padded, &target, &message_hash);
    let sig = wallet.sign_message(&msg);
    let sig_bytes: [u8; 64] = sig
        .as_ref()
        .try_into()
        .map_err(|_| anyhow!("signature length"))?;
    let ed = chain::ed25519_verify_instruction(&msg, &sig_bytes, &wallet.pubkey().to_bytes());
    let ix = chain::create_tag_instruction(
        &pid,
        &wallet.pubkey(),
        repo_pda,
        padded,
        chain::oid32(&target),
        message_hash.to_bytes32(),
    );
    let tx = chain.send(&wallet, &[ed, ix])?;
    println!("tag {name} -> {}  tx {tx}", target.to_tagged_string());
    if checkpoint {
        write_checkpoint(&repo.root, name, &repo_pda, &target, &message_hash)?;
    }
    Ok(())
}

fn merge(branch: &str) -> Result<()> {
    let repo = GitRepo::discover(&std::env::current_dir()?)?;
    let config = ForgeConfig::load(&repo.root)?;
    let wallet = wallet::load_repo_wallet(&repo.root, &config.wallet)?;
    let ts = i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock")?
            .as_secs(),
    )?;
    repo.merge_no_ff(branch, &wallet.pubkey().to_string(), "forge@localhost", ts)?;
    println!("merged {branch} locally; pushing merged history");
    sync::push(None, None)
}

fn verify_program(program_id_s: &str) -> Result<()> {
    let program = parse_address(program_id_s)?;
    let chain = sync::active_chain()?;
    let pid = chain.program_id();
    let addr = chain::program_attestation_pda(&pid, &program);
    let Some(claim) = chain.fetch_program_attestation(&addr)? else {
        return Err(VerifyStatus::MissingData
            .with(format!("no provenance claim for {program}"))
            .into());
    };
    let commit_hex = Oid::new(HashAlgorithm::Sha256, claim.commit_oid.to_vec())?.to_tagged_string();

    // The claimed commit must be anchored in the claimed repository.
    let commit_acct = chain::commit_pda(&pid, &claim.repo, &claim.commit_oid);
    if chain.fetch_commit(&commit_acct)?.is_none() {
        println!("MISMATCH program={program} reason=claimed commit not anchored");
        return Err(VerifyStatus::Mismatch
            .with("claimed commit is not anchored in the repository")
            .into());
    }

    match compute_artifact_hash() {
        Some(hash) if hash == claim.artifact_hash => {
            let artifact =
                Oid::new(HashAlgorithm::Sha256, claim.artifact_hash.to_vec())?.to_tagged_string();
            println!(
                "VERIFIED program={program} repo={} commit={commit_hex} artifact={artifact}",
                claim.repo
            );
            Ok(())
        }
        Some(_) => {
            println!("MISMATCH program={program} reason=artifact hash differs");
            Err(VerifyStatus::Mismatch.with("artifact hash mismatch").into())
        }
        None => {
            println!(
                "UNVERIFIED_CLAIM program={program} repo={} commit={commit_hex} (set FORGE_VERIFY_SO to compare a rebuilt artifact)",
                claim.repo
            );
            Err(VerifyStatus::ClaimOnly
                .with("claim only; no artifact to compare")
                .into())
        }
    }
}

fn compute_artifact_hash() -> Option<[u8; 32]> {
    let so = std::env::var("FORGE_VERIFY_SO").ok()?;
    let output = std::process::Command::new("solana-verify")
        .args(["get-executable-hash", &so])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Oid::from_hex(HashAlgorithm::Sha256, &text)
        .ok()
        .map(|o| o.to_bytes32())
}

fn write_checkpoint(
    root: &Path,
    name: &str,
    repo_pda: &solana_message::Address,
    target: &Oid,
    message_hash: &Oid,
) -> Result<()> {
    let payload = serde_json::json!({
        "v": 1,
        "kind": "forge-release-checkpoint",
        "tag": name,
        "repo": repo_pda.to_string(),
        "commit": target.to_tagged_string(),
        "messageHash": message_hash.to_tagged_string(),
    });
    let bytes = serde_json::to_vec_pretty(&payload)?;
    let dir = root.join(".forge/checkpoints");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(format!("{name}.json")), &bytes)?;
    match std::env::var("FORGE_ARWEAVE_URL") {
        Ok(url) => {
            let bundler = forge_storage::ArweaveBundler::new(
                "arweave",
                url,
                forge_storage::http::ReqwestTransport::default(),
            );
            match bundler.upload(&bytes) {
                Ok(loc) => println!("checkpoint uploaded: {loc:?}"),
                Err(err) => println!("checkpoint upload failed: {err}"),
            }
        }
        Err(_) => println!("checkpoint written locally (set FORGE_ARWEAVE_URL to upload)"),
    }
    Ok(())
}

fn permissions(action: PermissionsAction) -> Result<()> {
    let repo = GitRepo::discover(&std::env::current_dir()?)?;
    let config = ForgeConfig::load(&repo.root)?;
    let wallet = wallet::load_repo_wallet(&repo.root, &config.wallet)?;
    let chain = sync::active_chain()?;
    let pid = chain.program_id();
    if config.repo_pda.is_empty() {
        bail!("no onchain repository configured; run `forge push` first");
    }
    let repo_pda = parse_address(&config.repo_pda)?;
    match action {
        PermissionsAction::Set {
            contributor,
            role,
            expires_slot,
        } => {
            let contributor = parse_address(&contributor)?;
            let role = chain::parse_role(&role)?;
            let ix = chain::update_permissions_instruction(
                &pid,
                &wallet.pubkey(),
                repo_pda,
                contributor,
                role,
                expires_slot,
            );
            let tx = chain.send(&wallet, &[ix])?;
            println!("permission set for {contributor} (role {role})  tx {tx}");
        }
        PermissionsAction::Get { contributor } => {
            let contributor = parse_address(&contributor)?;
            let addr = chain::permission_pda(&pid, &repo_pda, &contributor);
            match chain.fetch_permission(&addr)? {
                Some(p) => println!(
                    "{contributor}: role={} expires_slot={}",
                    p.role, p.expires_slot
                ),
                None => println!("{contributor}: no permission entry"),
            }
        }
    }
    Ok(())
}
