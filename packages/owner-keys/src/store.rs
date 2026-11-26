//! Where the keys are kept: the OS key store (every normal launch), or, for a test copy that opted
//! in with `GHOSTEX_OWNER_KEYS_STORE=file`, plain files inside that copy's own `GHOSTEX_HOME`.

/*
CDXC:Security 2026-11-26 WHY:
A test copy of the app (its own bundle id, GHOSTEX_HOME and test server) asked the login Keychain for "dev.ghostex.keys" as it started, because the owner-key store always used the OS key store under one service name. The user's rule is that test copies never touch the login Keychain (or Credential Manager, or the Secret Service), and the prompt stopped unattended on-screen checks (the #772 check). The test tooling (`cargo xtask test-server`'s env.sh, the Windows smoke launchers, the smoke-test.md launch line) now sets `GHOSTEX_OWNER_KEYS_STORE=file`, and only then are the keys kept in `$GHOSTEX_HOME/owner-keys/`, one file per key, and no OS key store or presence prompt is ever called. It is never inferred from GHOSTEX_HOME alone: a normal launch sets no such variable and behaves exactly as before. An unknown value, or `file` without an absolute GHOSTEX_HOME, is an error rather than a quiet fall back to the OS key store.

What protects the file store: nothing but file permissions. The keys are the raw PKCS#8 bytes, not encrypted; on macOS and Linux the folder is 0700 and each file 0600 (a key file group or others can read is refused), on Windows they take the ACL of the test copy's own folder. Any program running as the user can read them, like Credential Manager and the Secret Service (#495 decision 1A), and unlike the login Keychain. That is acceptable only because a test copy's keys are made for it and enrolled only with its throwaway test server.

The presence prompt (Touch ID or the Mac password, Windows Hello, polkit) is not offered in a file-store copy: `confirm_presence` fails closed with PresenceUnavailable instead of approving. Approving without a prompt would let anything that can start the app with this variable approve as the owner; agents never answer presence prompts anyway, so a test copy loses nothing it could use.
*/

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use crate::{platform, KeyError, Presence};

/// The opt-in a test copy's launcher sets.
pub const STORE_VARIABLE: &str = "GHOSTEX_OWNER_KEYS_STORE";
/// The only value of [`STORE_VARIABLE`] there is.
pub const FILE_STORE: &str = "file";
/// The folder inside `GHOSTEX_HOME` that the file store keeps the keys in.
pub const FILE_STORE_FOLDER: &str = "owner-keys";

/// The key store this process uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Store {
    /// The login Keychain, Credential Manager or the Secret Service.
    Platform,
    /// Files in this folder (a test copy's `$GHOSTEX_HOME/owner-keys`).
    File(PathBuf),
}

impl Store {
    /// The store this process's environment asks for.
    pub(crate) fn current() -> Result<Self, KeyError> {
        Self::from_environment(
            std::env::var_os(STORE_VARIABLE),
            std::env::var_os("GHOSTEX_HOME"),
        )
    }

    pub(crate) fn from_environment(
        store: Option<OsString>,
        ghostex_home: Option<OsString>,
    ) -> Result<Self, KeyError> {
        let Some(store) = store.filter(|value| !value.is_empty()) else {
            return Ok(Self::Platform);
        };
        if store != FILE_STORE {
            return Err(KeyError::KeyStore(format!(
                "{STORE_VARIABLE}={} is not a key store Ghostex knows (only `{FILE_STORE}`, for test copies)",
                store.to_string_lossy()
            )));
        }
        let home = ghostex_home
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| {
                KeyError::KeyStore(format!(
                    "{STORE_VARIABLE}={FILE_STORE} needs the test copy's own GHOSTEX_HOME"
                ))
            })?;
        if !home.is_absolute() {
            return Err(KeyError::KeyStore(format!(
                "{STORE_VARIABLE}={FILE_STORE} needs an absolute GHOSTEX_HOME"
            )));
        }
        Ok(Self::File(home.join(FILE_STORE_FOLDER)))
    }

    pub(crate) fn read_secret(&self, account: &str) -> Result<Option<Vec<u8>>, KeyError> {
        match self {
            Self::Platform => platform::read_secret(account),
            Self::File(folder) => file::read_secret(folder, account),
        }
    }

    pub(crate) fn write_secret(&self, account: &str, secret: &[u8]) -> Result<(), KeyError> {
        match self {
            Self::Platform => platform::write_secret(account, secret),
            Self::File(folder) => file::write_secret(folder, account, secret),
        }
    }

    pub(crate) fn confirm_presence(&self, reason: &str) -> Result<Presence, KeyError> {
        match self {
            Self::Platform => platform::confirm_presence(reason),
            Self::File(_) => Err(KeyError::PresenceUnavailable(
                "this test copy keeps its keys in a file and shows no presence prompt".into(),
            )),
        }
    }
}

mod file {
    use std::{
        fs,
        io::{ErrorKind, Write},
    };

    use super::*;

    /// The file for `account`. Accounts are Ghostex's own names (`owner-key`, `owner-key-<scope>`
    /// with letters, digits and `-`), so anything else is refused rather than made into a path.
    fn key_path(folder: &Path, account: &str) -> Result<PathBuf, KeyError> {
        if account.is_empty()
            || account.starts_with('-')
            || !account
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            return Err(KeyError::KeyStore("invalid key name".to_string()));
        }
        Ok(folder.join(format!("{account}.key")))
    }

    fn store_error(what: &str, path: &Path, error: std::io::Error) -> KeyError {
        KeyError::KeyStore(format!("{what} {}: {error}", path.display()))
    }

    pub(super) fn read_secret(folder: &Path, account: &str) -> Result<Option<Vec<u8>>, KeyError> {
        let path = key_path(folder, account)?;
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(store_error("can't read", &path, error)),
        };
        if !metadata.is_file() {
            return Err(KeyError::KeyStore(format!(
                "{} is not a plain file",
                path.display()
            )));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(KeyError::KeyStore(format!(
                    "{} can be read by other users (it must be 0600)",
                    path.display()
                )));
            }
        }
        let secret = fs::read(&path).map_err(|error| store_error("can't read", &path, error))?;
        Ok((!secret.is_empty()).then_some(secret))
    }

    pub(super) fn write_secret(
        folder: &Path,
        account: &str,
        secret: &[u8],
    ) -> Result<(), KeyError> {
        let path = key_path(folder, account)?;
        prepare_folder(folder)?;
        // Written beside it and renamed over it, so a crash never leaves half a key; the name is
        // this process's and this write's, so two threads writing at once never share it.
        static WRITES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let write = WRITES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let temporary = folder.join(format!(".{account}.key.{}-{write}.tmp", std::process::id()));
        let _ = fs::remove_file(&temporary);
        let written = (|| {
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temporary)?;
            file.write_all(secret)?;
            file.sync_all()?;
            fs::rename(&temporary, &path)
        })();
        if let Err(error) = written {
            let _ = fs::remove_file(&temporary);
            return Err(store_error("can't write", &path, error));
        }
        Ok(())
    }

    /// Makes the folder (0700 on macOS and Linux) and refuses one that is a link elsewhere.
    fn prepare_folder(folder: &Path) -> Result<(), KeyError> {
        fs::create_dir_all(folder).map_err(|error| store_error("can't make", folder, error))?;
        let metadata = fs::symlink_metadata(folder)
            .map_err(|error| store_error("can't read", folder, error))?;
        if !metadata.is_dir() {
            return Err(KeyError::KeyStore(format!(
                "{} is not a plain folder",
                folder.display()
            )));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(folder, fs::Permissions::from_mode(0o700))
                .map_err(|error| store_error("can't protect", folder, error))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "ghostex-owner-keys-test-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&folder);
        folder
    }

    #[test]
    fn no_opt_in_is_the_platform_store() {
        assert_eq!(Store::from_environment(None, None), Ok(Store::Platform));
        assert_eq!(
            Store::from_environment(Some("".into()), Some("/tmp/x".into())),
            Ok(Store::Platform)
        );
        // GHOSTEX_HOME alone never picks the file store.
        assert_eq!(
            Store::from_environment(None, Some("/tmp/gxts-501-smoke".into())),
            Ok(Store::Platform)
        );
    }

    #[test]
    fn file_opt_in_keeps_keys_under_ghostex_home() {
        assert_eq!(
            Store::from_environment(Some("file".into()), Some("/tmp/gxts-501-smoke".into())),
            Ok(Store::File(PathBuf::from("/tmp/gxts-501-smoke/owner-keys")))
        );
    }

    #[test]
    fn a_bad_opt_in_is_an_error_not_the_platform_store() {
        for (store, home) in [
            (Some("File"), Some("/tmp/x")),
            (Some("keychain"), Some("/tmp/x")),
            (Some("file"), None),
            (Some("file"), Some("")),
            (Some("file"), Some("relative/home")),
        ] {
            let result = Store::from_environment(store.map(Into::into), home.map(Into::into));
            assert!(
                matches!(result, Err(KeyError::KeyStore(_))),
                "{store:?} {home:?} gave {result:?}"
            );
        }
    }

    #[test]
    fn file_store_round_trips_and_overwrites() {
        let folder = scratch("round-trip");
        let store = Store::File(folder.clone());
        assert_eq!(store.read_secret("owner-key"), Ok(None));
        store.write_secret("owner-key", b"first").unwrap();
        assert_eq!(store.read_secret("owner-key"), Ok(Some(b"first".to_vec())));
        store.write_secret("owner-key", b"second").unwrap();
        assert_eq!(store.read_secret("owner-key"), Ok(Some(b"second".to_vec())));
        assert_eq!(store.read_secret("approver-key"), Ok(None));
        // Nothing but the key itself is left in the folder.
        let names: Vec<_> = std::fs::read_dir(&folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names, vec!["owner-key.key".to_string()]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&folder), 0o700);
            assert_eq!(mode(&folder.join("owner-key.key")), 0o600);
        }
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn file_store_refuses_odd_names() {
        let store = Store::File(scratch("names"));
        for account in ["", "../owner-key", "owner/key", "-x", "owner.key"] {
            assert!(
                matches!(store.read_secret(account), Err(KeyError::KeyStore(_))),
                "{account:?}"
            );
            assert!(
                matches!(
                    store.write_secret(account, b"x"),
                    Err(KeyError::KeyStore(_))
                ),
                "{account:?}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn file_store_refuses_a_key_others_can_read() {
        use std::os::unix::fs::PermissionsExt;
        let folder = scratch("loose");
        let store = Store::File(folder.clone());
        store.write_secret("owner-key", b"secret").unwrap();
        let path = folder.join("owner-key.key");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(
            store.read_secret("owner-key"),
            Err(KeyError::KeyStore(_))
        ));
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn file_store_has_no_presence_prompt() {
        let store = Store::File(scratch("presence"));
        assert!(matches!(
            store.confirm_presence("approve something"),
            Err(KeyError::PresenceUnavailable(_))
        ));
    }
}
