//! Base64 decoding for the server's thumbnails: twenty lines of our own
//! rather than a crate (AGENTS.md).

/// The bytes of standard base64 (padding optional); None on any other character.
pub fn decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes().filter(|&c| c != b'=') {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        acc = (acc << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn decodes_with_and_without_padding() {
        assert_eq!(super::decode("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(super::decode("aGVsbG8").unwrap(), b"hello");
        assert_eq!(super::decode("/+8=").unwrap(), [0xff, 0xef]);
        assert!(super::decode("a b").is_none());
    }
}
