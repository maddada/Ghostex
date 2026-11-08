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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A throwaway key's signature over this archive's statement, made with Node's `crypto.sign`
    /// as tooling/release-gpui/component-signing.mjs does (the same vector gxserver's tests use).
    const TEST_PUBLIC_KEY: &str =
        "b388c522aec5df9f41e22e510e5c4ebfb3851662d477fee2ae872336ba128e58";
    const TEST_SIGNATURE: &str =
        "A/MJ7qChyfPi6XjpHxCCbF2etJ7m2YEfP5gZ+KwoptKYppGfpnAuUYUayr9yogepWp2Tn3ngvvxHs7y3eKIiAA==";

    fn component() -> ComponentDefinition {
        ComponentDefinition {
            name: "collab".into(),
            component_version: "1.0.0-abc".into(),
            download_tag: "collab-1.0.0-abc".into(),
            github_repo: None,
            platforms: HashMap::new(),
        }
    }

    fn asset(signature: Option<&str>) -> ComponentPlatformAsset {
        ComponentPlatformAsset {
            asset_name: "collab-1.0.0-abc-darwin-arm64.tar.gz".into(),
            sha256_sidecar_name: None,
            sha256: "a".repeat(64),
            size_bytes: 42,
            signature: signature.map(str::to_string),
        }
    }

    #[test]
    fn only_ghostexs_signature_over_this_exact_archive_passes() {
        // SAFETY: the variable is read only by this module's checks.
        unsafe { std::env::set_var("GHOSTEX_COMPONENT_SIGNING_PUBLIC_KEY", TEST_PUBLIC_KEY) };
        assert_eq!(
            verify_with(
                &component(),
                "darwin-arm64",
                &asset(Some(TEST_SIGNATURE)),
                false
            ),
            Ok(())
        );
        assert!(
            verify_with(
                &component(),
                "linux-x64",
                &asset(Some(TEST_SIGNATURE)),
                false
            )
            .is_err()
        );
        let mut bigger = asset(Some(TEST_SIGNATURE));
        bigger.size_bytes = 43;
        assert!(verify_with(&component(), "darwin-arm64", &bigger, false).is_err());
        assert!(verify_with(&component(), "darwin-arm64", &asset(None), false).is_err());
        assert!(verify_with(&component(), "darwin-arm64", &asset(None), true).is_ok());
        unsafe { std::env::remove_var("GHOSTEX_COMPONENT_SIGNING_PUBLIC_KEY") };
        assert!(
            verify_with(
                &component(),
                "darwin-arm64",
                &asset(Some(TEST_SIGNATURE)),
                false
            )
            .is_err()
        );
    }
}
