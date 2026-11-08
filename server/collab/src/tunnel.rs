//! The end-to-end lock between a guest's page and this gxserver. The relay carries only these
//! frames and the route id, and never the key they come from.
//!
//! - The link's key `K` is 32 random bytes, written as 43 URL-safe base64 characters after `#`.
//! - Route id: `hex(HKDF-SHA256(K, salt = "", info = "ghostex-collab/route/v1"))`.
//! - A guest connection starts with one clear frame, `0x01 || salt` (32 random bytes). Both ends
//!   then derive one AES-256-GCM key per direction: `HKDF(K, salt, "ghostex-collab/g2h/v1")` for
//!   guest to host and `"ghostex-collab/h2g/v1"` for host to guest.
//! - Every later frame is `AES-256-GCM(plaintext)` with the nonce `0u32 || counter_be_u64`, the
//!   counter starting at 0 for each direction. Frames arrive in order over one WebSocket, so the
//!   counter is never sent; a replayed, dropped or reordered frame fails to open and ends the
//!   connection.
//!
//! SEE-ALSO: apps/gpui-web/www/src/collab/tunnel.js (the page's half, on WebCrypto), server/share-relay (the relay).

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::Aes256Gcm;
use sha2::{Digest, Sha256};

pub const HELLO_TAG: u8 = 0x01;
const ROUTE_INFO: &[u8] = b"ghostex-collab/route/v1";
const GUEST_TO_HOST_INFO: &[u8] = b"ghostex-collab/g2h/v1";
const HOST_TO_GUEST_INFO: &[u8] = b"ghostex-collab/h2g/v1";

fn hmac_sha256(key: &[u8], data: &[&[u8]]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut block_key = [0u8; BLOCK];
    if key.len() > BLOCK {
        block_key[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block_key[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(block_key.map(|b| b ^ 0x36));
    for part in data {
        inner.update(part);
    }
    let inner = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(block_key.map(|b| b ^ 0x5c));
    outer.update(inner);
    outer.finalize().into()
}

/// HKDF-SHA256 (RFC 5869) with one 32-byte output block, the same bits WebCrypto's
/// `deriveBits({name: "HKDF", hash: "SHA-256", salt, info}, key, 256)` returns.
pub fn hkdf32(ikm: &[u8], salt: &[u8], info: &[u8]) -> [u8; 32] {
    let prk = hmac_sha256(salt, &[ikm]);
    hmac_sha256(&prk, &[info, &[1u8]])
}

pub fn route_id(key: &[u8; 32]) -> String {
    hex::encode(hkdf32(key, b"", ROUTE_INFO))
}

fn nonce(counter: u64) -> aes_gcm::aead::Nonce<Aes256Gcm> {
    let mut bytes = [0u8; 12];
    bytes[4..].copy_from_slice(&counter.to_be_bytes());
    aes_gcm::aead::Nonce::<Aes256Gcm>::try_from(&bytes[..]).expect("12-byte nonce")
}

/// One guest connection's two directions, on the host's side.
pub struct HostCipher {
    seal_key: Aes256Gcm,
    open_key: Aes256Gcm,
    sealed: u64,
    opened: u64,
}

impl HostCipher {
    /// From the guest's clear hello frame (`0x01 || salt`); `None` when it is not one.
    pub fn from_hello(key: &[u8; 32], hello: &[u8]) -> Option<Self> {
        if hello.len() != 33 || hello[0] != HELLO_TAG {
            return None;
        }
        let salt = &hello[1..];
        let seal = hkdf32(key, salt, HOST_TO_GUEST_INFO);
        let open = hkdf32(key, salt, GUEST_TO_HOST_INFO);
        Some(Self {
            seal_key: Aes256Gcm::new_from_slice(&seal).ok()?,
            open_key: Aes256Gcm::new_from_slice(&open).ok()?,
            sealed: 0,
            opened: 0,
        })
    }

    pub fn seal(&mut self, plain: &[u8]) -> Vec<u8> {
        let frame = self
            .seal_key
            .encrypt(&nonce(self.sealed), plain)
            .expect("AES-GCM seals any length the relay accepts");
        self.sealed += 1;
        frame
    }

    pub fn open(&mut self, frame: &[u8]) -> Option<Vec<u8>> {
        let plain = self.open_key.decrypt(&nonce(self.opened), frame).ok()?;
        self.opened += 1;
        Some(plain)
    }
}
