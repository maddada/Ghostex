//! The process rows below the zmx session roots, read from the kernel in-process (macOS).
//!
//! CDXC:Zmx 2026-11-04 WHY:
//! The session process snapshot used to run `zsh -c 'zmx list; ps -axo pid=,ppid=,tty=,command='`.
//! `ps` reads the argv of every process on the machine: about 140 ms of CPU each time on a Mac with
//! 1,400 processes, and presentation polls asked for a snapshot about every 2.7 seconds (measured
//! 2026-11-04, issues-1027/macos-perf). Identity resolution only walks down from each session's
//! root, so only those processes need their argv: parent links come from one pass of
//! `proc_listallpids` + `PROC_PIDT_SHORTBSDINFO`, argv from one reused `KERN_PROCARGS2` buffer,
//! and tty names from a cache (`devname` is slow). Rows keep the shape `ps` gave them (argv
//! joined by spaces, `(comm)` when argv is unreadable, the tty's device name), so the same
//! resolver reads them. Linux keeps the `ps` script.
//! SEE-ALSO: provider.rs (`read_zmx_session_process_identities`), process_identity.rs.

use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::CStr;
use std::sync::Mutex;

use super::ProcessRow;

/// Bounds the walk: a session tree deeper or wider than this is not one the resolver can use.
const MAX_TREE_PROCESSES: usize = 4096;

/// Every process at or below `root_pids`, roots included, as `ps -axo pid,ppid,tty,command` rows.
pub(crate) fn process_tree_rows(root_pids: &[i64]) -> Vec<ProcessRow> {
    // One pass over the process table for parent links (`proc_listchildpids` walks the whole
    // kernel list on every call). The short record is the cheaper read, and it answers for more
    // processes than the full one does.
    let mut short_infos = HashMap::new();
    let mut children = HashMap::<i32, Vec<i32>>::new();
    for pid in all_pids() {
        if let Some(info) = short_info(pid) {
            children.entry(info.pbsi_ppid as i32).or_default().push(pid);
            short_infos.insert(pid, info);
        }
    }
    let mut arguments = ProcArgs::new();
    let mut rows = Vec::new();
    let mut seen = HashSet::new();
    let mut queue = root_pids
        .iter()
        .filter_map(|pid| i32::try_from(*pid).ok().filter(|pid| *pid > 0))
        .collect::<VecDeque<_>>();
    while let Some(pid) = queue.pop_front() {
        if rows.len() >= MAX_TREE_PROCESSES || !seen.insert(pid) {
            continue;
        }
        let Some(short) = short_infos.get(&pid) else {
            continue;
        };
        // Only the full record carries the tty; without it the row has none, as `ps` shows `??`.
        let terminal = bsd_info(pid).and_then(|info| terminal_name(info.e_tdev));
        let command = arguments
            .read(pid)
            .unwrap_or_else(|| format!("({})", c_name(&short.pbsi_comm)));
        rows.push(ProcessRow::new(
            i64::from(pid),
            i64::from(short.pbsi_ppid),
            terminal,
            command,
        ));
        if let Some(kids) = children.get(&pid) {
            queue.extend(kids.iter().copied());
        }
    }
    rows
}

fn all_pids() -> Vec<i32> {
    let mut pids = vec![0 as libc::pid_t; 4096];
    loop {
        let size = (pids.len() * std::mem::size_of::<libc::pid_t>()) as libc::c_int;
        let count = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), size) };
        if count <= 0 {
            return Vec::new();
        }
        // A full buffer may have cut the list short; ask again with room to spare.
        if count as usize >= pids.len() && pids.len() < 1 << 20 {
            pids.resize(pids.len() * 4, 0);
            continue;
        }
        pids.truncate(count as usize);
        pids.retain(|pid| *pid > 0);
        return pids;
    }
}

/// `proc_pidinfo` into a plain C record; only a complete reply is read.
fn pid_info<T>(pid: i32, flavor: libc::c_int) -> Option<T> {
    let mut info: T = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<T>() as i32;
    let read = unsafe { libc::proc_pidinfo(pid, flavor, 0, (&mut info as *mut T).cast(), size) };
    (read == size).then_some(info)
}

fn short_info(pid: i32) -> Option<libc::proc_bsdshortinfo> {
    pid_info(pid, libc::PROC_PIDT_SHORTBSDINFO)
}

fn bsd_info(pid: i32) -> Option<libc::proc_bsdinfo> {
    pid_info(pid, libc::PROC_PIDTBSDINFO)
}

/// Device names by device number. `devname` searches `/dev` and took about half a millisecond a
/// call (measured 2026-11-04), more than the rest of a snapshot together; a pty's number always
/// names the same device, so each is looked up once.
static TERMINAL_NAMES: Mutex<Option<HashMap<u32, Option<String>>>> = Mutex::new(None);

/// The tty's device name as `ps -o tty` prints it (`ttys003`), or `None` without one.
fn terminal_name(device: u32) -> Option<String> {
    // NODEV
    if device == u32::MAX || device == 0 {
        return None;
    }
    let mut names = TERMINAL_NAMES.lock().unwrap_or_else(|e| e.into_inner());
    let names = names.get_or_insert_with(HashMap::new);
    if let Some(name) = names.get(&device) {
        return name.clone();
    }
    if names.len() >= 4096 {
        names.clear();
    }
    let name = unsafe { libc::devname(device as libc::dev_t, libc::S_IFCHR) };
    let name = (!name.is_null())
        .then(|| unsafe { CStr::from_ptr(name) }.to_str().ok().map(str::trim))
        .flatten()
        .filter(|name| !name.is_empty() && *name != "??")
        .map(str::to_string);
    names.insert(device, name.clone());
    name
}

fn c_name(raw: &[libc::c_char]) -> String {
    let bytes = raw
        .iter()
        .take_while(|byte| **byte != 0)
        .map(|byte| *byte as u8)
        .collect::<Vec<_>>();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// One `KERN_PROCARGS2` buffer reused for every process of a snapshot.
struct ProcArgs {
    buffer: Vec<u8>,
}

impl ProcArgs {
    fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    /// argv joined by spaces, as `ps -o command` prints it.
    fn read(&mut self, pid: i32) -> Option<String> {
        parse_procargs(self.read_raw(pid)?)
    }

    /// The raw `KERN_PROCARGS2` reply for `pid`.
    fn read_raw(&mut self, pid: i32) -> Option<&[u8]> {
        if self.buffer.is_empty() {
            // macOS rejects PROCARGS2 buffers larger than kern.argmax with EINVAL.
            let mut argmax = 0_i32;
            let mut size = std::mem::size_of_val(&argmax);
            let mut mib = [libc::CTL_KERN, libc::KERN_ARGMAX];
            let result = unsafe {
                libc::sysctl(
                    mib.as_mut_ptr(),
                    mib.len() as u32,
                    (&mut argmax as *mut i32).cast(),
                    &mut size,
                    std::ptr::null_mut(),
                    0,
                )
            };
            if result != 0 || argmax <= 0 {
                return None;
            }
            self.buffer = vec![0; argmax as usize];
        }
        let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
        let mut size = self.buffer.len();
        // KERN_PROCARGS2 returns argc, the executable path, padding, argv, then the environment.
        let result = unsafe {
            libc::sysctl(
                mib.as_mut_ptr(),
                mib.len() as u32,
                self.buffer.as_mut_ptr().cast(),
                &mut size,
                std::ptr::null_mut(),
                0,
            )
        };
        if result != 0 {
            return None;
        }
        Some(&self.buffer[..size.min(self.buffer.len())])
    }
}

/// The value of `name` in the environment `pid` started with (only a process of this user is readable).
pub(crate) fn process_environment_value(pid: i64, name: &str) -> Option<String> {
    let pid = i32::try_from(pid).ok().filter(|pid| *pid > 0)?;
    procargs_environment_value(ProcArgs::new().read_raw(pid)?, name)
}

/// When `pid` started (seconds, microseconds), from the kernel's process record; `None` once it is gone.
pub(crate) fn process_start_time(pid: i64) -> Option<(u64, u64)> {
    let pid = i32::try_from(pid).ok().filter(|pid| *pid > 0)?;
    bsd_info(pid).map(|info| (info.pbi_start_tvsec, info.pbi_start_tvusec))
}

/// The argv `pid` runs, joined by spaces as `ps -o command` prints it.
pub(crate) fn process_command(pid: i64) -> Option<String> {
    let pid = i32::try_from(pid).ok().filter(|pid| *pid > 0)?;
    ProcArgs::new().read(pid)
}

/// Every variable of the environment `pid` started with; `None` when it cannot be read.
pub(crate) fn process_environment(pid: i64) -> Option<std::collections::HashMap<String, String>> {
    let pid = i32::try_from(pid).ok().filter(|pid| *pid > 0)?;
    procargs_environment(ProcArgs::new().read_raw(pid)?)
}

fn procargs_environment(bytes: &[u8]) -> Option<std::collections::HashMap<String, String>> {
    let argc = i32::from_ne_bytes(bytes.get(..4)?.try_into().ok()?);
    let mut offset = 4;
    offset += bytes.get(offset..)?.iter().position(|byte| *byte == 0)? + 1;
    while bytes.get(offset) == Some(&0) {
        offset += 1;
    }
    let mut strings = bytes.get(offset..)?.split(|byte| *byte == 0);
    for _ in 0..argc.max(0) {
        strings.next()?;
    }
    // The environment ends at the first empty string; what follows is the kernel's apple strings.
    Some(
        strings
            .take_while(|entry| !entry.is_empty())
            .filter_map(|entry| {
                let entry = String::from_utf8_lossy(entry);
                let (name, value) = entry.split_once('=')?;
                Some((name.to_string(), value.to_string()))
            })
            .collect(),
    )
}

fn procargs_environment_value(bytes: &[u8], name: &str) -> Option<String> {
    let argc = i32::from_ne_bytes(bytes.get(..4)?.try_into().ok()?);
    let mut offset = 4;
    offset += bytes.get(offset..)?.iter().position(|byte| *byte == 0)? + 1;
    while bytes.get(offset) == Some(&0) {
        offset += 1;
    }
    let mut strings = bytes.get(offset..)?.split(|byte| *byte == 0);
    for _ in 0..argc.max(0) {
        strings.next()?;
    }
    let prefix = format!("{name}=");
    // The environment ends at the first empty string; what follows is the kernel's apple strings.
    strings
        .take_while(|entry| !entry.is_empty())
        .find_map(|entry| entry.strip_prefix(prefix.as_bytes()))
        .map(|value| String::from_utf8_lossy(value).into_owned())
}

fn parse_procargs(bytes: &[u8]) -> Option<String> {
    let argc = i32::from_ne_bytes(bytes.get(..4)?.try_into().ok()?);
    if argc <= 0 {
        return None;
    }
    let mut offset = 4;
    offset += bytes.get(offset..)?.iter().position(|byte| *byte == 0)? + 1;
    while bytes.get(offset) == Some(&0) {
        offset += 1;
    }
    let mut arguments = Vec::with_capacity(argc as usize);
    for _ in 0..argc {
        let rest = bytes.get(offset..)?;
        let length = rest
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(rest.len());
        arguments.push(String::from_utf8_lossy(&rest[..length]).into_owned());
        offset += length + 1;
    }
    // `ps` prints control characters as they are; the row parser splits on whitespace anyway.
    Some(arguments.join(" ").replace(['\n', '\r', '\t'], " "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn procargs_join_argv_and_skip_the_executable_path_and_padding() {
        let mut bytes = 3_i32.to_ne_bytes().to_vec();
        bytes.extend_from_slice(b"/usr/local/bin/claude\0\0\0\0claude\0--resume\0abc\0PATH=/bin\0");
        assert_eq!(
            parse_procargs(&bytes).as_deref(),
            Some("claude --resume abc")
        );
    }

    #[test]
    fn procargs_environment_skips_argv_and_stops_at_the_apple_strings() {
        let mut bytes = 2_i32.to_ne_bytes().to_vec();
        bytes.extend_from_slice(
            b"/bin/zmx\0\0\0zmx\0HOME=/argv-not-env\0HOME=/Users/a\0EMPTY=\0\0executable_path=/bin/zmx\0",
        );
        assert_eq!(
            procargs_environment_value(&bytes, "HOME").as_deref(),
            Some("/Users/a")
        );
        assert_eq!(
            procargs_environment_value(&bytes, "EMPTY").as_deref(),
            Some("")
        );
        assert_eq!(procargs_environment_value(&bytes, "executable_path"), None);
        assert_eq!(procargs_environment_value(&bytes, "HOM"), None);
    }

    /// Parked by `a_child_environment_is_readable`; idle otherwise.
    #[test]
    #[ignore]
    fn environment_reader_child() {
        if std::env::var_os("GHOSTEX_PROCARGS_ENV_TEST").is_some() {
            std::thread::sleep(std::time::Duration::from_secs(30));
        }
    }

    #[test]
    fn a_child_environment_is_readable() {
        // macOS hides the environment of Apple platform binaries such as /bin/sleep, so the child is this test binary.
        let mut child = std::process::Command::new(std::env::current_exe().expect("test binary"))
            .args([
                "--exact",
                "zmx::process_table_macos::tests::environment_reader_child",
            ])
            .args(["--ignored", "--test-threads=1"])
            .env("GHOSTEX_PROCARGS_ENV_TEST", "/some/where/auth/token")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("child");
        // Until the child has exec'd, the kernel still reports the image it forked from.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let value = loop {
            let value =
                process_environment_value(i64::from(child.id()), "GHOSTEX_PROCARGS_ENV_TEST");
            if value.is_some() || std::time::Instant::now() >= deadline {
                break value;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(value.as_deref(), Some("/some/where/auth/token"));
    }

    #[test]
    fn procargs_without_arguments_read_as_unreadable() {
        let bytes = 0_i32.to_ne_bytes().to_vec();
        assert_eq!(parse_procargs(&bytes), None);
    }

    #[test]
    fn this_process_tree_reads_like_ps() {
        let mut child = std::process::Command::new("/bin/sleep")
            .arg("30")
            .spawn()
            .expect("sleep");
        let me = i64::from(std::process::id());
        let rows = process_tree_rows(&[me]);
        let _ = child.kill();
        let _ = child.wait();
        let root = rows
            .iter()
            .find(|row| row.pid_for_test() == me)
            .expect("root row");
        assert!(!root.command_for_test().is_empty());
        let sleeper = rows
            .iter()
            .find(|row| row.pid_for_test() == i64::from(child.id()))
            .expect("child row");
        assert_eq!(sleeper.ppid_for_test(), me);
        assert_eq!(sleeper.command_for_test(), "/bin/sleep 30");
    }

    #[test]
    fn unknown_roots_give_no_rows() {
        assert!(process_tree_rows(&[0, -4, i64::from(i32::MAX)]).is_empty());
    }
}
