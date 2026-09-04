//! Digest normalization and verification helpers for downloaded artifacts.

use sha1::{Digest, Sha1};

/// Normalizes a hexadecimal digest only when it decodes to exactly `BYTES`.
pub(crate) fn normalized_hex<const BYTES: usize>(value: &str) -> Option<String> {
    let value = value.trim();
    let mut decoded = [0_u8; BYTES];
    hex::decode_to_slice(value, &mut decoded).ok()?;
    Some(value.to_ascii_lowercase())
}

pub(crate) fn sha1_hex(bytes: &[u8]) -> String {
    hex::encode(Sha1::digest(bytes))
}

pub(crate) fn hex_matches<const BYTES: usize>(expected: &str, actual: &str) -> bool {
    normalized_hex::<BYTES>(expected)
        .zip(normalized_hex::<BYTES>(actual))
        .is_some_and(|(expected, actual)| expected == actual)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_hex_accepts_exact_digest_and_lowercases_it() {
        assert_eq!(normalized_hex::<3>(" A1B2C3 "), Some("a1b2c3".to_owned()));
    }

    #[test]
    fn normalized_hex_rejects_wrong_length() {
        assert_eq!(normalized_hex::<3>("a1b2"), None);
    }

    #[test]
    fn normalized_hex_rejects_non_hexadecimal_input() {
        assert_eq!(normalized_hex::<3>("a1b2xz"), None);
    }

    #[test]
    fn sha1_hex_matches_known_vector() {
        assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
    }

    #[test]
    fn hex_comparison_normalizes_case_and_outer_whitespace() {
        assert!(hex_matches::<20>(
            "  A9993E364706816ABA3E25717850C26C9CD0D89D  ",
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        ));
    }
}
