//! CLI command dispatch (Phases 6–7).

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use forge_object::commit::Commit;
use forge_object::hash::HashAlgorithm;
use solana_keypair::Keypair;
use solana_signer::Signer;

use crate::attest;
use crate::config::{self, ForgeConfig};
use crate::git::{self, GitRepo};
use crate::sync;
use crate::wallet;
use crate::{Command, RemoteAction};

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
        Command::Gc {
            verify_availability,
        } => gc(verify_availability),
        other => bail!("`{other:?}` is not implemented yet (see phase_implementation.md)"),
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
