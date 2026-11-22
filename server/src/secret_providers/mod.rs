//! Secret providers: password managers Ghostex reads saved keys and tokens from instead of keeping
//! the values itself. 1Password (through its `op` CLI) is the first kind.
//!
//! CDXC:Secrets 2026-11-02 WHY:
//! Providers are kept by gxserver (`auth/secret-providers.json`), not the shared settings file, so gxserver checks every write against what it can read, and an entry of a kind this build does not know survives its saves. Only references are stored; values are read when needed and never cached, logged, or returned to a client.
//! SEE-ALSO: apps/desktop/src/app/window/settings_modal/tabs/secrets.rs (Settings > Secrets), server/src/voice_agent/keys.rs (a key read from 1Password), server/src/secret_requests/ (a secret request answered with a reference).

mod api;
pub(crate) mod one_password;
mod store;

pub(crate) use api::{handle_secret_providers_http, SECRET_PROVIDER_ENDPOINTS};
pub(crate) use store::{resolve, SecretReference};

use crate::paths::GxserverPaths;

/// The first 1Password provider: what a bare `op://…` reference typed into a field is read with.
fn default_one_password(paths: &GxserverPaths) -> Option<store::SecretProvider> {
    store::list(paths)
        .into_iter()
        .find(|provider| provider.kind == store::KIND_ONE_PASSWORD)
}

/// Whether a bare `op://…` reference can be read: a 1Password provider is set up.
pub(crate) fn has_one_password(paths: &GxserverPaths) -> bool {
    default_one_password(paths).is_some()
}

/// A bare `op://vault/item/field` typed where a secret goes, pointed at the first 1Password
/// provider; `None` when `text` is not a reference at all.
pub(crate) fn typed_reference(
    paths: &GxserverPaths,
    text: &str,
) -> Option<Result<SecretReference, String>> {
    let trimmed = text.trim();
    if !trimmed.starts_with("op://") {
        return None;
    }
    if !one_password::is_reference(trimmed) {
        return Some(Err(one_password::INVALID_REFERENCE_MESSAGE.to_string()));
    }
    Some(match default_one_password(paths) {
        Some(provider) => Ok(SecretReference {
            provider: provider.id,
            reference: trimmed.to_string(),
        }),
        None => Err(
            "To use a 1Password reference, add 1Password in Settings > Secrets first.".to_string(),
        ),
    })
}

/// Reads a typed `op://…` reference now (blocking: 1Password may ask to unlock); `None` when the
/// text is an ordinary secret.
pub(crate) fn resolve_typed_reference(
    paths: &GxserverPaths,
    text: &str,
) -> Option<Result<String, String>> {
    typed_reference(paths, text)
        .map(|reference| reference.and_then(|reference| resolve(paths, &reference)))
}

#[cfg(test)]
mod tests;
