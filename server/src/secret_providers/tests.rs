//! Every test reads through a fake `op` script, never the user's real 1Password.

use std::path::{Path, PathBuf};

use super::{one_password, resolve, resolve_typed_reference, store, SecretReference};
use crate::paths::get_gxserver_paths;

/// A stand-in `op`: `read` answers one known reference with a made-up value and fails otherwise;
/// `account list` lists one made-up account.
fn fake_op(dir: &Path) -> PathBuf {
    let script = dir.join("op");
    std::fs::write(
        &script,
        r#"#!/bin/sh
case "$*" in
  *"account list"*) printf '[{"url":"example.1password.com","email":"someone@example.com","user_uuid":"U","account_uuid":"A"}]' ;;
  *"read --account=example.1password.com --no-newline op://Private/Example/credential") printf 'fake-value-123' ;;
  *) echo "[ERROR] not found" >&2; exit 1 ;;
esac
"#,
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    one_password::FAKE_OP.with(|fake| *fake.borrow_mut() = Some(script.clone()));
    script
}

#[test]
#[cfg(unix)]
fn a_provider_reads_through_op_and_reports_its_account() {
    let temp = tempfile::tempdir().unwrap();
    fake_op(temp.path());
    let paths = get_gxserver_paths(Some(temp.path().join("home")));

    let provider = store::save(
        &paths,
        None,
        "onePassword",
        "Personal",
        "example.1password.com",
    )
    .unwrap();
    assert_eq!(provider.id, "personal");
    let settings = store::read_for_settings(&paths);
    assert_eq!(settings["providers"][0]["status"], "ready");
    assert_eq!(settings["onePasswordCliInstalled"], true);

    let reference = SecretReference {
        provider: "personal".to_string(),
        reference: "op://Private/Example/credential".to_string(),
    };
    assert_eq!(resolve(&paths, &reference).unwrap(), "fake-value-123");
    let missing = SecretReference {
        reference: "op://Private/Missing/credential".to_string(),
        ..reference.clone()
    };
    let error = resolve(&paths, &missing).unwrap_err();
    assert!(
        !error.contains("not found"),
        "op's own output stays out of messages"
    );

    // A bare reference typed into a secret field goes through the first 1Password provider.
    assert_eq!(
        resolve_typed_reference(&paths, " op://Private/Example/credential ")
            .unwrap()
            .unwrap(),
        "fake-value-123"
    );
    assert!(resolve_typed_reference(&paths, "plain-secret").is_none());
    assert!(resolve_typed_reference(&paths, "op://bad")
        .unwrap()
        .is_err());
}

#[test]
#[cfg(unix)]
fn writes_are_checked_and_unknown_accounts_flagged() {
    let temp = tempfile::tempdir().unwrap();
    fake_op(temp.path());
    let paths = get_gxserver_paths(Some(temp.path().join("home")));

    assert!(store::save(&paths, None, "onePassword", "Work", "-rf").is_err());
    assert!(store::save(&paths, None, "onePassword", "", "work.1password.com").is_err());
    assert!(store::save(&paths, None, "vault", "Work", "work.1password.com").is_err());
    store::save(&paths, None, "onePassword", "Work", "work.1password.com").unwrap();
    assert!(store::save(&paths, None, "onePassword", "work", "x.1password.com").is_err());
    let settings = store::read_for_settings(&paths);
    assert_eq!(settings["providers"][0]["status"], "accountMissing");

    let edited = store::save(
        &paths,
        Some("work"),
        "onePassword",
        "Work team",
        "example.1password.com",
    )
    .unwrap();
    assert_eq!(edited.id, "work");
    assert!(store::remove(&paths, "work").unwrap());
    assert!(!store::remove(&paths, "work").unwrap());
    // With no provider, a typed reference says how to set one up.
    assert!(
        resolve_typed_reference(&paths, "op://Private/Example/credential")
            .unwrap()
            .unwrap_err()
            .contains("Settings > Secrets")
    );
}

#[test]
#[cfg(unix)]
fn the_providers_file_is_owner_only_and_keeps_unknown_kinds() {
    let temp = tempfile::tempdir().unwrap();
    fake_op(temp.path());
    let paths = get_gxserver_paths(Some(temp.path().join("home")));
    let file = paths.auth_dir.join("secret-providers.json");
    std::fs::create_dir_all(&paths.auth_dir).unwrap();
    std::fs::write(
        &file,
        r#"{"providers":[{"id":"later","kind":"someFutureKind","label":"Later","account":"x"}]}"#,
    )
    .unwrap();
    store::save(
        &paths,
        None,
        "onePassword",
        "Personal",
        "example.1password.com",
    )
    .unwrap();
    let kept = store::list(&paths);
    assert_eq!(kept.len(), 2);
    assert_eq!(kept[0].kind, "someFutureKind");
    assert_eq!(
        store::read_for_settings(&paths)["providers"][0]["status"],
        "unsupported"
    );
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
}
