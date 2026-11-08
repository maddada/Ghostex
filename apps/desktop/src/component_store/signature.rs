//! Ghostex's Ed25519 signature on a component archive (Collab is gxserver's; this checks the
//! desktop's own: the Code editor's code-server and CEF), checked after the archive's size and
//! SHA-256 and before anything else touches it: before its macOS quarantine flag is removed and
//! before it is unpacked. A release build refuses an unsigned archive; a development build lets
//! one through with a warning (local dev manifests carry none), never a wrong signature.
//!
//! Contract and public key: packages/gx-protocol/src/component_signing.rs.
//!
//! CDXC:Release 2026-11-08 DECISION:
//! User (#495, Q1731): one ed25519 Ghostex signature per component archive, public key built in, checked for every component; refuse unsigned or mismatched archives, and stop removing the macOS quarantine flag before the check passes.

use base64::Engine as _;
use ghostex_gx_protocol::component_signing::{
    COMPONENT_SIGNING_PUBLIC_KEY_HEX, component_signing_statement, decode_public_key,
};

use super::manifest::{ComponentDefinition, ComponentPlatformAsset};

fn public_key() -> Option<[u8; 32]> {
    if cfg!(debug_assertions)
        && let Ok(hex) = std::env::var("GHOSTEX_COMPONENT_SIGNING_PUBLIC_KEY")
    {
        return decode_public_key(&hex);
    }
    decode_public_key(COMPONENT_SIGNING_PUBLIC_KEY_HEX)
}

pub(crate) fn verify_component_signature(
    component: &ComponentDefinition,
    platform: &str,
    asset: &ComponentPlatformAsset,
) -> Result<(), String> {
    verify_with(component, platform, asset, cfg!(debug_assertions))
}

fn verify_with(
    component: &ComponentDefinition,
    platform: &str,
    asset: &ComponentPlatformAsset,
    allow_unsigned: bool,
) -> Result<(), String> {
    let Some(signature) = asset.signature.as_deref() else {
        if allow_unsigned {
            log::warn!(
                "component {} {} for {platform} is unsigned; a release build refuses it",
                component.name,
                component.component_version
            );
            return Ok(());
        }
        return Err(format!(
            "{} isn't signed by Ghostex, so it wasn't installed.",
            asset.asset_name
        ));
    };
    let key =
        public_key().ok_or_else(|| "Ghostex's component signing key is unreadable.".to_string())?;
    let signature = base64::engine::general_purpose::STANDARD
        .decode(signature.trim())
        .map_err(|_| {
            format!(
                "{}'s signature is unreadable, so it wasn't installed.",
                asset.asset_name
            )
        })?;
    let statement = component_signing_statement(
        &component.name,
        &component.component_version,
        platform,
        &asset.asset_name,
        &asset.sha256,
        asset.size_bytes,
    );
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, key)
        .verify(statement.as_bytes(), &signature)
        .map_err(|_| {
            format!(
                "{}'s signature doesn't match Ghostex's, so it wasn't installed.",
                asset.asset_name
            )
        })
}
