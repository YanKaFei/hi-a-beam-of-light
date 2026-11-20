//! Minimal hex and base32 codecs.
//!
//! Deliberately implemented in-crate: F1 must build and test with the smallest
//! possible dependency surface, and the encoding is part of the stable wire
//! contract so it should not drift with a third-party crate.

const B32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

pub fn hex_encode(bytes: &[u8]) -> String {
    const H: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(H[(b >> 4) as usize] as char);
        out.push(H[(b & 0x0f) as usize] as char);
    }
    out
}

/// RFC 4648 base32, uppercase, no padding.
pub fn base32_encode(data: &[u8]) -> String {
    let mut out = String::new();
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;
    for &byte in data {
        buffer = (buffer << 8) | byte as u32;
        bits += 8;
        while bits >= 5 {
            let idx = ((buffer >> (bits - 5)) & 0x1f) as usize;
            out.push(B32[idx] as char);
            bits -= 5;
        }
    }
    if bits > 0 {
        let idx = ((buffer << (5 - bits)) & 0x1f) as usize;
        out.push(B32[idx] as char);
    }
    out
}

/// Decode RFC 4648 base32 (case-insensitive, padding and separators tolerated).
pub fn base32_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;
    for ch in s.chars() {
        if ch == '=' || ch == '-' || ch == ' ' || ch == '\n' || ch == '\r' {
            continue;
        }
        let upper = ch.to_ascii_uppercase();
        let val = B32.iter().position(|&c| c as char == upper)? as u32;
        buffer = (buffer << 5) | val;
        bits += 5;
        if bits >= 8 {
            out.push(((buffer >> (bits - 8)) & 0xff) as u8);
            bits -= 8;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip_shape() {
        assert_eq!(hex_encode(&[0x00, 0x0f, 0xff]), "000fff");
    }

    #[test]
    fn base32_roundtrip() {
        for len in 0..40usize {
            let data: Vec<u8> = (0..len).map(|i| (i * 7 + 3) as u8).collect();
            let enc = base32_encode(&data);
            let dec = base32_decode(&enc).expect("decode");
            assert_eq!(dec, data, "len {len}");
        }
    }

    #[test]
    fn base32_known_vectors() {
        assert_eq!(base32_encode(b"foobar"), "MZXW6YTBOI");
        assert_eq!(base32_decode("MZXW6YTBOI").unwrap(), b"foobar");
        // separator / case tolerance
        assert_eq!(base32_decode("mzxw-6ytb oi").unwrap(), b"foobar");
    }

    #[test]
    fn base32_rejects_non_alphabet() {
        assert!(base32_decode("MZXW!").is_none());
    }
}
