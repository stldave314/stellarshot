// SPDX-License-Identifier: GPL-3.0-only

//! An API token for the web interface: high-entropy random bytes, not a
//! UUID (fewer bits, and not meant as a secret in the first place). Shown to
//! whoever generated it exactly once; only its SHA-256 hash is ever stored,
//! so a leaked settings file never leaks a usable token. A token is already
//! high-entropy, so a slow, memory-hard hash (as a low-entropy password
//! would need) buys nothing here.

use rand::RngExt;
use sha2::{Digest, Sha256};

/// Bytes of entropy in a generated token, before hex encoding.
const TOKEN_BYTES: usize = 32;

/// A freshly generated token: `raw` is shown once, `hash` is what to keep.
pub struct Token {
    pub raw: String,
    pub hash: String,
}

pub fn generate() -> Token {
    let mut bytes = [0u8; TOKEN_BYTES];
    rand::rng().fill(&mut bytes);
    let raw = hex_encode(&bytes);
    let hash = hash(&raw);
    Token { raw, hash }
}

/// Whether `candidate` is the token `hash` was generated from.
pub fn verify(candidate: &str, hash: &str) -> bool {
    self::hash(candidate) == hash
}

fn hash(token: &str) -> String {
    hex_encode(&Sha256::digest(token.as_bytes()))
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_generated_tokens_hash_verifies_it() {
        let token = generate();
        assert!(verify(&token.raw, &token.hash));
    }

    #[test]
    fn a_wrong_token_does_not_verify() {
        let token = generate();
        assert!(!verify("not-the-token", &token.hash));
    }

    #[test]
    fn two_generated_tokens_are_never_the_same() {
        let a = generate();
        let b = generate();
        assert_ne!(a.raw, b.raw);
    }

    #[test]
    fn the_hash_never_equals_the_raw_token() {
        let token = generate();
        assert_ne!(token.raw, token.hash);
    }
}
