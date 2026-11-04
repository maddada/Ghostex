//! PKCE (RFC 7636, S256) for Instabox's loopback sign-in.

use sha2::{Digest, Sha256};

/// The code challenge of a verifier: base64url(sha256(verifier)) without padding.
pub fn s256_challenge(verifier: &str) -> String {
    crate::ticket::base64url_encode(&Sha256::digest(verifier.as_bytes()))
}

/// A verifier from caller-supplied randomness (at least 32 bytes give 43+ characters).
pub fn verifier_from_entropy(entropy: &[u8]) -> String {
    crate::ticket::base64url_encode(entropy)
}
