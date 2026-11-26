//! Where the keys are kept: the OS key store (every normal launch), or, for a test copy that opted
//! in with `GHOSTEX_OWNER_KEYS_STORE=file`, plain files inside that copy's own `GHOSTEX_HOME`.

/*
CDXC:Security 2026-11-26 WHY:
A test copy of the app (its own bundle id, GHOSTEX_HOME and test server) asked the login Keychain for "dev.ghostex.keys" as it started, because the owner-key store always used the OS key store under one service name. The user's rule is that test copies never touch the login Keychain (or Credential Manager, or the Secret Service), and the prompt stopped unattended on-screen checks (the #772 check). The test tooling (`cargo xtask test-server`'s env.sh, the Windows smoke launchers, the smoke-test.md launch line) now sets `GHOSTEX_OWNER_KEYS_STORE=file`, and only then are the keys kept in `$GHOSTEX_HOME/owner-keys/`, one file per key, and no OS key store or presence prompt is ever called. It is never inferred from GHOSTEX_HOME alone: a normal launch sets no such variable and behaves exactly as before. An unknown value, or `file` without an absolute GHOSTEX_HOME, is an error rather than a quiet fall back to the OS key store.

What protects the file store: nothing but file permissions. The keys are the raw PKCS#8 bytes, not encrypted; on macOS and Linux the folder is 0700 and each file 0600 (a key file group or others can read is refused), on Windows they take the ACL of the test copy's own folder (the smoke launcher makes `owner-keys` owner-only). Any program running as the user can read them, like Credential Manager and the Secret Service (#495 decision 1A), and unlike the login Keychain. That is acceptable only because a test copy's keys are made for it and enrolled only with its throwaway test server.

The presence prompt (Touch ID or the Mac password, Windows Hello, polkit) is not offered in a file-store copy: `confirm_presence` fails closed with PresenceUnavailable instead of approving. Approving without a prompt would let anything that can start the app with this variable approve as the owner; agents never answer presence prompts anyway, so a test copy loses nothing it could use.

CDXC:Security 2026-11-26 FIX:
Review of d8ef5c609 (M2, L1, L3, L5). A file-store home started again without the variable (a relaunch that kept GHOSTEX_HOME and dropped the opt-in) went back to the OS key store with no word, so the "never touches the Keychain" promise held only for the first start. A home whose `owner-keys/` folder exists is now a file-store home: without the variable every key call is an error, never the OS store, and the relaunch paths carry the variable next to GHOSTEX_HOME. Reads check what writes check (a linked, foreign or group/other-writable folder is refused, the key is opened without following a link and checked on the open handle: a plain file of this user, 0600, one link, and on macOS no ACL entries), an empty key file is BadKey instead of being replaced by a new key, leftover temporary files from a crashed write are deleted, and a GHOSTEX_HOME with spaces around it or a `..` in it is refused.
*/

use std::{
    collections::HashSet,
    ffi::OsString,
    path::{Component, Path, PathBuf},
    sync::Mutex,
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
            |folder| std::fs::symlink_metadata(folder).is_ok(),
        )
    }

    /// `file_store_exists` says whether something is at a home's `owner-keys` path.
    pub(crate) fn from_environment(
        store: Option<OsString>,
        ghostex_home: Option<OsString>,
        file_store_exists: impl Fn(&Path) -> bool,
    ) -> Result<Self, KeyError> {
        let ghostex_home = ghostex_home.filter(|value| !value.is_empty());
        let Some(store) = store.filter(|value| !value.is_empty()) else {
            // A home a test copy made keys in stays a file-store home.
            if let Some(home) = &ghostex_home {
                if file_store_exists(&Path::new(home).join(FILE_STORE_FOLDER)) {
                    return Err(KeyError::KeyStore(format!(
                        "{} keeps a test copy's keys; start it with {STORE_VARIABLE}={FILE_STORE}",
                        Path::new(home).join(FILE_STORE_FOLDER).display()
                    )));
                }
            }
            return Ok(Self::Platform);
        };
        if store != FILE_STORE {
            return Err(KeyError::KeyStore(format!(
                "{STORE_VARIABLE}={} is not a key store Ghostex knows (only `{FILE_STORE}`, for test copies)",
                store.to_string_lossy()
            )));
        }
        let home = ghostex_home.ok_or_else(|| {
            KeyError::KeyStore(format!(
                "{STORE_VARIABLE}={FILE_STORE} needs the test copy's own GHOSTEX_HOME"
            ))
        })?;
        let text = home.to_string_lossy();
        if text.trim() != text {
            return Err(KeyError::KeyStore(format!(
                "{STORE_VARIABLE}={FILE_STORE} needs a GHOSTEX_HOME without spaces around it"
            )));
        }
        let home = PathBuf::from(home);
        if !home.is_absolute() || home.components().any(|part| part == Component::ParentDir) {
            return Err(KeyError::KeyStore(format!(
                "{STORE_VARIABLE}={FILE_STORE} needs an absolute GHOSTEX_HOME without `..`"
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

/// Folders whose leftover temporary files this process already deleted.
static SWEPT: Mutex<Option<HashSet<PathBuf>>> = Mutex::new(None);

mod file {
    use std::{
        fs,
        io::{ErrorKind, Read, Write},
        time::{Duration, SystemTime},
    };

    use super::*;

    /// A temporary file older than this is a crashed write's, never one still being written.
    const STALE_TEMPORARY: Duration = Duration::from_secs(60);

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

    fn refuse(path: &Path, why: &str) -> KeyError {
        KeyError::KeyStore(format!("{} {why}", path.display()))
    }

    pub(super) fn read_secret(folder: &Path, account: &str) -> Result<Option<Vec<u8>>, KeyError> {
        let path = key_path(folder, account)?;
        if !check_folder(folder)? {
            return Ok(None);
        }
        sweep_once(folder);
        let mut file = match open_key(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(store_error("can't read", &path, error)),
        };
        check_key_file(&path, &file)?;
        let mut secret = Vec::new();
        file.read_to_end(&mut secret)
            .map_err(|error| store_error("can't read", &path, error))?;
        // A write renames a whole file into place, so an empty one is damage, not "no key".
        if secret.is_empty() {
            return Err(KeyError::BadKey);
        }
        Ok(Some(secret))
    }

    pub(super) fn write_secret(
        folder: &Path,
        account: &str,
        secret: &[u8],
    ) -> Result<(), KeyError> {
        let path = key_path(folder, account)?;
        prepare_folder(folder)?;
        sweep_once(folder);
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
            fs::rename(&temporary, &path)?;
            // The rename itself is kept too.
            #[cfg(unix)]
            fs::File::open(folder)?.sync_all()?;
            Ok::<(), std::io::Error>(())
        })();
        if let Err(error) = written {
            let _ = fs::remove_file(&temporary);
            return Err(store_error("can't write", &path, error));
        }
        Ok(())
    }

    /// Makes the folder (0700 on macOS and Linux) and checks it as reads do.
    fn prepare_folder(folder: &Path) -> Result<(), KeyError> {
        if let Some(home) = folder.parent() {
            fs::create_dir_all(home).map_err(|error| store_error("can't make", home, error))?;
        }
        #[cfg_attr(not(unix), allow(unused_mut))]
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(folder) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
            Err(error) => return Err(store_error("can't make", folder, error)),
        }
        if !check_folder(folder)? {
            return Err(refuse(folder, "went away while it was made"));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(folder, fs::Permissions::from_mode(0o700))
                .map_err(|error| store_error("can't protect", folder, error))?;
        }
        Ok(())
    }

    /// `false` when there is no folder yet; an error for one that is a link, not this user's, or
    /// one that others can write in (or, on macOS, has ACL entries).
    fn check_folder(folder: &Path) -> Result<bool, KeyError> {
        let metadata = match fs::symlink_metadata(folder) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(store_error("can't read", folder, error)),
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(refuse(folder, "is not a plain folder"));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
            if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return Err(refuse(folder, "is a link, not a plain folder"));
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if metadata.uid() != unsafe { libc::geteuid() } {
                return Err(refuse(folder, "belongs to another user"));
            }
            if metadata.mode() & 0o022 != 0 {
                return Err(refuse(folder, "can be written by other users"));
            }
        }
        #[cfg(target_os = "macos")]
        if acl::path_has_entries(folder)
            .map_err(|error| store_error("can't read", folder, error))?
        {
            return Err(refuse(
                folder,
                "has ACL entries that may let others in (chmod -N it)",
            ));
        }
        Ok(true)
    }

    /// Opens the key without following a link at its path (and, for a FIFO, without waiting).
    fn open_key(path: &Path) -> std::io::Result<fs::File> {
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
            const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
            const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
            options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
            let file = options.open(path)?;
            if file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return Err(std::io::Error::other("the key is a link"));
            }
            return Ok(file);
        }
        #[allow(unreachable_code)]
        options.open(path)
    }

    /// The open key is a plain file of this user that nobody else can read.
    fn check_key_file(path: &Path, file: &fs::File) -> Result<(), KeyError> {
        let metadata = file
            .metadata()
            .map_err(|error| store_error("can't read", path, error))?;
        if !metadata.is_file() {
            return Err(refuse(path, "is not a plain file"));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if metadata.uid() != unsafe { libc::geteuid() } {
                return Err(refuse(path, "belongs to another user"));
            }
            if metadata.mode() & 0o077 != 0 {
                return Err(refuse(path, "can be read by other users (it must be 0600)"));
            }
            if metadata.nlink() != 1 {
                return Err(refuse(path, "has other links to it"));
            }
        }
        #[cfg(target_os = "macos")]
        {
            use std::os::fd::AsRawFd;
            if acl::fd_has_entries(file.as_raw_fd())
                .map_err(|error| store_error("can't read", path, error))?
            {
                return Err(refuse(
                    path,
                    "has ACL entries that may let others read it (chmod -N it)",
                ));
            }
        }
        Ok(())
    }

    /// Deletes what a crashed write left in `folder` (`.<name>.key.<pid>-<n>.tmp` older than a
    /// minute), once per process.
    fn sweep_once(folder: &Path) {
        {
            let mut swept = SWEPT
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if !swept
                .get_or_insert_with(HashSet::new)
                .insert(folder.to_path_buf())
            {
                return;
            }
        }
        let Ok(entries) = fs::read_dir(folder) else {
            return;
        };
        let now = SystemTime::now();
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !(name.starts_with('.') && name.ends_with(".tmp") && name.contains(".key.")) {
                continue;
            }
            let Ok(metadata) = entry.path().symlink_metadata() else {
                continue;
            };
            let stale = metadata
                .modified()
                .ok()
                .and_then(|modified| now.duration_since(modified).ok())
                .is_some_and(|age| age >= STALE_TEMPORARY);
            if stale && !metadata.is_dir() {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}

/// macOS extended ACLs: mode bits say nothing about them, so a 0600 key with an
/// "everyone allow read" entry is readable by everyone.
#[cfg(target_os = "macos")]
mod acl {
    use std::{
        ffi::CString,
        os::raw::{c_char, c_int, c_void},
        path::Path,
    };

    const ACL_TYPE_EXTENDED: c_int = 0x0000_0100;
    const ACL_FIRST_ENTRY: c_int = 0;

    extern "C" {
        fn acl_get_fd_np(fd: c_int, kind: c_int) -> *mut c_void;
        fn acl_get_link_np(path: *const c_char, kind: c_int) -> *mut c_void;
        fn acl_get_entry(acl: *mut c_void, entry_id: c_int, entry: *mut *mut c_void) -> c_int;
        fn acl_free(object: *mut c_void) -> c_int;
    }

    fn has_entries(acl: *mut c_void) -> std::io::Result<bool> {
        if acl.is_null() {
            let error = std::io::Error::last_os_error();
            // No ACL at all.
            return match error.raw_os_error() {
                Some(libc::ENOENT) => Ok(false),
                _ => Err(error),
            };
        }
        let mut entry = std::ptr::null_mut();
        let found = unsafe { acl_get_entry(acl, ACL_FIRST_ENTRY, &mut entry) } == 0;
        unsafe { acl_free(acl) };
        Ok(found)
    }

    pub(super) fn fd_has_entries(fd: c_int) -> std::io::Result<bool> {
        has_entries(unsafe { acl_get_fd_np(fd, ACL_TYPE_EXTENDED) })
    }

    pub(super) fn path_has_entries(path: &Path) -> std::io::Result<bool> {
        use std::os::unix::ffi::OsStrExt;
        let path = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| std::io::Error::other("a NUL in the path"))?;
        has_entries(unsafe { acl_get_link_np(path.as_ptr(), ACL_TYPE_EXTENDED) })
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

    fn environment(store: Option<&str>, home: Option<&str>) -> Result<Store, KeyError> {
        Store::from_environment(store.map(Into::into), home.map(Into::into), |_| false)
    }

    #[test]
    fn no_opt_in_is_the_platform_store() {
        assert_eq!(environment(None, None), Ok(Store::Platform));
        assert_eq!(environment(Some(""), Some("/tmp/x")), Ok(Store::Platform));
        // GHOSTEX_HOME alone never picks the file store.
        assert_eq!(
            environment(None, Some("/tmp/gxts-501-smoke")),
            Ok(Store::Platform)
        );
    }

    #[test]
    fn a_file_store_home_without_the_opt_in_is_an_error() {
        let has_store = |folder: &Path| folder == Path::new("/tmp/gxts-501-smoke/owner-keys");
        for store in [None, Some("")] {
            let result = Store::from_environment(
                store.map(Into::into),
                Some("/tmp/gxts-501-smoke".into()),
                has_store,
            );
            assert!(matches!(result, Err(KeyError::KeyStore(_))), "{result:?}");
        }
        // Without GHOSTEX_HOME (a normal install) nothing is looked at.
        assert_eq!(
            Store::from_environment(None, None, |_| panic!("looked")),
            Ok(Store::Platform)
        );
        assert_eq!(
            Store::from_environment(None, Some("/tmp/other".into()), has_store),
            Ok(Store::Platform)
        );
    }

    #[test]
    fn file_opt_in_keeps_keys_under_ghostex_home() {
        assert_eq!(
            environment(Some("file"), Some("/tmp/gxts-501-smoke")),
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
            (Some("file"), Some("/tmp/x ")),
            (Some("file"), Some(" /tmp/x")),
            (Some("file"), Some("/tmp/x/../y")),
        ] {
            let result = environment(store, home);
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

    #[test]
    fn an_empty_key_file_is_a_bad_key() {
        let folder = scratch("empty");
        let store = Store::File(folder.clone());
        store.write_secret("owner-key", b"secret").unwrap();
        std::fs::write(folder.join("owner-key.key"), b"").unwrap();
        assert_eq!(store.read_secret("owner-key"), Err(KeyError::BadKey));
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_crashed_writes_leftover_is_deleted() {
        let folder = scratch("leftover");
        std::fs::create_dir_all(&folder).unwrap();
        let stale = folder.join(".owner-key.key.1-0.tmp");
        let fresh = folder.join(".owner-key.key.2-0.tmp");
        std::fs::write(&stale, b"half").unwrap();
        std::fs::write(&fresh, b"being written").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&stale)
            .unwrap()
            .set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(600))
            .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        assert_eq!(
            Store::File(folder.clone()).read_secret("owner-key"),
            Ok(None)
        );
        assert!(!stale.exists());
        assert!(fresh.exists(), "a write that may still run is left alone");
        std::fs::remove_dir_all(&folder).unwrap();
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

    #[cfg(unix)]
    #[test]
    fn reads_refuse_a_linked_or_open_folder_and_a_linked_key() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let root = scratch("links");
        let real = root.join("real");
        let store = Store::File(real.clone());
        store.write_secret("owner-key", b"secret").unwrap();

        // The owner-keys folder is a link to the real one.
        let linked = root.join("linked");
        symlink(&real, &linked).unwrap();
        assert!(matches!(
            Store::File(linked).read_secret("owner-key"),
            Err(KeyError::KeyStore(_))
        ));

        // A key that is a link to a 0600 file elsewhere.
        std::fs::write(root.join("elsewhere"), b"planted").unwrap();
        std::fs::set_permissions(
            root.join("elsewhere"),
            std::fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        symlink(root.join("elsewhere"), real.join("approver-key.key")).unwrap();
        assert!(matches!(
            store.read_secret("approver-key"),
            Err(KeyError::KeyStore(_))
        ));

        // A folder others can write in.
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(matches!(
            store.read_secret("owner-key"),
            Err(KeyError::KeyStore(_))
        ));
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(store.read_secret("owner-key"), Ok(Some(b"secret".to_vec())));

        // A key with a second hard link.
        std::fs::hard_link(real.join("owner-key.key"), root.join("second-link")).unwrap();
        assert!(matches!(
            store.read_secret("owner-key"),
            Err(KeyError::KeyStore(_))
        ));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn reads_refuse_a_key_or_folder_with_acl_entries() {
        let folder = scratch("acl");
        let store = Store::File(folder.clone());
        store.write_secret("owner-key", b"secret").unwrap();
        let path = folder.join("owner-key.key");
        let chmod = |args: &[&str], path: &Path| {
            assert!(std::process::Command::new("/bin/chmod")
                .args(args)
                .arg(path)
                .status()
                .unwrap()
                .success());
        };
        chmod(&["+a", "everyone allow read"], &path);
        assert!(matches!(
            store.read_secret("owner-key"),
            Err(KeyError::KeyStore(_))
        ));
        chmod(&["-N"], &path);
        assert_eq!(store.read_secret("owner-key"), Ok(Some(b"secret".to_vec())));
        chmod(&["+a", "everyone allow add_file"], &folder);
        assert!(matches!(
            store.read_secret("owner-key"),
            Err(KeyError::KeyStore(_))
        ));
        chmod(&["-N"], &folder);
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
