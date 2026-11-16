//! This user's processes on macOS, read from the kernel in-process (the way
//! server/src/zmx/process_table_macos.rs reads a session's tree, without running `ps`).

use super::processes::ProcessRow;

/// The tag zmx gives every process of a session.
const SESSION_TAG: &str = "ZMX_SESSION";

pub(super) fn user_processes() -> Vec<ProcessRow> {
    let uid = unsafe { libc::getuid() };
    let mut arguments = ProcArgs::default();
    all_pids()
        .into_iter()
        .filter_map(|pid| {
            let info = short_info(pid)?;
            if info.pbsi_uid != uid {
                return None;
            }
            let start = start_time(pid as u32)?;
            let raw = arguments.read(pid);
            let command = raw.and_then(command_line).unwrap_or_default();
            // Only a process launchd adopted can have left its session's tree; the others are
            // found by walking down from the session's daemon.
            let session_tag = (info.pbsi_ppid == 1)
                .then(|| raw.and_then(|bytes| environment_value(bytes, SESSION_TAG)))
                .flatten();
            Some(ProcessRow {
                pid: pid as u32,
                ppid: info.pbsi_ppid,
                start,
                command,
                session_tag,
            })
        })
        .collect()
}

/// phys_footprint, the figure Activity Monitor shows as Memory (compressed pages included).
pub(super) fn footprint_bytes(pid: u32) -> Option<u64> {
    #[repr(C)]
    #[derive(Default)]
    struct RusageInfoV0 {
        ri_uuid: [u8; 16],
        ri_user_time: u64,
        ri_system_time: u64,
        ri_pkg_idle_wkups: u64,
        ri_interrupt_wkups: u64,
        ri_pageins: u64,
        ri_wired_size: u64,
        ri_resident_size: u64,
        ri_phys_footprint: u64,
        ri_proc_start_abstime: u64,
        ri_proc_exit_abstime: u64,
    }
    unsafe extern "C" {
        fn proc_pid_rusage(
            pid: libc::c_int,
            flavor: libc::c_int,
            buffer: *mut libc::c_void,
        ) -> libc::c_int;
    }
    let mut info = RusageInfoV0::default();
    let status = unsafe {
        proc_pid_rusage(
            pid as libc::c_int,
            0,
            (&raw mut info).cast::<libc::c_void>(),
        )
    };
    (status == 0).then_some(info.ri_phys_footprint)
}

/// When `pid` started, in microseconds since the epoch.
pub(super) fn start_time(pid: u32) -> Option<u64> {
    let info: libc::proc_bsdinfo = pid_info(i32::try_from(pid).ok()?, libc::PROC_PIDTBSDINFO)?;
    Some(info.pbi_start_tvsec * 1_000_000 + info.pbi_start_tvusec)
}

fn all_pids() -> Vec<i32> {
    let mut pids = vec![0 as libc::pid_t; 4096];
    loop {
        let size = (pids.len() * std::mem::size_of::<libc::pid_t>()) as libc::c_int;
        let count = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), size) };
        if count <= 0 {
            return Vec::new();
        }
        if count as usize >= pids.len() && pids.len() < 1 << 20 {
            pids.resize(pids.len() * 4, 0);
            continue;
        }
        pids.truncate(count as usize);
        pids.retain(|pid| *pid > 1);
        return pids;
    }
}

fn pid_info<T>(pid: i32, flavor: libc::c_int) -> Option<T> {
    let mut info: T = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<T>() as i32;
    let read = unsafe { libc::proc_pidinfo(pid, flavor, 0, (&mut info as *mut T).cast(), size) };
    (read == size).then_some(info)
}

fn short_info(pid: i32) -> Option<libc::proc_bsdshortinfo> {
    pid_info(pid, libc::PROC_PIDT_SHORTBSDINFO)
}

/// One `KERN_PROCARGS2` buffer reused for the whole scan.
#[derive(Default)]
struct ProcArgs {
    buffer: Vec<u8>,
    size: usize,
}

impl ProcArgs {
    fn read(&mut self, pid: i32) -> Option<&[u8]> {
        if self.buffer.is_empty() {
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
        self.size = self.buffer.len();
        let result = unsafe {
            libc::sysctl(
                mib.as_mut_ptr(),
                mib.len() as u32,
                self.buffer.as_mut_ptr().cast(),
                &mut self.size,
                std::ptr::null_mut(),
                0,
            )
        };
        (result == 0).then(|| &self.buffer[..self.size.min(self.buffer.len())])
    }
}

/// The argv strings of a `KERN_PROCARGS2` reply (argc, the executable path, padding, argv, env)
/// and where the environment starts.
fn argv_and_env_start(bytes: &[u8]) -> Option<(Vec<String>, usize)> {
    let argc = i32::from_ne_bytes(bytes.get(..4)?.try_into().ok()?);
    let mut offset = 4;
    offset += bytes.get(offset..)?.iter().position(|byte| *byte == 0)? + 1;
    while bytes.get(offset) == Some(&0) {
        offset += 1;
    }
    let mut argv = Vec::new();
    for _ in 0..argc.max(0) {
        let rest = bytes.get(offset..)?;
        let length = rest
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(rest.len());
        argv.push(String::from_utf8_lossy(&rest[..length]).into_owned());
        offset += length + 1;
    }
    Some((argv, offset))
}

fn command_line(bytes: &[u8]) -> Option<String> {
    let (argv, _) = argv_and_env_start(bytes)?;
    (!argv.is_empty()).then(|| argv.join(" ").replace(['\n', '\r', '\t'], " "))
}

fn environment_value(bytes: &[u8], name: &str) -> Option<String> {
    let (_, offset) = argv_and_env_start(bytes)?;
    let prefix = format!("{name}=");
    // The environment ends at the first empty string; the kernel's apple strings follow.
    bytes
        .get(offset..)?
        .split(|byte| *byte == 0)
        .take_while(|entry| !entry.is_empty())
        .find_map(|entry| entry.strip_prefix(prefix.as_bytes()))
        .map(|value| String::from_utf8_lossy(value).into_owned())
}
