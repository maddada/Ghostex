//! Ghostex's signature on every on-demand component archive (Collab, the Code editor's
//! code-server, CEF), GitHub #495.
//!
//! The release signs one statement per archive with Ghostex's component signing key (an Ed25519
//! key held only as the `GHOSTEX_COMPONENT_SIGNING_KEY` Actions secret,
//! tooling/release-gpui/component-signing.mjs), and seals the signature into the app's
//! `on-demand-resources.json` beside the archive's SHA-256 and size (`platforms.<p>.signature`).
//! gxserver and the desktop check the archive's size and SHA-256 while it downloads, then this
//! signature against [`COMPONENT_SIGNING_PUBLIC_KEY_HEX`], before anything is unpacked (and, on
//! macOS, before the download's quarantine flag is removed). A release build refuses an archive
//! that is unsigned or whose signature doesn't match.
//!
//! The statement binds the component, its version, the platform, the file name, its SHA-256 and
//! its size, so a genuine archive can't be passed off as another component, version or platform.
//!
//! CDXC:Release 2026-11-08 DECISION:
//! User (#495, Q1731): "one ed25519 Ghostex signature per component archive (CI secret, public key built into gxserver), checked for every component (Collab, VS Code / code-server, GBots, CEF), not just Collab".

/// First line of every signed statement; bump the version to change the format.
pub const COMPONENT_SIGNING_DOMAIN: &str = "ghostex-component-v1";

/// Ghostex's component signing public key (raw Ed25519, hex).
pub const COMPONENT_SIGNING_PUBLIC_KEY_HEX: &str =
    "0a0c966a4663b7787a9b60248a0a2527a14ab9b904a2fcd3f5dd4fa958f665df";

/// The bytes the release signs for one archive: UTF-8, one field per line, `\n` line ends, no
/// trailing newline. `sha256` is lowercase hex.
pub fn component_signing_statement(
    name: &str,
    component_version: &str,
    platform: &str,
    asset_name: &str,
    sha256: &str,
    size_bytes: u64,
) -> String {
    format!(
        "{COMPONENT_SIGNING_DOMAIN}\n{name}\n{component_version}\n{platform}\n{asset_name}\n{}\n{size_bytes}",
        sha256.to_ascii_lowercase()
    )
}

/// A 32-byte key from hex, or `None`.
pub fn decode_public_key(hex: &str) -> Option<[u8; 32]> {
    let hex = hex.trim();
    if hex.len() != 64 {
        return None;
    }
    let mut key = [0_u8; 32];
    for (index, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(hex.get(index * 2..index * 2 + 2)?, 16).ok()?;
    }
    Some(key)
}
