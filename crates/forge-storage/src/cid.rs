//! CIDv1 (raw codec, SHA-256) locators for IPFS blocks.
//!
//! Content is **addressed by Git OID**, not CID (§8.3). A CID is only a
//! storage locator / hint and is re-verified on every read by hashing the
//! block bytes.

use sha2::{Digest, Sha256};

use crate::error::StorageError;

/// Multicodec `raw` (0x55).
pub const CODEC_RAW: u8 = 0x55;
/// Multihash `sha2-256` (0x12).
pub const MULTIHASH_SHA2_256: u8 = 0x12;
/// CIDv1 version byte.
pub const CID_VERSION: u8 = 0x01;
/// Binary length of a CIDv1-raw-sha256.
pub const CID_V1_RAW_SHA256_LEN: usize = 36;

const BASE32: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

/// A CIDv1 raw SHA-256 identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Cid {
    bytes: [u8; CID_V1_RAW_SHA256_LEN],
}

impl Cid {
    /// Computes the CID of `block` as CIDv1 / raw / sha2-256.
    pub fn of_raw_sha256(block: &[u8]) -> Self {
        let digest = Sha256::digest(block);
        let mut bytes = [0u8; CID_V1_RAW_SHA256_LEN];
        bytes[0] = CID_VERSION;
        bytes[1] = CODEC_RAW;
        bytes[2] = MULTIHASH_SHA2_256;
        bytes[3] = 32;
        bytes[4..].copy_from_slice(&digest);
        Self { bytes }
    }

    /// Parses a binary CIDv1-raw-sha256.
    ///
    /// # Errors
    /// Returns [`StorageError::InvalidCid`] when the codec/multihash is not
    /// the Forge-supported profile.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, StorageError> {
        if bytes.len() != CID_V1_RAW_SHA256_LEN {
            return Err(StorageError::InvalidCid(format!(
                "unsupported CID length {}",
                bytes.len()
            )));
        }
        if bytes[0] != CID_VERSION
            || bytes[1] != CODEC_RAW
            || bytes[2] != MULTIHASH_SHA2_256
            || bytes[3] != 32
        {
            return Err(StorageError::InvalidCid(
                "only CIDv1 raw sha2-256 is supported".into(),
            ));
        }
        let mut out = [0u8; CID_V1_RAW_SHA256_LEN];
        out.copy_from_slice(bytes);
        Ok(Self { bytes: out })
    }

    /// Parses a multibase base32 string (`bafkrei…`).
    ///
    /// # Errors
    /// Returns [`StorageError::InvalidCid`] for an unknown multibase or
    /// malformed payload.
    pub fn from_multibase(s: &str) -> Result<Self, StorageError> {
        let rest = s.strip_prefix('b').ok_or_else(|| {
            StorageError::InvalidCid("CID must be multibase base32 (b-prefix)".into())
        })?;
        let decoded = decode_base32_lower(rest)?;
        Self::from_bytes(&decoded)
    }

    /// Binary CID bytes (no multibase prefix).
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// SHA-256 digest of the block (the multihash payload).
    pub fn digest(&self) -> &[u8] {
        &self.bytes[4..]
    }

    /// Multibase base32 string (`bafkrei…`).
    pub fn to_multibase(&self) -> String {
        let mut out = String::from("b");
        out.push_str(&encode_base32_lower(&self.bytes));
        out
    }
}

impl std::fmt::Display for Cid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_multibase())
    }
}

fn encode_base32_lower(data: &[u8]) -> String {
    let mut bits = 0u32;
    let mut nbits = 0u32;
    let mut out = String::new();
    for &b in data {
        bits = (bits << 8) | u32::from(b);
        nbits += 8;
        while nbits >= 5 {
            nbits -= 5;
            let idx = ((bits >> nbits) & 0x1f) as usize;
            out.push(BASE32[idx] as char);
        }
    }
    if nbits > 0 {
        let idx = ((bits << (5 - nbits)) & 0x1f) as usize;
        out.push(BASE32[idx] as char);
    }
    out
}

fn decode_base32_lower(s: &str) -> Result<Vec<u8>, StorageError> {
    let mut bits = 0u32;
    let mut nbits = 0u32;
    let mut out = Vec::new();
    for ch in s.chars() {
        let byte = u8::try_from(ch)
            .map_err(|_| StorageError::InvalidCid(format!("invalid base32 char {ch}")))?;
        let idx = BASE32
            .iter()
            .position(|b| *b == byte)
            .ok_or_else(|| StorageError::InvalidCid(format!("invalid base32 char {ch}")))?;
        bits = (bits << 5) | u32::try_from(idx).expect("base32 index fits u32");
        nbits += 5;
        if nbits >= 8 {
            nbits -= 8;
            out.push(u8::try_from((bits >> nbits) & 0xff).expect("byte"));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_raw_block_has_well_known_cid() {
        let cid = Cid::of_raw_sha256(b"");
        assert_eq!(
            cid.to_multibase(),
            "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku"
        );
        assert_eq!(Cid::from_multibase(&cid.to_multibase()).unwrap(), cid);
    }

    #[test]
    fn roundtrip_non_empty() {
        let cid = Cid::of_raw_sha256(b"forge");
        let parsed = Cid::from_multibase(&cid.to_multibase()).unwrap();
        assert_eq!(parsed, cid);
        assert_eq!(parsed.digest(), Sha256::digest(b"forge").as_slice());
    }
}
