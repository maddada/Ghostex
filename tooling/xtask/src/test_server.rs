//! `cargo xtask test-server`: a throwaway gxserver for agents and threads to test against, kept apart from the user's real one.
//!
//! CDXC:Build 2026-11-06 DECISION:
//! User: "Build a one-command throwaway gxserver for agents and threads to test against, so nobody has to piece one together by hand or touch the user's real gxserver." Threads each spent about 20 minutes on zmx links, a dev port, GHOSTEX_HOME and the CLI environment, and some left login-recovery LaunchAgents behind.
//! Each server keeps its record, copies and logs in ~/.local/share/agent-work/Ghostex/test-servers/<name>/ and its GHOSTEX_HOME (state, auth, the zmx socket folder `zmx/`) in /tmp/gxts-<uid>-<name>/ (see `tmp_home`); a server made before that move keeps its `home/` in the folder. `gxserver/bin/` holds copies of the debug gxserver and ghostex CLI next to a `zmx` link (gxserver resolves zmx beside its own binary, server/src/toolchain.rs), and `env.sh` points a shell at it. The copies keep a running server and its CLI stable while other threads rebuild server/target.
//! CDXC:Build 2026-11-09 WHY: the home is in /tmp because a socket path is capped at 103 bytes, and on a computer whose home sits on another volume (`/Volumes/<disk>/<user>`) `<home>/zmx` was already too long for any `--name`, so no test server could start. Keeping the zmx folder inside GHOSTEX_HOME also keeps gxserver's own ownership rule (`zmx_socket_directory_is_shared`: ZMX_DIR inside GHOSTEX_HOME) true, so its unowned-daemon cleanup works without any change in gxserver. /tmp is shared by every account, so the folder is named after the user id and is used only while it is a real directory of this user that nobody else can open (`create_private_home`). A /tmp home disappears when the computer restarts, which is fine for a throwaway server: `remove` still clears its record. The checks stop another account's folder or a link from being used; they do not defend against another process of the same user swapping the folder between check and use, since that process can already read the token file and stop any server directly.
//! The server starts with every inherited GHOSTEX_*/ZMX_* variable removed: an agent's shell carries its own session's identity and the real server's base URL and token file, which would otherwise reach the CLI and the sessions it starts.
//! CDXC:Build 2026-11-06 WHY: the agent CLIs' own variables are removed too (`INHERITED_PREFIXES`). A test server started from a Claude thread inherited its `CLAUDE_CONFIG_DIR` (that thread's claude-swap account folder) and `CLAUDECODE`/`CLAUDE_CODE_*`, so every Claude session it started ran on the thread's account folder instead of the plain login; Codex's `CODEX_HOME` and `CODEX_*` sandbox variables would do the same for Codex.
//! SEE-ALSO: server/src/server/macos_login_recovery.rs (an isolated profile installs no login-recovery agent), AGENTS.md "Test against a throwaway gxserver".

use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::bail;
use crate::gxserver::health;
use crate::util::{home_dir, kill_pid, output, root, sleep_ms, stdout_if_ok, xdg_state_root, Res};

const USAGE: &str = "Usage: cargo xtask test-server <command>
  start [--name <n>] [--projects <path>...] [--port <p>] [--network online|offline|probe] [--no-build] [--dry-run] [--binaries <dir>]
                build (or reuse) the debug gxserver, start it on its own GHOSTEX_HOME and port,
                add the projects, and print the env lines for the ghostex CLI and `ghostex web`;
                --binaries runs the gxserver and ghostex in <dir> instead (an older release's, to check
                compatibility); --network sets what its online check sees (online by default, probe for
                the real check)
  network <name> online|offline|probe
                switch what a running test server's online check sees: offline holds Autopilot and
                coordinator reports as a real outage does, online brings them back, probe uses the real check
  env <name>    print those env lines again
  stop <name>   close its sessions, stop it, and remove any LaunchAgent it created
  remove <name> stop it and delete its folder
  list          show the test servers and whether they run";

/// Ports the test servers pick from: clear of the real gxserver (58744/58745/58746) and the isolated app variants (58747, 58751).
const PORT_RANGE: std::ops::RangeInclusive<u16> = 58900..=58999;
const RECORD_FILE: &str = "test-server.json";
const MAX_NAME_LENGTH: usize = 20;

pub fn run(args: &[String]) -> Res<i32> {
    let Some(command) = args.first() else {
        println!("{USAGE}");
        return Ok(2);
    };
    let rest = &args[1..];
    match command.as_str() {
        "start" => start(rest),
        "env" => {
            let server = Server::load(single_name(rest)?)?;
            print_env(&server);
            Ok(0)
        }
        "stop" => {
            let _lock = lock()?;
            stop(&Server::load(single_name(rest)?)?)?;
            Ok(0)
        }
        "remove" => {
            let _lock = lock()?;
            let name = single_name(rest)?;
            let folder = servers_root().join(name);
            if folder.join(RECORD_FILE).is_file() {
                // Loading checks the recorded home before anything is stopped or deleted; only that server's own home is ever removed.
                let server = Server::load(name)?;
                stop(&server)?;
                // Folders outside the server's own folder: the home, and a socket folder that is not inside it.
                if !server.home.starts_with(&folder) {
                    remove_private_home(&server.home)?;
                }
                if !server.zmx_dir.starts_with(&folder) && !server.zmx_dir.starts_with(&server.home)
                {
                    // A separate socket folder goes only when it is empty: sockets left in it belong to daemons that may still run (stop could not list them), and deleting the folder would make them unreachable.
                    if is_private_directory(&server.zmx_dir).unwrap_or(false)
                        && fs::remove_dir(&server.zmx_dir).is_err()
                    {
                        println!(
                            "Kept {}: it still holds zmx sockets of sessions that may be running.",
                            server.zmx_dir.display()
                        );
                    }
                }
            } else if !folder.is_dir() && fs::symlink_metadata(tmp_home(name)).is_err() {
                bail!("No test server named {name} ({}).", folder.display());
            } else {
                // CDXC:Build 2026-11-10 WHY: a start that died after it made /tmp/gxts-<uid>-<name> but before its record was written leaves that home with no record to name it; `remove` clears it here (it is derived from the name, and only removed while it is a private folder of this user), so the name is free again and nothing lingers in /tmp.
                #[cfg(unix)]
                remove_private_home(&tmp_home(name))?;
            }
            if folder.is_dir() {
                fs::remove_dir_all(&folder)?;
            }
            println!("Removed {}.", folder.display());
            Ok(0)
        }
        "network" => {
            let [name, state] = rest else {
                bail!("Usage: cargo xtask test-server network <name> online|offline|probe");
            };
            validate_name(name)?;
            let _lock = lock()?;
            let server = Server::load(name)?;
            require_network_flag(&server)?;
            set_network(&server, state)?;
            println!("Test server {name}'s online check now sees: {state}.");
            warn_if_other_build(&server);
            Ok(0)
        }
        "list" => list(),
        "help" | "--help" | "-h" => {
            println!("{USAGE}");
            Ok(0)
        }
        other => bail!("unknown test-server command `{other}`.\n\n{USAGE}"),
    }
}

fn servers_root() -> PathBuf {
    home_dir().join(".local/share/agent-work/Ghostex/test-servers")
}

struct Server {
    name: String,
    port: u16,
    folder: PathBuf,
    /// GHOSTEX_HOME: `tmp_home` for a server made now, the record's `home` (`<folder>/home` or `tmp_home`) for one that exists.
    home: PathBuf,
    /// ZMX_DIR: `<home>/zmx`, or `tmp_home` for the few servers made while the socket folder alone was in /tmp (their record names it).
    zmx_dir: PathBuf,
}

impl Server {
    fn new(name: &str, port: u16) -> Self {
        Self {
            name: name.to_string(),
            port,
            folder: servers_root().join(name),
            home: tmp_home(name),
            zmx_dir: tmp_home(name).join("zmx"),
        }
    }

    fn load(name: &str) -> Res<Self> {
        let record_path = servers_root().join(name).join(RECORD_FILE);
        let text = fs::read_to_string(&record_path).map_err(|_| {
            format!(
                "No test server named {name} ({} is missing). `cargo xtask test-server list` shows them.",
                record_path.display()
            )
        })?;
        let record: serde_json::Value = serde_json::from_str(&text)?;
        let Some(port) = record["port"].as_u64().and_then(|p| u16::try_from(p).ok()) else {
            bail!("{} has no port.", record_path.display());
        };
        let mut server = Self::new(name, port);
        // The paths the record names are used only when they are one of the two places this command puts them, and then always in the spelling built here, never the recorded one (a trailing slash compares equal but makes the system follow a link), so a hand-edited record never makes `remove` delete some other folder.
        let own_home = server.folder.join("home");
        server.home = match record["home"].as_str().map(PathBuf::from) {
            None => own_home,
            Some(recorded) if recorded == own_home => own_home,
            Some(recorded) if recorded == tmp_home(name) => tmp_home(name),
            Some(recorded) => bail!(
                "{} names the home {}, which is neither {} nor {}. Nothing was touched; fix or delete that folder by hand.",
                record_path.display(),
                recorded.display(),
                own_home.display(),
                tmp_home(name).display()
            ),
        };
        server.zmx_dir = server.home.join("zmx");
        if let Some(zmx_dir) = record["zmxDir"].as_str().map(PathBuf::from) {
            if zmx_dir == tmp_home(name) {
                server.zmx_dir = tmp_home(name);
            } else if zmx_dir != server.zmx_dir {
                bail!(
                    "{} names the zmx folder {}, not {}. Nothing was touched; fix or delete that folder by hand.",
                    record_path.display(),
                    zmx_dir.display(),
                    server.zmx_dir.display()
                );
            }
        }
        // Before any command reads this server's token or sends it a stop: a home that exists must be a real private directory of this user, not a link to another server's home.
        #[cfg(unix)]
        for dir in [&server.home, &server.zmx_dir] {
            if fs::symlink_metadata(dir).is_ok() && !is_private_directory(dir)? {
                bail!(
                    "{} is not a private folder of this user (a link, another owner or open permissions), so test server {name} is left alone. Nothing was touched; fix or delete it by hand.",
                    dir.display()
                );
            }
        }
        Ok(server)
    }

    fn home(&self) -> PathBuf {
        self.home.clone()
    }
    fn bin(&self) -> PathBuf {
        self.folder.join("gxserver/bin")
    }
    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
    fn token_file(&self) -> PathBuf {
        self.home().join("state/gxserver/auth/token")
    }
    fn pid_file(&self) -> PathBuf {
        self.folder.join("gxserver.pid")
    }
    fn log_file(&self) -> PathBuf {
        self.folder.join("gxserver.log")
    }
    /// Its own zmx socket folder (ZMX_DIR) inside its home, never the default one the real server uses (CDXC:Build 2026-11-06 at `ensure_isolated`).
    fn zmx_dir(&self) -> PathBuf {
        self.zmx_dir.clone()
    }
    fn identity_file(&self) -> PathBuf {
        self.home().join("state/gxserver/identity.json")
    }
    fn server_id(&self) -> Option<String> {
        read_server_id(&self.identity_file())
    }
    fn label_prefix(&self) -> String {
        // ghostex_paths::launchd_label_prefix for a profile with GHOSTEX_HOME: every launchd job this server creates (zmx sessions, login recovery) starts with it.
        format!("com.madda.ghostex-isolated.{}.", self.port)
    }

    fn record_has_own_zmx_dir(&self) -> bool {
        fs::read_to_string(self.folder.join(RECORD_FILE))
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .is_some_and(|record| record["zmxDir"].is_string())
    }

    /// The server's token, read only through a home that is a real private directory of this user at the moment of reading; a missing home (a /tmp home lost in a restart) or a link there means "not running" and nothing is read, so the token is never sent anywhere through it.
    fn token(&self) -> Option<String> {
        let token = read_private_token(&self.home, "state/gxserver/auth/token")?;
        let token = token.trim();
        (!token.is_empty()).then(|| token.to_string())
    }

    fn health(&self) -> Option<serde_json::Value> {
        health(&self.base_url(), &self.token()?, Duration::from_secs(2))
    }

    fn pid(&self) -> Option<u32> {
        fs::read_to_string(self.pid_file())
            .ok()?
            .trim()
            .parse()
            .ok()
    }

    /// The variables that point the ghostex CLI, `ghostex web` and gxserver at this server.
    fn environment(&self) -> Vec<(&'static str, String)> {
        let gxserver = self.bin().join("gxserver").display().to_string();
        vec![
            ("GHOSTEX_HOME", self.home().display().to_string()),
            ("GHOSTEX_GXSERVER_DEV_PORT", self.port.to_string()),
            ("GHOSTEX_GXSERVER_BASE_URL", self.base_url()),
            (
                "GHOSTEX_GXSERVER_AUTH_TOKEN_FILE",
                self.token_file().display().to_string(),
            ),
            ("GHOSTEX_GXSERVER_BIN", gxserver.clone()),
            ("GHOSTEX_GXSERVER_CLI", gxserver),
            ("ZMX_DIR", self.zmx_dir().display().to_string()),
            // Skill installs would fetch GitHub main into this throwaway profile on every start.
            ("GHOSTEX_AGENT_SKILLS_REMOTE", "off".into()),
        ]
    }

    /// A command with this server's environment and none of the caller's inherited Ghostex, zmx or
    /// agent variables (`INHERITED_PREFIXES`).
    fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut command = Command::new(program);
        for (key, _) in std::env::vars_os() {
            if is_inherited_variable(&key.to_string_lossy()) {
                command.env_remove(&key);
            }
        }
        command.envs(self.environment());
        let path = std::env::var("PATH").unwrap_or_default();
        command.env("PATH", format!("{}:{path}", self.bin().display()));
        command
    }

    fn ghostex(&self) -> Command {
        self.command(self.bin().join("ghostex"))
    }

    fn write_env_file(&self) -> Res {
        let mut lines = vec![
            "# Source this to point the ghostex CLI and `ghostex web` at this test server.".to_string(),
            "# It first drops the GHOSTEX_*/ZMX_* variables an agent's shell carries for the real server, and".to_string(),
            "# the agent's own CLAUDE_*/CLAUDECODE/CODEX_* ones (its account folders).".to_string(),
            format!(
                "unset $(env | awk -F= '/^({})/ {{print $1}}')",
                INHERITED_PREFIXES
                    .iter()
                    .map(|prefix| if prefix.ends_with('_') { prefix.to_string() } else { format!("{prefix}=") })
                    .collect::<Vec<_>>()
                    .join("|")
            ),
        ];
        for (key, value) in self.environment() {
            lines.push(format!("export {key}={}", shell_quote(&value)));
        }
        lines.push(format!(
            "export PATH={}:\"$PATH\"",
            shell_quote(&self.bin().display().to_string())
        ));
        fs::write(self.folder.join("env.sh"), lines.join("\n") + "\n")?;
        Ok(())
    }
}

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "/._-:=@+,".contains(c))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn single_name(args: &[String]) -> Res<&str> {
    match args {
        [name] => {
            validate_name(name)?;
            Ok(name.as_str())
        }
        _ => bail!("Name one test server.\n\n{USAGE}"),
    }
}

fn validate_name(name: &str) -> Res {
    if name.is_empty()
        || name.starts_with('.')
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
    {
        bail!("A test server name uses letters, digits, '.', '_' and '-' only (got `{name}`).");
    }
    // The zmx socket path (/tmp/gxts-<uid>-<name>/zmx/<session>) must fit macOS's 104-byte socket address.
    if name.len() > MAX_NAME_LENGTH {
        bail!("A test server name has at most {MAX_NAME_LENGTH} characters (got `{name}`).");
    }
    Ok(())
}

/// One test-server command at a time changes the folder, so two threads starting at once never pick the same port.
fn lock() -> Res<fs::File> {
    let root = servers_root();
    fs::create_dir_all(&root)?;
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(root.join(".lock"))?;
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
            bail!("could not lock {}", root.join(".lock").display());
        }
    }
    Ok(file)
}

fn start(args: &[String]) -> Res<i32> {
    let mut name = None;
    let mut port = None;
    let mut projects = Vec::new();
    let mut build = true;
    let mut dry_run = false;
    let mut binaries: Option<PathBuf> = None;
    // Online unless asked: a throwaway server should not depend on the shell reaching the agents' API hosts (the plan's default was the real check; online is deliberate, since the real check is what failed).
    let mut network: Option<String> = None;
    let mut index = 0;
    while index < args.len() {
        let value = |index: usize| -> Res<&String> {
            args.get(index + 1)
                .ok_or_else(|| format!("{} needs a value.", args[index]).into())
        };
        match args[index].as_str() {
            "--name" => {
                name = Some(value(index)?.clone());
                index += 1;
            }
            "--port" => {
                port = Some(
                    value(index)?
                        .parse::<u16>()
                        .map_err(|_| "--port takes a number")?,
                );
                index += 1;
            }
            "--projects" | "--project" => {
                while let Some(path) = args.get(index + 1).filter(|a| !a.starts_with("--")) {
                    projects.push(path.clone());
                    index += 1;
                }
            }
            "--network" => {
                let state = value(index)?.clone();
                check_network_state(&state)?;
                network = Some(state);
                index += 1;
            }
            "--no-build" => build = false,
            "--dry-run" => dry_run = true,
            // Another build's gxserver and ghostex (an older release's, for a compatibility check): staged instead of
            // server/target/debug, with the same isolation.
            "--binaries" => {
                binaries = Some(std::path::absolute(value(index)?)?);
                build = false;
                index += 1;
            }
            other => bail!("unknown argument {other}.\n\n{USAGE}"),
        }
        index += 1;
    }
    if let Some(name) = &name {
        validate_name(name)?;
    }
    // Kept as typed (made absolute), not canonicalized: macOS may return a differently cased path, which gxserver treats as another project.
    let projects = projects
        .iter()
        .map(|path| {
            let absolute = std::path::absolute(path)?;
            if !absolute.is_dir() {
                bail!("project {path} is not a folder.");
            }
            Ok(absolute)
        })
        .collect::<Res<Vec<PathBuf>>>()?;

    let built = if build { build_binaries() } else { Ok(()) };
    let lock = lock()?;
    let linked = binaries.is_some();
    let debug = binaries.unwrap_or_else(|| root().join("server/target/debug"));
    if let Err(error) = built {
        if !debug.join("gxserver").is_file() || !debug.join("ghostex").is_file() {
            return Err(error);
        }
        eprintln!(
            "{error}\nReusing the debug binaries already in {}.",
            debug.display()
        );
    }

    // Only a missing record means "start fresh": an invalid one is refused, never overwritten.
    let existing = match name.as_deref() {
        Some(name) if servers_root().join(name).join(RECORD_FILE).exists() => {
            Some(Server::load(name)?)
        }
        _ => None,
    };
    if let Some(existing) = &existing {
        if existing.health().is_some() {
            println!(
                "Test server {} already runs on port {}.",
                existing.name, existing.port
            );
            if !existing.record_has_own_zmx_dir() {
                eprintln!("Warning: it was started before test servers had their own zmx folder and server id check. Run `cargo xtask test-server stop {0}` and start it again.", existing.name);
            }
            if let Some(state) = &network {
                require_network_flag(existing)?;
                set_network(existing, state)?;
                println!("Its online check now sees: {state}.");
                warn_if_other_build(existing);
            }
            add_projects(existing, &projects)?;
            print_env(existing);
            return Ok(0);
        }
        if port.is_none() {
            port = Some(existing.port);
        }
    }
    let port = match port {
        Some(port) => {
            if !port_is_free(port) {
                bail!("Port {port} is in use.");
            }
            port
        }
        None => pick_port()?,
    };
    let name = name.unwrap_or_else(|| format!("t{port}"));
    // A server that exists keeps the home its record names, so one made before the move to /tmp keeps working where it is.
    let server = match existing {
        Some(mut existing) => {
            existing.port = port;
            existing
        }
        None => Server::new(&name, port),
    };
    prepare_folder(&server, &debug, linked)?;
    set_network(&server, network.as_deref().unwrap_or("online"))?;
    ensure_isolated(&server)?;
    server.write_env_file()?;
    fs::write(
        server.folder.join(RECORD_FILE),
        serde_json::to_string_pretty(&serde_json::json!({
            "name": server.name,
            "port": server.port,
            "home": server.home().display().to_string(),
            "zmxDir": server.zmx_dir().display().to_string(),
            "createdBy": "cargo xtask test-server",
        }))? + "\n",
    )?;
    if dry_run {
        print_isolation(&server)?;
        return Ok(0);
    }
    launch(&server)?;
    drop(lock);
    add_projects(&server, &projects)?;
    println!(
        "Test server {} runs on port {} (pid {}, log {}), server id {}, zmx folder {}.",
        server.name,
        server.port,
        server.pid().unwrap_or(0),
        server.log_file().display(),
        server.server_id().unwrap_or_else(|| "?".into()),
        server.zmx_dir().display()
    );
    print_env(&server);
    Ok(0)
}

/// The home of a test server made now: /tmp/gxts-<uid>-<name>.
fn tmp_home(name: &str) -> PathBuf {
    #[cfg(unix)]
    let uid = unsafe { libc::getuid() }.to_string();
    #[cfg(not(unix))]
    let uid = std::env::var("USERNAME").unwrap_or_default();
    PathBuf::from(format!("/tmp/gxts-{uid}-{name}"))
}

/// Makes `home` (0700) and refuses it unless it is a real directory of this user that nobody else can open: /tmp is shared by every account, so a folder another account made first, or a link to one, is not this server's.
#[cfg(unix)]
fn create_private_home(home: &Path) -> Res {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(home)?;
    if !is_private_directory(home)? {
        bail!(
            "Refusing to start: {} is not a private folder of this user (a link, another owner or open permissions). Remove it and start again.",
            home.display()
        );
    }
    Ok(())
}

#[cfg(unix)]
fn is_private_directory(path: &Path) -> Res<bool> {
    use std::os::unix::fs::MetadataExt;
    let meta = fs::symlink_metadata(path)?;
    Ok(meta.is_dir() && meta.uid() == unsafe { libc::getuid() } && meta.mode() & 0o077 == 0)
}

/// Reads `relative` inside `home` through one directory handle: the home is opened without following a link, checked (owner and permissions) on that handle, and the file is then opened relative to it, so a link put at the home path after a check cannot redirect the read (CDXC:Build 2026-11-09 WHY: another account can create `/tmp/gxts-<uid>-<name>` as a link to a running server's home while this server's record survives a restart, and the token read through it would be sent to whoever listens on this server's port).
#[cfg(unix)]
fn read_private_token(home: &Path, relative: &str) -> Option<String> {
    use std::ffi::CString;
    use std::io::Read;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::io::FromRawFd;
    let home = CString::new(home.as_os_str().as_bytes()).ok()?;
    let relative = CString::new(relative).ok()?;
    let directory = unsafe {
        libc::open(
            home.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if directory < 0 {
        return None;
    }
    // Closed on every path out of this function, by the File that owns it.
    let directory = unsafe { fs::File::from_raw_fd(directory) };
    let mut stat: libc::stat = unsafe { std::mem::zeroed() };
    use std::os::unix::io::AsRawFd;
    if unsafe { libc::fstat(directory.as_raw_fd(), &mut stat) } != 0
        || stat.st_uid != unsafe { libc::getuid() }
        || u32::from(stat.st_mode) & 0o077 != 0
    {
        return None;
    }
    let file = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            relative.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC,
        )
    };
    if file < 0 {
        return None;
    }
    let mut text = String::new();
    unsafe { fs::File::from_raw_fd(file) }
        .read_to_string(&mut text)
        .ok()?;
    Some(text)
}

/// Deletes a home outside the server's folder, only when it is still the private directory this command made.
#[cfg(unix)]
fn remove_private_home(home: &Path) -> Res {
    if fs::symlink_metadata(home).is_err() {
        return Ok(());
    }
    if !is_private_directory(home)? {
        bail!(
            "Not deleting {}: it is not a private folder of this user (a link, another owner or open permissions).",
            home.display()
        );
    }
    fs::remove_dir_all(home)?;
    Ok(())
}

/// CDXC:Build 2026-11-06 DECISION:
/// User: test servers shared the real zmx socket folder, and one that drew the real server's id (S90; ids are `S` + digit + letter-or-digit, 360 in all) ended every real session as "unowned" each time it started (05:58, 06:09 and 06:17 on 2026-11-06). Each test server therefore gets its own ZMX_DIR and a server id no other gxserver here uses, and this refuses to launch one whose zmx folder is a default (shared) one or whose id is taken. gxserver also no longer ends unowned daemons from a shared folder (`zmx_socket_directory_is_shared`, server/src/zmx/screen_capture.rs).
fn ensure_isolated(server: &Server) -> Res {
    let zmx_dir = server.zmx_dir();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&zmx_dir)?;
        // A socket folder outside the home (the intermediate layout a few servers were made with) is in /tmp too, so it gets the same check as the home.
        if !zmx_dir.starts_with(server.home()) && !is_private_directory(&zmx_dir)? {
            bail!(
                "Refusing to start: {} is not a private folder of this user (a link, another owner or open permissions). Remove it and start again.",
                zmx_dir.display()
            );
        }
    }
    let canonical = fs::canonicalize(&zmx_dir)?;
    for shared in default_zmx_dirs() {
        if fs::canonicalize(&shared).is_ok_and(|shared| shared == canonical) {
            bail!(
                "Refusing to start: the zmx folder {} is the default one the real gxserver uses.",
                canonical.display()
            );
        }
    }
    // The longest session name gxserver makes is about 16 bytes (S90-P1234-G1234).
    if canonical.as_os_str().len() + 1 + 20 > 103 {
        bail!(
            "Refusing to start: {} is too long for zmx socket paths; pick a shorter --name.",
            canonical.display()
        );
    }

    let taken = taken_server_ids(server);
    match server.server_id() {
        Some(id) if taken.contains(&id) => bail!(
            "Refusing to start: this test server's id {id} is already used by another gxserver on this computer (the real one, another test server, or live zmx sessions), and it would treat their sessions as its own. Run `cargo xtask test-server remove {}` and start a new one.",
            server.name
        ),
        Some(_) => Ok(()),
        None => {
            let id = free_server_id(&taken)?;
            if let Some(folder) = server.identity_file().parent() {
                fs::create_dir_all(folder)?;
            }
            fs::write(
                server.identity_file(),
                serde_json::to_string_pretty(&serde_json::json!({
                    "createdAt": "1970-01-01T00:00:00.000Z",
                    "serverId": id,
                }))? + "\n",
            )?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(server.identity_file(), fs::Permissions::from_mode(0o600))?;
            }
            Ok(())
        }
    }
}

/// The socket folders zmx uses without ZMX_DIR (.dependencies/zmx/src/cfg.zig `socketDir`), where the real gxserver's daemons live.
fn default_zmx_dirs() -> Vec<PathBuf> {
    let uid = unsafe { libc::getuid() };
    let temporary = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".into());
    let mut folders = vec![
        PathBuf::from(format!("{}/zmx-{uid}", temporary.trim_end_matches('/'))),
        PathBuf::from(format!("/tmp/zmx-{uid}")),
    ];
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") {
        folders.push(PathBuf::from(runtime).join("zmx"));
    }
    folders
}

fn read_server_id(identity_file: &Path) -> Option<String> {
    let text = fs::read_to_string(identity_file).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value["serverId"].as_str().map(str::to_string)
}

/// Ids in use: the real gxserver's, every other test server's, and every id with a live socket in a default zmx folder.
fn taken_server_ids(server: &Server) -> std::collections::HashSet<String> {
    let mut taken = std::collections::HashSet::new();
    taken.extend(read_server_id(
        &xdg_state_root().join("ghostex/gxserver/identity.json"),
    ));
    for entry in fs::read_dir(servers_root()).into_iter().flatten().flatten() {
        let other = entry.file_name().to_string_lossy().into_owned();
        if other != server.name {
            // Both layouts' identity files, whether or not the record loads: an invalid record can still belong to a running server, and its id must stay taken (an id is not secret, so reading it through a link is harmless).
            taken.extend(read_server_id(
                &entry.path().join("home/state/gxserver/identity.json"),
            ));
            taken.extend(read_server_id(
                &tmp_home(&other).join("state/gxserver/identity.json"),
            ));
        }
    }
    for folder in default_zmx_dirs() {
        for entry in fs::read_dir(folder).into_iter().flatten().flatten() {
            if let Some((id, _)) = entry.file_name().to_string_lossy().split_once('-') {
                taken.insert(id.to_string());
            }
        }
    }
    taken
}

/// A server id (`S` + digit + lowercase letter or digit, as server/src/ids.rs makes) that is not taken, starting at a time-seeded place.
fn free_server_id(taken: &std::collections::HashSet<String>) -> Res<String> {
    let seconds: Vec<char> = "abcdefghijklmnopqrstuvwxyz0123456789".chars().collect();
    let all: Vec<String> = (0..10)
        .flat_map(|digit| seconds.iter().map(move |c| format!("S{digit}{c}")))
        .collect();
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.subsec_nanos() as usize ^ std::process::id() as usize)
        .unwrap_or(0);
    (0..all.len())
        .map(|offset| &all[(seed + offset) % all.len()])
        .find(|id| !taken.contains(*id))
        .cloned()
        .ok_or_else(|| "Every gxserver id is taken.".into())
}

/// What `command` and `env.sh` drop from the caller's environment. A name ending in `_` is a
/// prefix; any other is one exact name (`CLAUDECODE`, set by Claude Code beside its CLAUDE_CODE_*).
const INHERITED_PREFIXES: [&str; 5] = ["GHOSTEX_", "ZMX_", "CLAUDE_", "CLAUDECODE", "CODEX_"];

fn is_inherited_variable(key: &str) -> bool {
    // Collab's development switches (the component to install, a local relay, the unpinned guest
    // page) are meant for the test server (server/share-relay/README.md).
    if key.starts_with("GHOSTEX_COLLAB_") {
        return false;
    }
    // The memory guard's test pressure level and thresholds, read only by a debug gxserver
    // (server/src/memory_guard/pressure.rs and usage.rs).
    if key.starts_with("GHOSTEX_MEMORY_GUARD_TEST_") {
        return false;
    }
    INHERITED_PREFIXES.iter().any(|prefix| {
        if prefix.ends_with('_') {
            key.starts_with(prefix)
        } else {
            key == *prefix
        }
    })
}

/// `start --dry-run`: what the server would see, without launching it.
fn print_isolation(server: &Server) -> Res {
    // What a command the server runs actually gets: the caller's dropped names, and any that survive.
    let dropped = std::env::vars_os()
        .map(|(key, _)| key.to_string_lossy().to_string())
        .filter(|key| is_inherited_variable(key))
        .count();
    let seen = output(&mut server.command("env"))?;
    let leaked = String::from_utf8_lossy(&seen.stdout)
        .lines()
        .filter_map(|line| line.split_once('=').map(|(key, _)| key.to_string()))
        .filter(|key| is_inherited_variable(key))
        .filter(|key| !server.environment().iter().any(|(set, _)| set == key))
        .collect::<Vec<_>>();
    let inherited = format!(
        "  inherited variables dropped: {dropped} ({}); left in the server's environment: [{}]",
        INHERITED_PREFIXES.join(", "),
        leaked.join(", ")
    );
    let real = read_server_id(&xdg_state_root().join("ghostex/gxserver/identity.json"));
    let listed = output(
        server
            .command(server.bin().join("zmx"))
            .args(["list", "--short"]),
    )?;
    println!(
        "Not started (--dry-run).\n  server id: {} (the real gxserver's: {})\n  zmx folder: {}\n  default zmx folders it does not use: {}\n  sessions its zmx sees: [{}]",
        server.server_id().unwrap_or_else(|| "?".into()),
        real.unwrap_or_else(|| "?".into()),
        server.zmx_dir().display(),
        default_zmx_dirs()
            .iter()
            .map(|d| d.display().to_string())
            .collect::<Vec<_>>()
            .join(", "),
        String::from_utf8_lossy(&listed.stdout).split_whitespace().collect::<Vec<_>>().join(", ")
    );
    println!("{inherited}");
    Ok(())
}

fn build_binaries() -> Res {
    println!("Building the debug gxserver and ghostex CLI (server/)...");
    let status = Command::new("cargo")
        .args(["build", "--bin", "gxserver", "--bin", "ghostex"])
        .current_dir(root().join("server"))
        .stdin(Stdio::null())
        .status()?;
    if !status.success() {
        bail!("cargo build in server/ failed.");
    }
    Ok(())
}

fn port_is_free(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

fn pick_port() -> Res<u16> {
    let taken: Vec<u16> = fs::read_dir(servers_root())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| Server::load(&entry.file_name().to_string_lossy()).ok())
        .map(|server| server.port)
        .collect();
    PORT_RANGE
        .clone()
        .find(|port| !taken.contains(port) && port_is_free(*port))
        .ok_or_else(|| format!("No free port in {PORT_RANGE:?}.").into())
}

/// `linked` links the binaries instead of copying them: a signed release binary taken out of its app bundle no longer
/// passes macOS's signature check (amfid kills it), so `--binaries` points at it where it lies.
fn prepare_folder(server: &Server, debug: &Path, linked: bool) -> Res {
    #[cfg(unix)]
    create_private_home(&server.home())?;
    let bin = server.bin();
    fs::create_dir_all(&bin)?;
    for binary in ["gxserver", "ghostex"] {
        #[cfg(unix)]
        if linked {
            let link = bin.join(binary);
            let _ = fs::remove_file(&link);
            std::os::unix::fs::symlink(fs::canonicalize(debug.join(binary))?, &link)?;
            continue;
        }
        // A rename over the old copy: a stopped server's binary may still be mapped by a process that is exiting.
        let staged = bin.join(format!(".{binary}.new"));
        fs::copy(debug.join(binary), &staged)?;
        fs::rename(&staged, bin.join(binary))?;
    }
    let zmx = find_zmx()?;
    let link = bin.join("zmx");
    let _ = fs::remove_file(&link);
    #[cfg(unix)]
    std::os::unix::fs::symlink(&zmx, &link)?;
    Ok(())
}

/// The repository's built zmx first: a test server's daemons then never run from the installed app, whose gxserver treats daemons from its own Resources that it does not track as orphans. The installed app's zmx when the repository has none built.
fn find_zmx() -> Res<PathBuf> {
    let candidates = [
        root().join(".dependencies/zmx/zig-out/bin/zmx"),
        PathBuf::from("/Applications/Ghostex.app/Contents/Resources/Web/gxserver/bin/zmx"),
    ];
    for candidate in &candidates {
        if candidate.is_file()
            && stdout_if_ok(Command::new(candidate).arg("version"))
                .is_some_and(|out| out.contains("wire_generation"))
        {
            if is_debug_zmx(candidate) {
                bail!(
                    "{} is a Debug build: each of its session daemons holds about 86 MB (Zig's debug allocator) instead of about 3 MB. Rebuild it with `zig build -Doptimize=ReleaseSafe` in .dependencies/zmx (as prepare-macos-runtime.sh does), then start the test server again.",
                    candidate.display()
                );
            }
            return Ok(candidate.clone());
        }
    }
    bail!(
        "No working zmx found. Build it with `zig build -Doptimize=ReleaseSafe` in .dependencies/zmx (as prepare-macos-runtime.sh does) or install Ghostex. Looked at:\n  {}",
        candidates
            .iter()
            .map(|c| c.display().to_string())
            .collect::<Vec<_>>()
            .join("\n  ")
    )
}

/// Whether a zmx was built in Debug mode: only a Debug build links Zig's `DebugAllocator` (zmx picks it on `builtin.mode == .debug`), so its symbol name is in the binary.
///
/// CDXC:Zmx 2026-11-12 WHY: a plain `zig build` makes a Debug zmx, and a test server's 64 daemons from one held 2.46 GB (86 MB each against about 3 MB from the ReleaseSafe build the app ships). A daemon keeps running the binary that spawned it, so a later release rebuild of zig-out does not shrink them.
fn is_debug_zmx(path: &Path) -> bool {
    const MARKER: &[u8] = b"heap.debug_allocator.DebugAllocator(";
    fs::read(path).is_ok_and(|bytes| bytes.windows(MARKER.len()).any(|window| window == MARKER))
}

/// CDXC:OfflineHold 2026-11-13 WHY: a test server started from a sandboxed or proxied shell could not connect straight to the agents' API hosts, so it read as offline and held every report; pointing HTTPS_PROXY at its own port was the workaround. gxserver's hidden `--network-status-file` makes its online check read this file instead while it says `online` or `offline` (server/src/network_status/probe.rs), so a test server is online by default and an outage can be tested by switching it, never by cutting the computer's network.
fn network_file(server: &Server) -> PathBuf {
    server.folder.join("network-status")
}

fn check_network_state(state: &str) -> Res {
    if !matches!(state, "online" | "offline" | "probe") {
        bail!("The network state is online, offline or probe (got {state}).");
    }
    Ok(())
}

/// `probe` removes the file, so the server runs the real check.
fn set_network(server: &Server, state: &str) -> Res {
    check_network_state(state)?;
    let file = network_file(server);
    if state == "probe" {
        if file.exists() {
            fs::remove_file(&file)?;
        }
    } else {
        // Written aside and renamed in, so the server never reads a half-written file (which would fall through to the real check for a tick).
        let staged = file.with_extension("new");
        fs::write(&staged, format!("{state}\n"))?;
        fs::rename(&staged, &file)?;
    }
    Ok(())
}

/// A server that runs was started before `--network-status-file` existed when its command line lacks it; it would keep the real check whatever the file says, so the change is refused with the restart that fixes it.
fn require_network_flag(server: &Server) -> Res {
    let Some(pid) = server.pid().filter(|_| server.health().is_some()) else {
        return Ok(());
    };
    let command_line =
        stdout_if_ok(Command::new("ps").args(["-o", "command=", "-p", &pid.to_string()]))
            .unwrap_or_default();
    if !command_line.contains("--network-status-file") {
        bail!(
            "Test server {0} was started without the network switch (before it existed). Restart it: cargo xtask test-server stop {0}, then start --name {0} again.",
            server.name
        );
    }
    Ok(())
}

/// A server started with `--binaries` runs another build's gxserver; one older than the network seam ignores the file, so the switch is only claimed for this checkout's build.
fn warn_if_other_build(server: &Server) {
    if fs::symlink_metadata(server.bin().join("gxserver"))
        .is_ok_and(|meta| meta.file_type().is_symlink())
    {
        println!("Note: this server runs the gxserver given with --binaries. A gxserver from 11.6.0 or earlier ignores the network file and keeps the real check.");
    }
}

fn launch(server: &Server) -> Res {
    let log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(server.log_file())?;
    let mut command = server.command(server.bin().join("gxserver"));
    command
        .arg("--foreground")
        .arg("--network-status-file")
        .arg(network_file(server))
        .current_dir(&server.folder)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Its own process group, so the caller's Ctrl+C or shell exit does not take it down.
        command.process_group(0);
    }
    let mut child = command.spawn()?;
    let pid = child.id();
    fs::write(server.pid_file(), format!("{pid}\n"))?;
    let deadline = Instant::now() + Duration::from_secs(90);
    while Instant::now() < deadline {
        if server.health().is_some() {
            return Ok(());
        }
        if child.try_wait()?.is_some() {
            bail!(
                "gxserver exited before it answered. Last log lines ({}):\n{}",
                server.log_file().display(),
                log_tail(&server.log_file())
            );
        }
        sleep_ms(250);
    }
    bail!(
        "gxserver (pid {pid}) did not answer on {} within 90 seconds. Last log lines:\n{}",
        server.base_url(),
        log_tail(&server.log_file())
    )
}

#[cfg(unix)]
fn pid_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

fn log_tail(path: &Path) -> String {
    let text = fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(20)..].join("\n")
}

fn add_projects(server: &Server, projects: &[PathBuf]) -> Res {
    for project in projects {
        let result = output(
            server
                .ghostex()
                .arg("add-project")
                .arg(project)
                .arg("--json"),
        )?;
        if !result.status.success() {
            bail!(
                "ghostex add-project {} failed:\n{}{}",
                project.display(),
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
        }
        let project_id = serde_json::from_slice::<serde_json::Value>(&result.stdout)
            .ok()
            .and_then(|value| value["project"]["projectId"].as_str().map(str::to_string))
            .unwrap_or_else(|| "?".into());
        println!(
            "Added project {} (--project-id {project_id}).",
            project.display()
        );
    }
    Ok(())
}

fn print_env(server: &Server) {
    let env_file = server.folder.join("env.sh");
    println!(
        "\nUse it from any shell (each agent Bash call is a new shell, so prefix every call):\n  source {}\n  ghostex sessions\n  ghostex create-agent claude --project-id <id>\n  ghostex web --dist-dir \"$PWD/apps/gpui-web/www/dist\" --no-open --port <free port>\n\nThe env lines ({}):\n{}",
        shell_quote(&env_file.display().to_string()),
        env_file.display(),
        fs::read_to_string(&env_file).unwrap_or_default().trim_end()
    );
    println!(
        "\nWhen done: cargo xtask test-server stop {0}   (or remove {0} to delete its folder too)",
        server.name
    );
}

/// Closes the server's sessions, stops it, and clears any launchd job or LaunchAgent left under its label prefix, then any zmx daemon still serving one of its sessions.
fn stop(server: &Server) -> Res {
    let zmx_names = session_zmx_names(server);
    if server.health().is_some() {
        let closed = output(server.ghostex().args(["kill", "all", "--json"]))?;
        let results = serde_json::from_slice::<serde_json::Value>(&closed.stdout)
            .ok()
            .and_then(|value| value["sessions"].as_array().cloned())
            .unwrap_or_default();
        let failed = results.iter().filter(|r| r["ok"] != true).count();
        if closed.status.success() {
            println!(
                "Closed {} session(s){}.",
                results.len() - failed,
                if failed > 0 {
                    format!(", {failed} failed")
                } else {
                    String::new()
                }
            );
        } else {
            eprintln!(
                "ghostex kill all failed (stop-all still kills the daemons): {}{}",
                String::from_utf8_lossy(&closed.stdout).trim(),
                String::from_utf8_lossy(&closed.stderr).trim()
            );
        }
        // A closed session's daemon ends on its own; give it a moment before the control plane goes.
        wait_for_zmx_exit(&zmx_names, Duration::from_secs(5));
        let stopped = output(
            server
                .command(server.bin().join("gxserver"))
                .arg("stop-all"),
        )?;
        println!("{}", String::from_utf8_lossy(&stopped.stdout).trim());
    }
    if let Some(pid) = server.pid() {
        let deadline = Instant::now() + Duration::from_secs(15);
        while pid_alive(pid) && is_this_gxserver(server, pid) && Instant::now() < deadline {
            sleep_ms(200);
        }
        if pid_alive(pid) && is_this_gxserver(server, pid) {
            eprintln!("gxserver pid {pid} did not stop; killing it.");
            kill_pid(pid as i32, true);
        }
        let _ = fs::remove_file(server.pid_file());
    }
    let cleared = clear_launchd(server)?;
    // Only names carrying this server's own id, so nothing of another gxserver is ever signalled.
    let own_prefix = server.server_id().map(|id| format!("{id}-"));
    let own_names: Vec<String> = zmx_names
        .into_iter()
        .filter(|name| {
            own_prefix
                .as_deref()
                .is_some_and(|prefix| name.starts_with(prefix))
        })
        .collect();
    let killed = kill_leftover_daemons(&own_names);
    println!(
        "Test server {} stopped{}{}.",
        server.name,
        if cleared.is_empty() {
            String::new()
        } else {
            format!("; removed launchd jobs/agents: {}", cleared.join(", "))
        },
        if killed.is_empty() {
            String::new()
        } else {
            format!(
                "; killed zmx processes that outlived their closed session: {}",
                killed.join(", ")
            )
        }
    );
    Ok(())
}

/// A recorded pid is only signalled while it is still this server's gxserver, never a process that reused the number.
fn is_this_gxserver(server: &Server, pid: u32) -> bool {
    stdout_if_ok(Command::new("ps").args(["-o", "command=", "-p", &pid.to_string()]))
        .is_some_and(|line| line.contains(&server.bin().join("gxserver").display().to_string()))
}

/// The zmx session names of this server's sessions, read before it stops so a daemon that outlives it can be found.
fn session_zmx_names(server: &Server) -> Vec<String> {
    if server.health().is_none() {
        return Vec::new();
    }
    let Some(text) = stdout_if_ok(server.ghostex().args(["sessions", "--json", "--full"])) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    collect_zmx_names(&serde_json::from_str(&text).unwrap_or_default(), &mut names);
    names.sort();
    names.dedup();
    names
}

fn collect_zmx_names(value: &serde_json::Value, names: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                if matches!(key.as_str(), "zmxName" | "zmxSessionName" | "sessionName") {
                    if let Some(name) = child
                        .as_str()
                        .filter(|n| n.starts_with('S') && n.contains("-P"))
                    {
                        names.push(name.to_string());
                    }
                }
                collect_zmx_names(child, names);
            }
        }
        serde_json::Value::Array(items) => {
            items.iter().for_each(|item| collect_zmx_names(item, names))
        }
        _ => {}
    }
}

#[cfg(target_os = "macos")]
fn clear_launchd(server: &Server) -> Res<Vec<String>> {
    let prefix = server.label_prefix();
    let uid = unsafe { libc::getuid() };
    let mut cleared = Vec::new();
    let listed = stdout_if_ok(Command::new("/bin/launchctl").arg("list")).unwrap_or_default();
    for label in listed
        .lines()
        .filter_map(|line| line.split('\t').nth(2))
        .filter(|label| label.starts_with(&prefix))
    {
        let _ =
            output(Command::new("/bin/launchctl").args(["bootout", &format!("gui/{uid}/{label}")]));
        cleared.push(label.to_string());
    }
    let agents = home_dir().join("Library/LaunchAgents");
    for entry in fs::read_dir(&agents).into_iter().flatten().flatten() {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if file_name.starts_with(&prefix) && file_name.ends_with(".plist") {
            fs::remove_file(entry.path())?;
            cleared.push(entry.path().display().to_string());
        }
    }
    Ok(cleared)
}

#[cfg(not(target_os = "macos"))]
fn clear_launchd(_server: &Server) -> Res<Vec<String>> {
    Ok(Vec::new())
}

/// The zmx processes (pid, `<subcommand> <session>`) serving one of these sessions. They are matched by their `zmx <subcommand> … <session name>` command line; session names carry the server's own id, so the user's sessions never match.
fn zmx_processes(zmx_names: &[String]) -> Vec<(i32, String)> {
    if zmx_names.is_empty() {
        return Vec::new();
    }
    let processes =
        stdout_if_ok(Command::new("ps").args(["-axwwo", "pid=,command="])).unwrap_or_default();
    let mut found = Vec::new();
    for line in processes.lines() {
        let Some((pid, command)) = line.trim().split_once(' ') else {
            continue;
        };
        let words: Vec<&str> = command.split_whitespace().collect();
        let is_zmx = words
            .first()
            .is_some_and(|program| program.ends_with("/zmx") || *program == "zmx");
        if let (true, Ok(pid)) = (is_zmx, pid.parse::<i32>()) {
            if let Some(name) = zmx_names.iter().find(|name| words.contains(&name.as_str())) {
                found.push((pid, format!("{} {name}", words.get(1).unwrap_or(&"?"))));
            }
        }
    }
    found
}

/// Waits up to `timeout` for every zmx process of these sessions to exit.
fn wait_for_zmx_exit(zmx_names: &[String], timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while !zmx_processes(zmx_names).is_empty() && Instant::now() < deadline {
        sleep_ms(200);
    }
}

fn kill_leftover_daemons(zmx_names: &[String]) -> Vec<String> {
    zmx_processes(zmx_names)
        .into_iter()
        .map(|(pid, what)| {
            kill_pid(pid, false);
            format!("zmx {what} (pid {pid})")
        })
        .collect()
}

fn list() -> Res<i32> {
    let mut servers: Vec<Server> = fs::read_dir(servers_root())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            match Server::load(&name) {
                Ok(server) => Some(server),
                Err(error) => {
                    if entry.path().join(RECORD_FILE).exists() {
                        println!("{name:<20} invalid: {error}");
                    }
                    None
                }
            }
        })
        .collect();
    servers.sort_by(|a, b| a.name.cmp(&b.name));
    if servers.is_empty() {
        println!("No test servers ({}).", servers_root().display());
        return Ok(0);
    }
    for server in servers {
        let state = match server.health() {
            Some(health) => format!(
                "running, pid {}",
                health["pid"].as_u64().map_or("?".into(), |p| p.to_string())
            ),
            // A server that answers 401 or stays silent still runs; only a refused connection is stopped.
            None => match server.token().map(|token| {
                crate::gxserver::probe(&server.base_url(), &token, Duration::from_secs(2))
            }) {
                Some(probe) if probe.answered() => format!("running, but {}", probe.describe()),
                Some(crate::gxserver::Probe::NoAnswer(
                    crate::http::NoAnswer::TimedOut | crate::http::NoAnswer::Failed,
                )) => "holds its port but does not answer".to_string(),
                _ => "stopped".to_string(),
            },
        };
        println!(
            "{:<20} port {}  {:<22} {}",
            server.name,
            server.port,
            state,
            server.folder.display()
        );
    }
    Ok(0)
}
