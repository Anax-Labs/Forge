//! IPLD CARv1 encoding of Git-framed objects (§8.3, [IPLD CAR](https://ipld.io/specs/transport/car/carv1/)).
//!
//! Each block is the Git framing (`<type> <len>\0<payload>`). The block CID is
//! CIDv1-raw-sha256 of those bytes — a **locator**, not the protocol address.
//! The protocol address remains the Git OID, which is recomputed on unpack.

use crate::cid::{Cid, CID_V1_RAW_SHA256_LEN};
use crate::error::StorageError;
use crate::object::GitObject;
use forge_object::hash::{HashAlgorithm, Oid};

/// A CARv1 archive of Git-framed objects.
#[derive(Debug, Clone)]
pub struct Car {
    /// Root block CIDs (typically the tip commit's framed-object CID).
    pub roots: Vec<Cid>,
    /// Blocks in archive order.
    pub blocks: Vec<CarBlock>,
}

/// One CAR block: CID locator plus Git-framed bytes.
#[derive(Debug, Clone)]
pub struct CarBlock {
    /// CIDv1 of [`Self::bytes`].
    pub cid: Cid,
    /// Git-framed object bytes.
    pub bytes: Vec<u8>,
}

impl Car {
    /// Builds a CAR from Git objects. `root` is placed first in `roots` using
    /// the CID of its framed bytes.
    ///
    /// # Errors
    /// Returns [`StorageError::NotFound`] when `root` is not in `objects`.
    pub fn from_objects(objects: &[GitObject], root: &Oid) -> Result<Self, StorageError> {
        let mut blocks = Vec::with_capacity(objects.len());
        let mut root_cid = None;
        for obj in objects {
            obj.verify()?;
            let bytes = obj.framed();
            let cid = Cid::of_raw_sha256(&bytes);
            if obj.oid == *root {
                root_cid = Some(cid.clone());
            }
            blocks.push(CarBlock { cid, bytes });
        }
        let root_cid = root_cid.ok_or_else(|| StorageError::NotFound(root.to_tagged_string()))?;
        Ok(Self {
            roots: vec![root_cid],
            blocks,
        })
    }

    /// Encodes CARv1 (unsigned-varint length prefixes, DAG-CBOR header).
    pub fn encode(&self) -> Vec<u8> {
        let header = encode_header(&self.roots);
        let mut out = Vec::new();
        put_uvarint(&mut out, header.len() as u64);
        out.extend_from_slice(&header);
        for block in &self.blocks {
            let block_len = block.cid.as_bytes().len() + block.bytes.len();
            put_uvarint(&mut out, block_len as u64);
            out.extend_from_slice(block.cid.as_bytes());
            out.extend_from_slice(&block.bytes);
        }
        out
    }

    /// Decodes a CARv1 and **re-hashes every block CID** before returning.
    ///
    /// # Errors
    /// Returns [`StorageError::InvalidCar`] or [`StorageError::CidMismatch`].
    pub fn decode(bytes: &[u8]) -> Result<Self, StorageError> {
        let mut pos = 0usize;
        let header_len = usize::try_from(take_uvarint(bytes, &mut pos)?)
            .map_err(|_| StorageError::InvalidCar("header length overflow".into()))?;
        if pos + header_len > bytes.len() {
            return Err(StorageError::InvalidCar("truncated header".into()));
        }
        let header = &bytes[pos..pos + header_len];
        pos += header_len;
        let roots = decode_header(header)?;
        let mut blocks = Vec::new();
        while pos < bytes.len() {
            let block_len = usize::try_from(take_uvarint(bytes, &mut pos)?)
                .map_err(|_| StorageError::InvalidCar("block length overflow".into()))?;
            if pos + block_len > bytes.len() {
                return Err(StorageError::InvalidCar("truncated block".into()));
            }
            let block = &bytes[pos..pos + block_len];
            pos += block_len;
            if block.len() < CID_V1_RAW_SHA256_LEN {
                return Err(StorageError::InvalidCar("block shorter than CID".into()));
            }
            let cid = Cid::from_bytes(&block[..CID_V1_RAW_SHA256_LEN])?;
            let data = block[CID_V1_RAW_SHA256_LEN..].to_vec();
            let actual = Cid::of_raw_sha256(&data);
            if actual != cid {
                return Err(StorageError::CidMismatch {
                    expected: cid.to_multibase(),
                    actual: actual.to_multibase(),
                });
            }
            blocks.push(CarBlock { cid, bytes: data });
        }
        Ok(Self { roots, blocks })
    }

    /// Finds a Git object by **recomputing** its oid from each block.
    ///
    /// # Errors
    /// Returns framing errors, or [`StorageError::NotFound`].
    pub fn get(&self, expected: &Oid) -> Result<GitObject, StorageError> {
        for block in &self.blocks {
            if let Ok(obj) = GitObject::verify_framed(&block.bytes, expected) {
                return Ok(obj);
            }
        }
        Err(StorageError::NotFound(expected.to_tagged_string()))
    }

    /// Unpacks every block as a Git object for `algorithm`.
    ///
    /// # Errors
    /// Returns framing errors from `forge-object`.
    pub fn unpack(&self, algorithm: HashAlgorithm) -> Result<Vec<GitObject>, StorageError> {
        self.blocks
            .iter()
            .map(|block| GitObject::from_framed(&block.bytes, algorithm))
            .collect()
    }
}

fn put_uvarint(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn take_uvarint(bytes: &[u8], pos: &mut usize) -> Result<u64, StorageError> {
    let mut result = 0u64;
    let mut shift = 0u32;
    loop {
        if *pos >= bytes.len() {
            return Err(StorageError::InvalidCar("truncated varint".into()));
        }
        let byte = bytes[*pos];
        *pos += 1;
        if shift >= 64 {
            return Err(StorageError::InvalidCar("varint overflow".into()));
        }
        result |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(result);
        }
        shift += 7;
        if shift > 63 {
            return Err(StorageError::InvalidCar("varint overflow".into()));
        }
    }
}

fn encode_head(out: &mut Vec<u8>, major: u8, value: u64) {
    let tag = major << 5;
    if let Ok(small) = u8::try_from(value) {
        if small < 24 {
            out.push(tag | small);
            return;
        }
        out.push(tag | 24);
        out.push(small);
        return;
    }
    if let Ok(medium) = u16::try_from(value) {
        out.push(tag | 25);
        out.extend_from_slice(&medium.to_be_bytes());
        return;
    }
    if let Ok(large) = u32::try_from(value) {
        out.push(tag | 26);
        out.extend_from_slice(&large.to_be_bytes());
        return;
    }
    out.push(tag | 27);
    out.extend_from_slice(&value.to_be_bytes());
}

fn encode_text(out: &mut Vec<u8>, value: &str) {
    encode_head(out, 3, value.len() as u64);
    out.extend_from_slice(value.as_bytes());
}

fn encode_bytes(out: &mut Vec<u8>, value: &[u8]) {
    encode_head(out, 2, value.len() as u64);
    out.extend_from_slice(value);
}

fn encode_header(roots: &[Cid]) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(0xA2);
    encode_text(&mut out, "roots");
    encode_head(&mut out, 4, roots.len() as u64);
    for root in roots {
        let mut tagged = Vec::with_capacity(1 + CID_V1_RAW_SHA256_LEN);
        tagged.push(0x00);
        tagged.extend_from_slice(root.as_bytes());
        out.push(0xD8);
        out.push(0x2A);
        encode_bytes(&mut out, &tagged);
    }
    encode_text(&mut out, "version");
    encode_head(&mut out, 0, 1);
    out
}

fn decode_header(bytes: &[u8]) -> Result<Vec<Cid>, StorageError> {
    let mut pos = 0usize;
    let first = take(bytes, &mut pos, 1)?[0];
    if first >> 5 != 5 {
        return Err(StorageError::InvalidCar("header is not a map".into()));
    }
    let map_len = additional(bytes, &mut pos, first)?;
    if map_len != 2 {
        return Err(StorageError::InvalidCar(
            "header map must have 2 keys".into(),
        ));
    }
    let k1 = read_text(bytes, &mut pos)?;
    if k1 != "roots" {
        return Err(StorageError::InvalidCar("first key must be roots".into()));
    }
    let arr_head = take(bytes, &mut pos, 1)?[0];
    if arr_head >> 5 != 4 {
        return Err(StorageError::InvalidCar("roots is not an array".into()));
    }
    let n = additional(bytes, &mut pos, arr_head)?;
    let mut roots = Vec::new();
    for _ in 0..n {
        roots.push(read_tagged_cid(bytes, &mut pos)?);
    }
    let k2 = read_text(bytes, &mut pos)?;
    if k2 != "version" {
        return Err(StorageError::InvalidCar(
            "second key must be version".into(),
        ));
    }
    let vhead = take(bytes, &mut pos, 1)?[0];
    if vhead >> 5 != 0 {
        return Err(StorageError::InvalidCar(
            "version is not an unsigned int".into(),
        ));
    }
    let version = additional(bytes, &mut pos, vhead)?;
    if version != 1 {
        return Err(StorageError::InvalidCar(format!(
            "unsupported CAR version {version}"
        )));
    }
    if pos != bytes.len() {
        return Err(StorageError::InvalidCar("trailing header bytes".into()));
    }
    Ok(roots)
}

fn take<'a>(bytes: &'a [u8], pos: &mut usize, n: usize) -> Result<&'a [u8], StorageError> {
    if *pos + n > bytes.len() {
        return Err(StorageError::InvalidCar("truncated DAG-CBOR".into()));
    }
    let slice = &bytes[*pos..*pos + n];
    *pos += n;
    Ok(slice)
}

fn additional(bytes: &[u8], pos: &mut usize, first: u8) -> Result<u64, StorageError> {
    match first & 0x1f {
        n @ 0..=23 => Ok(u64::from(n)),
        24 => Ok(u64::from(take(bytes, pos, 1)?[0])),
        25 => {
            let s = take(bytes, pos, 2)?;
            Ok(u64::from(u16::from_be_bytes([s[0], s[1]])))
        }
        26 => {
            let s = take(bytes, pos, 4)?;
            Ok(u64::from(u32::from_be_bytes([s[0], s[1], s[2], s[3]])))
        }
        27 => {
            let s = take(bytes, pos, 8)?;
            Ok(u64::from_be_bytes([
                s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7],
            ]))
        }
        _ => Err(StorageError::InvalidCar("indefinite DAG-CBOR".into())),
    }
}

fn read_text(bytes: &[u8], pos: &mut usize) -> Result<String, StorageError> {
    let first = take(bytes, pos, 1)?[0];
    if first >> 5 != 3 {
        return Err(StorageError::InvalidCar("expected text".into()));
    }
    let len = usize::try_from(additional(bytes, pos, first)?)
        .map_err(|_| StorageError::InvalidCar("text too long".into()))?;
    let s = take(bytes, pos, len)?;
    String::from_utf8(s.to_vec()).map_err(|_| StorageError::InvalidCar("text not UTF-8".into()))
}

fn read_tagged_cid(bytes: &[u8], pos: &mut usize) -> Result<Cid, StorageError> {
    let b0 = take(bytes, pos, 1)?[0];
    if b0 != 0xD8 || take(bytes, pos, 1)?[0] != 0x2A {
        return Err(StorageError::InvalidCar("expected CID tag 42".into()));
    }
    let first = take(bytes, pos, 1)?[0];
    if first >> 5 != 2 {
        return Err(StorageError::InvalidCar("CID is not a byte string".into()));
    }
    let len = usize::try_from(additional(bytes, pos, first)?)
        .map_err(|_| StorageError::InvalidCar("CID too long".into()))?;
    let raw = take(bytes, pos, len)?;
    if raw.first() != Some(&0x00) {
        return Err(StorageError::InvalidCar(
            "CID missing identity multibase".into(),
        ));
    }
    Cid::from_bytes(&raw[1..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_object::blob::blob_oid;
    use forge_object::object::ObjectType;

    #[test]
    fn car_roundtrip_empty_blob() {
        let obj = GitObject::from_payload(ObjectType::Blob, Vec::new(), HashAlgorithm::Sha256);
        let car = Car::from_objects(std::slice::from_ref(&obj), &obj.oid).unwrap();
        let encoded = car.encode();
        let decoded = Car::decode(&encoded).unwrap();
        let got = decoded.get(&obj.oid).unwrap();
        assert_eq!(got, obj);
    }

    #[test]
    fn car_rejects_corrupt_block_cid() {
        let obj = GitObject::from_payload(ObjectType::Blob, b"x".to_vec(), HashAlgorithm::Sha256);
        let mut encoded = Car::from_objects(&[obj], &blob_oid(b"x", HashAlgorithm::Sha256))
            .unwrap()
            .encode();
        let last = encoded.len() - 1;
        encoded[last] ^= 0xff;
        assert!(matches!(
            Car::decode(&encoded),
            Err(StorageError::CidMismatch { .. })
        ));
    }
}
