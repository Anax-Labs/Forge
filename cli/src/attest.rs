//! Sidecar attestations: canonical CBOR + detached Ed25519 signature (ADR 0007).
//!
//! `.forge/attestations/<oid>.cbor` is exactly `canonical_cbor(attestation)`
//! (§5.5 / `docs/protocol.md`). The 64-byte signature is stored next to it as
//! `<oid>.sig` so the CBOR file stays a pure attestation.

use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use forge_object::attestation::{Attestation, ATTESTATION_VERSION};
use forge_object::commit::Commit;
use forge_object::hash::Oid;
use solana_keypair::Keypair;
use solana_pubkey::Pubkey;
use solana_signature::Signature;
use solana_signer::Signer;

use crate::config::attestations_dir;

/// Writes CBOR + signature for `commit` signed by `wallet`.
///
/// # Errors
/// Attestation construction or I/O errors.
pub fn write_sidecar(
    root: &Path,
    repo: &str,
    commit: &Commit,
    wallet: &Keypair,
    nonce: &str,
) -> Result<(Attestation, Signature)> {
    let algorithm = commit.algorithm();
    let message_hash = Oid::new(algorithm, algorithm.digest(&commit.message))?;
    let attestation = Attestation::new(
        ATTESTATION_VERSION,
        repo,
        commit.oid(),
        commit.parents.clone(),
        commit.tree.clone(),
        wallet.pubkey().to_string(),
        commit.author.timestamp,
        message_hash,
        nonce,
    )?;
    let cbor = attestation.to_canonical_cbor();
    let hash = attestation.attestation_hash();
    let signature = wallet.sign_message(hash.as_bytes());
    let dir = attestations_dir(root);
    fs::create_dir_all(&dir)?;
    let stem = commit.oid().to_hex();
    fs::write(dir.join(format!("{stem}.cbor")), cbor)?;
    fs::write(dir.join(format!("{stem}.sig")), signature.as_ref())?;
    Ok((attestation, signature))
}

/// Loads the sidecar for `oid`.
///
/// # Errors
/// Missing files, non-canonical CBOR, or a truncated signature.
pub fn load_sidecar(root: &Path, oid: &Oid) -> Result<(Attestation, Signature)> {
    let dir = attestations_dir(root);
    let stem = oid.to_hex();
    let cbor_path = dir.join(format!("{stem}.cbor"));
    let sig_path = dir.join(format!("{stem}.sig"));
    let cbor = fs::read(&cbor_path)
        .with_context(|| format!("missing attestation sidecar {}", cbor_path.display()))?;
    let sig_bytes = fs::read(&sig_path)
        .with_context(|| format!("missing attestation signature {}", sig_path.display()))?;
    let attestation = Attestation::from_canonical_cbor(&cbor)?;
    let signature = Signature::try_from(sig_bytes.as_slice())
        .map_err(|err| anyhow::anyhow!("invalid signature bytes: {err}"))?;
    Ok((attestation, signature))
}

/// Local verification of attestation binding + Ed25519 (no history inclusion).
///
/// # Errors
/// Mismatch between attestation and commit, or a bad signature.
pub fn verify_local(
    commit: &Commit,
    attestation: &Attestation,
    signature: &Signature,
) -> Result<()> {
    if attestation.commit != commit.oid() {
        bail!(
            "attestation commit {} != {}",
            attestation.commit.to_tagged_string(),
            commit.oid().to_tagged_string()
        );
    }
    if attestation.tree != commit.tree {
        bail!("attestation tree does not match commit");
    }
    if attestation.parents != commit.parents {
        bail!("attestation parents do not match commit");
    }
    let expected_msg = Oid::new(
        commit.algorithm(),
        commit.algorithm().digest(&commit.message),
    )?;
    if attestation.message_hash != expected_msg {
        bail!("attestation messageHash does not match commit message");
    }
    if attestation.authored_at != commit.author.timestamp {
        bail!("attestation authoredAt does not match commit author timestamp");
    }
    let recomputed = attestation.attestation_hash();
    let file_hash = {
        let mut preimage = Vec::from(forge_object::attestation::ATTESTATION_DOMAIN);
        preimage.extend_from_slice(&attestation.to_canonical_cbor());
        Oid::new(
            attestation.algorithm(),
            attestation.algorithm().digest(&preimage),
        )?
    };
    if file_hash != recomputed {
        bail!("attestation_hash is not H(domain || canonical_cbor)");
    }
    let author: Pubkey = attestation
        .author
        .parse()
        .map_err(|err| anyhow::anyhow!("attestation author is not a pubkey: {err}"))?;
    if !signature.verify(author.as_ref(), recomputed.as_bytes()) {
        bail!("Ed25519 signature does not verify over attestation_hash");
    }
    Ok(())
}

/// 16 random bytes as base58 (attestation nonce).
pub fn random_nonce(wallet: &Keypair) -> String {
    let extra = Keypair::new();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&extra.to_bytes()[..16]);
    bytes[0] ^= wallet.to_bytes()[0];
    bs58::encode(bytes).into_string()
}
