//! The public API under `GHOSTEX_OWNER_KEYS_STORE=file`, as a test copy runs it. One test in its
//! own binary, because it sets the process's environment. Every call here goes to the file store
//! or fails before reaching a key store, so it never asks the login Keychain, Credential Manager
//! or the Secret Service for anything.

use ghostex_owner_keys::{
    approver_public_key, approver_public_key_or_create, confirm_presence, load_or_create_owner_key,
    load_or_create_scoped_owner_key, load_scoped_owner_key, local_salt, opted_in_to_file_store,
    store_problem, unlock_approver_key, uses_file_store, KeyError, FILE_STORE_FOLDER,
    STORE_VARIABLE,
};

#[test]
fn a_test_copy_keeps_its_keys_in_its_own_home() {
    let home =
        std::env::temp_dir().join(format!("ghostex-owner-keys-opt-in-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);

    // A bad opt-in fails before any key store is asked.
    std::env::set_var(STORE_VARIABLE, "keychain");
    std::env::set_var("GHOSTEX_HOME", &home);
    assert!(matches!(
        load_or_create_owner_key(),
        Err(KeyError::KeyStore(_))
    ));
    assert!(matches!(confirm_presence("x"), Err(KeyError::KeyStore(_))));
    std::env::set_var(STORE_VARIABLE, "file");
    std::env::remove_var("GHOSTEX_HOME");
    assert!(matches!(
        load_or_create_owner_key(),
        Err(KeyError::KeyStore(_))
    ));
    std::env::set_var("GHOSTEX_HOME", "relative/home");
    assert!(matches!(local_salt(), Err(KeyError::KeyStore(_))));

    std::env::set_var("GHOSTEX_HOME", &home);
    let owner = load_or_create_owner_key().unwrap();
    assert_eq!(
        load_or_create_owner_key().unwrap().public_key_b64(),
        owner.public_key_b64(),
        "the second start reads the same owner key"
    );
    assert!(owner.sign_b64(b"challenge").is_ok());

    let salt = local_salt().unwrap();
    assert_eq!(salt.len(), 32);
    assert_eq!(local_salt().unwrap(), salt);

    assert!(load_scoped_owner_key("m7-arch").unwrap().is_none());
    let scoped = load_or_create_scoped_owner_key("m7-arch").unwrap();
    assert_ne!(scoped.public_key_b64(), owner.public_key_b64());
    assert_eq!(
        load_scoped_owner_key("m7-arch")
            .unwrap()
            .unwrap()
            .public_key_b64(),
        scoped.public_key_b64()
    );

    assert_eq!(approver_public_key(), Ok(None));
    let approver = approver_public_key_or_create().unwrap();
    assert_eq!(approver_public_key(), Ok(Some(approver)));

    // No presence prompt: approving fails closed.
    assert!(matches!(
        confirm_presence("x"),
        Err(KeyError::PresenceUnavailable(_))
    ));
    assert!(matches!(
        unlock_approver_key("x"),
        Err(KeyError::PresenceUnavailable(_))
    ));

    assert!(uses_file_store());
    assert!(opted_in_to_file_store());
    assert_eq!(store_problem(), None);

    // The same home started again without the opt-in never falls back to the OS key store.
    std::env::remove_var(STORE_VARIABLE);
    assert!(!uses_file_store());
    assert!(!opted_in_to_file_store());
    assert!(store_problem().is_some());
    assert!(matches!(
        load_or_create_owner_key(),
        Err(KeyError::KeyStore(_))
    ));
    assert!(matches!(local_salt(), Err(KeyError::KeyStore(_))));
    assert!(matches!(approver_public_key(), Err(KeyError::KeyStore(_))));
    assert!(matches!(confirm_presence("x"), Err(KeyError::KeyStore(_))));
    std::env::set_var(STORE_VARIABLE, "file");

    let mut files: Vec<String> = std::fs::read_dir(home.join(FILE_STORE_FOLDER))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    files.sort();
    assert_eq!(
        files,
        [
            "approver-key.key",
            "owner-key-m7-arch.key",
            "owner-key-salt.key",
            "owner-key.key"
        ]
    );
    std::fs::remove_dir_all(&home).unwrap();
}
