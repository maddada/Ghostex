//! `cargo xtask test-server`: a throwaway gxserver for agents and threads to test against, kept apart from the user's real one.
//!
//! CDXC:Build 2026-11-06 DECISION:
//! User: "Build a one-command throwaway gxserver for agents and threads to test against, so nobody has to piece one together by hand or touch the user's real gxserver." Threads each spent about 20 minutes on zmx links, a dev port, GHOSTEX_HOME and the CLI environment, and some left login-recovery LaunchAgents behind.
//! Each server lives in ~/.local/share/agent-work/Ghostex/test-servers/<name>/: `home/` is its GHOSTEX_HOME, `gxserver/bin/` holds copies of the debug gxserver and ghostex CLI next to a `zmx` link (gxserver resolves zmx beside its own binary, server/src/toolchain.rs), and `env.sh` points a shell at it. The copies keep a running server and its CLI stable while other threads rebuild server/target.
//! The server starts with every inherited GHOSTEX_*/ZMX_* variable removed: an agent's shell carries its own session's identity and the real server's base URL and token file, which would otherwise reach the CLI and the sessions it starts.
//! SEE-ALSO: server/src/server/macos_login_recovery.rs (an isolated profile installs no login-recovery agent), AGENTS.md "Test against a throwaway gxserver".

use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::bail;
use crate::gxserver::health;
use crate::util::{home_dir, kill_pid, output, root, sleep_ms, stdout_if_ok, Res};

const USAGE: &str = "Usage: cargo xtask test-server <command>
  start [--name <n>] [--projects <path>...] [--port <p>] [--no-build]
                build (or reuse) the debug gxserver, start it on its own GHOSTEX_HOME and port,
                add the projects, and print the env lines for the ghostex CLI and `ghostex web`
  env <name>    print those env lines again
  stop <name>   close its sessions, stop it, and remove any LaunchAgent it created
  remove <name> stop it and delete its folder
  list          show the test servers and whether they run";

/// Ports the test servers pick from: clear of the real gxserver (58744/58745/58746) and the isolated app variants (58747, 58751).
const PORT_RANGE: std::ops::RangeInclusive<u16> = 58900..=58999;
const RECORD_FILE: &str = "test-server.json";

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
                stop(&Server::load(name)?)?;
            } else if !folder.is_dir() {
                bail!("No test server named {name} ({}).", folder.display());
            }
            fs::remove_dir_all(&folder)?;
            println!("Removed {}.", folder.display());
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
}

impl Server {
    fn new(name: &str, port: u16) -> Self {
        Self {
            name: name.to_string(),
            port,
            folder: servers_root().join(name),
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
        Ok(Self::new(name, port))
    }

    fn home(&self) -> PathBuf {
        self.folder.join("home")
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
    fn label_prefix(&self) -> String {
        // ghostex_paths::launchd_label_prefix for a profile with GHOSTEX_HOME: every launchd job this server creates (zmx sessions, login recovery) starts with it.
        format!("com.madda.ghostex-isolated.{}.", self.port)
    }

    fn token(&self) -> Option<String> {
        let token = fs::read_to_string(self.token_file()).ok()?;
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
            // Skill installs would fetch GitHub main into this throwaway profile on every start.
            ("GHOSTEX_AGENT_SKILLS_REMOTE", "off".into()),
        ]
    }

    /// A command with this server's environment and none of the caller's GHOSTEX_*/ZMX_* variables.
    fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut command = Command::new(program);
        for (key, _) in std::env::vars_os() {
            let key_text = key.to_string_lossy();
            if key_text.starts_with("GHOSTEX_") || key_text.starts_with("ZMX_") {
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
            "# It first drops the GHOSTEX_*/ZMX_* variables an agent's shell carries for the real server.".to_string(),
            "unset $(env | awk -F= '/^(GHOSTEX_|ZMX_)/ {print $1}')".to_string(),
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
            "--no-build" => build = false,
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
    let debug = root().join("server/target/debug");
    if let Err(error) = built {
        if !debug.join("gxserver").is_file() || !debug.join("ghostex").is_file() {
            return Err(error);
        }
        eprintln!(
            "{error}\nReusing the debug binaries already in {}.",
            debug.display()
        );
    }

    if let Some(existing) = name.as_deref().and_then(|name| Server::load(name).ok()) {
        if existing.health().is_some() {
            println!(
                "Test server {} already runs on port {}.",
                existing.name, existing.port
            );
            add_projects(&existing, &projects)?;
            print_env(&existing);
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
    let server = Server::new(&name, port);
    prepare_folder(&server, &debug)?;
    server.write_env_file()?;
    fs::write(
        server.folder.join(RECORD_FILE),
        serde_json::to_string_pretty(&serde_json::json!({
            "name": server.name,
            "port": server.port,
            "home": server.home().display().to_string(),
            "createdBy": "cargo xtask test-server",
        }))? + "\n",
    )?;
    launch(&server)?;
    drop(lock);
    add_projects(&server, &projects)?;
    println!(
        "Test server {} runs on port {} (pid {}, log {}).",
        server.name,
        server.port,
        server.pid().unwrap_or(0),
        server.log_file().display()
    );
    print_env(&server);
    Ok(0)
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

fn prepare_folder(server: &Server, debug: &Path) -> Res {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(server.home())?;
    }
    let bin = server.bin();
    fs::create_dir_all(&bin)?;
    for binary in ["gxserver", "ghostex"] {
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
            return Ok(candidate.clone());
        }
    }
    bail!(
        "No working zmx found. Build it with `zig build` in .dependencies/zmx (as prepare-macos-runtime.sh does) or install Ghostex. Looked at:\n  {}",
        candidates
            .iter()
            .map(|c| c.display().to_string())
            .collect::<Vec<_>>()
            .join("\n  ")
    )
}

fn launch(server: &Server) -> Res {
    let log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(server.log_file())?;
    let mut command = server.command(server.bin().join("gxserver"));
    command
        .arg("--foreground")
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
    let killed = kill_leftover_daemons(&zmx_names);
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
        .filter_map(|entry| Server::load(&entry.file_name().to_string_lossy()).ok())
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
            None => "stopped".to_string(),
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
