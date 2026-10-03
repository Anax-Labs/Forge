//! Optional onchain storage hints. Hints are **never trusted** (§8.3, §25 #11).

use sha2::{Digest, Sha256};

use crate::backend::Locator;

/// Domain-separated hash of a locator, suitable for an optional onchain field.
/// Clients must still fetch by Git OID and re-hash; this value is only a
/// lookup hint.
pub fn hint_hash(locator: &Locator) -> [u8; 32] {
    let mut data = Vec::from(b"forge-storage-hint\0");
    data.extend_from_slice(locator.backend_label().as_bytes());
    data.push(0);
    data.extend_from_slice(locator.provider.as_bytes());
    data.push(0);
    data.extend_from_slice(locator.id.as_bytes());
    Sha256::digest(&data).into()
}

impl Locator {
    fn backend_label(&self) -> &'static str {
        match self.backend {
            crate::backend::LocatorBackend::Ipfs => "ipfs",
            crate::backend::LocatorBackend::Arweave => "arweave",
            crate::backend::LocatorBackend::Fs => "fs",
            crate::backend::LocatorBackend::Memory => "memory",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{Locator, LocatorBackend};

    #[test]
    fn hint_hash_is_deterministic_and_domain_separated() {
        let loc = Locator {
            backend: LocatorBackend::Ipfs,
            provider: "ipfs-0".into(),
            id: "bafkreiabc".into(),
        };
        let a = hint_hash(&loc);
        let b = hint_hash(&loc);
        assert_eq!(a, b);
        let mut other = loc.clone();
        other.id.push('x');
        assert_ne!(hint_hash(&other), a);
    }
}
