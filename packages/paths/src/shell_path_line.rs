//! The line that puts `~/.local/bin` on PATH for the user's shell, which startup file it goes in,
//! and appending it there once.
//!
//! CDXC:Cli 2026-11-25 DECISION:
//! User (picked "Repair adds the line to the shell file itself"): when the `ghostex` and `gx`
//! launchers sit in ~/.local/bin and that folder is off PATH, Settings > Integrations' Repair writes
//! the PATH line into the shell's startup file instead of only showing it, because running the
//! line in a terminal fixes that one terminal only. The write appends one marked line and only when
//! no line there already adds the folder; it never rewrites or reorders the file, follows a
//! symlinked dotfile to its target, and keeps the file's permissions. gxserver does the write
//! (`/api/machineIntegrations` action `addCliFolderToPath`) so every client can ask for it; the
//! desktop reads the same file list here to word its Settings row.
//! SEE-ALSO: server/src/machine_integrations.rs, apps/desktop/src/app/helpers/os_cli/cli_install.rs.

use std::{
    fs,
    io::{self, Write as _},
    path::{Path, PathBuf},
};

/// The marker at the end of the line Ghostex appends, so a person reading the file knows where it
/// came from.
pub const MARKER: &str = "# Added by Ghostex so terminals find ghostex and gx";

/// A shell whose startup file Ghostex knows how to add the line to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellKind {
    Zsh,
    Bash,
    Fish,
    /// sh, dash, ksh: they read ~/.profile.
    Posix,
}

impl ShellKind {
    /// `shell` is a path or a name (`$SHELL`, the passwd entry); `None` for a shell whose syntax
    /// Ghostex does not write (tcsh, nu, xonsh, ...).
    pub fn from_shell(shell: &str) -> Option<Self> {
        let name = shell.trim().rsplit(['/', '\\']).next().unwrap_or_default();
        match name {
            "zsh" => Some(Self::Zsh),
            "bash" => Some(Self::Bash),
            "fish" => Some(Self::Fish),
            "sh" | "dash" | "ksh" | "mksh" => Some(Self::Posix),
            _ => None,
        }
    }

    /// The line as a person would type it (shown with a copy button).
    pub fn command(self) -> &'static str {
        match self {
            Self::Fish => "fish_add_path $HOME/.local/bin",
            _ => "export PATH=\"$HOME/.local/bin:$PATH\"",
        }
    }

    /// The line Ghostex appends: the command and its marker.
    pub fn marked_line(self) -> String {
        format!("{} {MARKER}", self.command())
    }
}

/// The startup files the line goes in for `kind`, under `home`.
///
/// zsh reads ~/.zshrc. bash reads ~/.bashrc in the terminals Linux opens; macOS opens login shells,
/// which read the first of ~/.bash_profile, ~/.bash_login and ~/.profile that exists (so creating
/// ~/.bash_profile beside an existing ~/.profile would stop bash reading ~/.profile), and also gets
/// ~/.bashrc for a bash started inside one. fish reads ~/.config/fish/config.fish.
pub fn startup_files(home: &Path, kind: ShellKind, macos: bool) -> Vec<PathBuf> {
    match kind {
        ShellKind::Zsh => vec![home.join(".zshrc")],
        ShellKind::Bash if macos => {
            let login = [".bash_profile", ".bash_login", ".profile"]
                .into_iter()
                .map(|name| home.join(name))
                .find(|path| fs::symlink_metadata(path).is_ok())
                .unwrap_or_else(|| home.join(".bash_profile"));
            vec![home.join(".bashrc"), login]
        }
        ShellKind::Bash => vec![home.join(".bashrc")],
        ShellKind::Fish => vec![home.join(".config/fish/config.fish")],
        ShellKind::Posix => vec![home.join(".profile")],
    }
}

/// `path` as `~/…` when it is under `home`.
pub fn shown_path(path: &Path, home: &Path) -> String {
    path.strip_prefix(home)
        .map(|rest| format!("~/{}", rest.to_string_lossy()))
        .unwrap_or_else(|_| path.to_string_lossy().into_owned())
}

/// True when an uncommented line of `contents` already adds `.local/bin` to PATH (Ghostex's own
/// line, `export PATH="$HOME/.local/bin:$PATH"`, `path+=(~/.local/bin)`, `fish_add_path
/// ~/.local/bin`, Debian's `PATH="$HOME/.local/bin:$PATH"` inside its `if`, ...). A folder below it
/// (`.local/bin/tools`) does not count.
pub fn contents_add_local_bin(contents: &str) -> bool {
    contents.lines().any(|line| {
        let line = line.trim_start();
        if line.starts_with('#') {
            return false;
        }
        let code = line.split(" #").next().unwrap_or(line);
        let names_path = code.contains("PATH") || code.contains("path");
        names_path
            && code.match_indices(".local/bin").any(|(at, found)| {
                code[at + found.len()..]
                    .chars()
                    .next()
                    .is_none_or(|next| !(next.is_alphanumeric() || "/_-.".contains(next)))
            })
    })
}

/// True when `path` (followed through a link) already adds `.local/bin` to PATH.
pub fn file_adds_local_bin(path: &Path) -> bool {
    fs::read(path)
        .map(|bytes| contents_add_local_bin(&String::from_utf8_lossy(&bytes)))
        .unwrap_or(false)
}

/// What [`add_line`] did to one file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AddOutcome {
    /// The line was appended (`created` when the file did not exist).
    Added { created: bool },
    /// A line there already adds the folder; nothing was written.
    AlreadyThere,
}

/// Appends `kind`'s marked line to `path` unless a line there already adds `.local/bin`. A link is
/// followed to its target (a link to nothing is refused, never replaced); the file is appended to in
/// place, so its permissions, owner and every existing line stay as they were, and it ends with a
/// newline. A missing file is created (and its folder, for fish's config.fish).
pub fn add_line(path: &Path, kind: ShellKind) -> io::Result<AddOutcome> {
    let target = match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => fs::canonicalize(path).map_err(|_| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "{} is a link to a file that isn't there",
                    path.to_string_lossy()
                ),
            )
        })?,
        Ok(meta) if !meta.is_file() => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{} is not a file", path.to_string_lossy()),
            ));
        }
        Ok(_) => path.to_path_buf(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => path.to_path_buf(),
        Err(error) => return Err(error),
    };
    let existing = match fs::read(&target) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    if let Some(bytes) = &existing {
        if contents_add_local_bin(&String::from_utf8_lossy(bytes)) {
            return Ok(AddOutcome::AlreadyThere);
        }
    }
    if existing.is_none() {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
    }
    let mut text = String::new();
    if existing
        .as_ref()
        .is_some_and(|bytes| !bytes.is_empty() && !bytes.ends_with(b"\n"))
    {
        text.push('\n');
    }
    text.push_str(&kind.marked_line());
    text.push('\n');
    let mut options = fs::OpenOptions::new();
    options.append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o644);
    }
    let mut file = options.open(&target)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    Ok(AddOutcome::Added {
        created: existing.is_none(),
    })
}
