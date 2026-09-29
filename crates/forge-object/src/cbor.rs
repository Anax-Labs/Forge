//! Minimal canonical CBOR (RFC 8949 §4.2.1 core deterministic encoding).
//!
//! Only the subset needed by the Forge attestation is implemented: unsigned and
//! negative integers, text strings, arrays, and maps. Encoding always uses the
//! shortest form, definite lengths, and bytewise-lexicographically sorted map
//! keys. Decoding rejects non-minimal lengths and indefinite-length items, so a
//! successfully decoded attestation is guaranteed canonical.

use crate::error::ObjectError;

fn invalid(message: impl Into<String>) -> ObjectError {
    ObjectError::Cbor(message.into())
}

/// Writes a CBOR head for `major` type with `value`.
pub(crate) fn encode_head(out: &mut Vec<u8>, major: u8, value: u64) {
    let tag = major << 5;
    if let Ok(small) = u8::try_from(value) {
        if small < 24 {
            out.push(tag | small);
        } else {
            out.push(tag | 24);
            out.push(small);
        }
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

/// Encodes a signed integer, using the shortest form.
pub(crate) fn encode_int(out: &mut Vec<u8>, value: i64) {
    if value >= 0 {
        encode_head(out, 0, value.unsigned_abs());
    } else {
        encode_head(out, 1, (-1 - value).unsigned_abs());
    }
}

/// Encodes `value` as an unsigned integer.
pub(crate) fn encode_uint(out: &mut Vec<u8>, value: u64) {
    encode_head(out, 0, value);
}

/// Encodes a definite-length text string.
pub(crate) fn encode_text(out: &mut Vec<u8>, value: &str) {
    encode_head(out, 3, value.len() as u64);
    out.extend_from_slice(value.as_bytes());
}

/// Encodes an array header.
pub(crate) fn encode_array_head(out: &mut Vec<u8>, len: usize) {
    encode_head(out, 4, len as u64);
}

/// Encodes a map header.
pub(crate) fn encode_map_head(out: &mut Vec<u8>, len: usize) {
    encode_head(out, 5, len as u64);
}

/// A cursor over CBOR bytes that enforces deterministic encoding.
pub(crate) struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub(crate) fn is_done(&self) -> bool {
        self.pos == self.data.len()
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], ObjectError> {
        if self.pos + n > self.data.len() {
            return Err(invalid("unexpected end of input"));
        }
        let slice = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn head(&mut self) -> Result<(u8, u64), ObjectError> {
        let first = self.take(1)?[0];
        let major = first >> 5;
        let info = first & 0x1f;
        let value = match info {
            0..=23 => u64::from(info),
            24 => {
                let v = u64::from(self.take(1)?[0]);
                if v < 24 {
                    return Err(invalid("non-minimal 1-byte length"));
                }
                v
            }
            25 => {
                let s = self.take(2)?;
                let v = u64::from(u16::from_be_bytes([s[0], s[1]]));
                if u8::try_from(v).is_ok() {
                    return Err(invalid("non-minimal 2-byte length"));
                }
                v
            }
            26 => {
                let s = self.take(4)?;
                let v = u64::from(u32::from_be_bytes([s[0], s[1], s[2], s[3]]));
                if u16::try_from(v).is_ok() {
                    return Err(invalid("non-minimal 4-byte length"));
                }
                v
            }
            27 => {
                let s = self.take(8)?;
                let v = u64::from_be_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]);
                if u32::try_from(v).is_ok() {
                    return Err(invalid("non-minimal 8-byte length"));
                }
                v
            }
            _ => return Err(invalid("indefinite or reserved additional information")),
        };
        Ok((major, value))
    }

    pub(crate) fn read_map_head(&mut self) -> Result<u64, ObjectError> {
        let (major, value) = self.head()?;
        if major != 5 {
            return Err(invalid("expected a map"));
        }
        Ok(value)
    }

    pub(crate) fn read_array_head(&mut self) -> Result<u64, ObjectError> {
        let (major, value) = self.head()?;
        if major != 4 {
            return Err(invalid("expected an array"));
        }
        Ok(value)
    }

    pub(crate) fn read_uint(&mut self) -> Result<u64, ObjectError> {
        let (major, value) = self.head()?;
        if major != 0 {
            return Err(invalid("expected an unsigned integer"));
        }
        Ok(value)
    }

    pub(crate) fn read_int(&mut self) -> Result<i64, ObjectError> {
        let (major, value) = self.head()?;
        match major {
            0 => i64::try_from(value).map_err(|_| invalid("integer out of range")),
            1 => {
                let n = i64::try_from(value).map_err(|_| invalid("integer out of range"))?;
                Ok(-1 - n)
            }
            _ => Err(invalid("expected an integer")),
        }
    }

    pub(crate) fn read_text(&mut self) -> Result<String, ObjectError> {
        let (major, len) = self.head()?;
        if major != 3 {
            return Err(invalid("expected a text string"));
        }
        let len = usize::try_from(len).map_err(|_| invalid("text too long"))?;
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| invalid("text is not valid UTF-8"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_uint_encodings() {
        let mut out = Vec::new();
        encode_uint(&mut out, 0);
        assert_eq!(out, [0x00]);
        out.clear();
        encode_uint(&mut out, 23);
        assert_eq!(out, [0x17]);
        out.clear();
        encode_uint(&mut out, 24);
        assert_eq!(out, [0x18, 0x18]);
        out.clear();
        encode_uint(&mut out, 1000);
        assert_eq!(out, [0x19, 0x03, 0xE8]);
    }

    #[test]
    fn negative_int_encoding() {
        let mut out = Vec::new();
        encode_int(&mut out, -1);
        assert_eq!(out, [0x20]);
        out.clear();
        encode_int(&mut out, -500);
        assert_eq!(out, [0x39, 0x01, 0xF3]);
    }

    #[test]
    fn reader_rejects_non_minimal() {
        // 0x18 0x00 encodes 0 non-minimally.
        let mut reader = Reader::new(&[0x18, 0x00]);
        assert!(reader.read_uint().is_err());
    }

    #[test]
    fn reader_rejects_indefinite() {
        // 0x5f = indefinite-length byte string.
        let mut reader = Reader::new(&[0x5f]);
        assert!(reader.read_text().is_err());
    }

    #[test]
    fn roundtrip_int_and_text() {
        let mut out = Vec::new();
        encode_int(&mut out, -12345);
        encode_text(&mut out, "hello");
        let mut reader = Reader::new(&out);
        assert_eq!(reader.read_int().unwrap(), -12345);
        assert_eq!(reader.read_text().unwrap(), "hello");
        assert!(reader.is_done());
    }
}
