use std::{
    collections::{HashMap, HashSet},
    process::{Command, Stdio},
};

use crate::*;

pub(crate) fn gpui_parse_native_resource_processes(
    output: &str,
    process_id: impl Fn(u32) -> u32,
) -> Vec<GpuiNativeResourceProcess> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let system_pid = fields.next()?.parse::<u32>().ok()?;
            let system_ppid = fields.next()?.parse::<u32>().ok()?;
            let cpu = fields.next()?.parse::<f64>().ok()?;
            let rss_kb = fields.next()?.parse::<f64>().ok()?;
            let command = fields.collect::<Vec<_>>().join(" ");
            if command.is_empty() {
                return None;
            }
            Some(GpuiNativeResourceProcess {
                command,
                cpu,
                pid: process_id(system_pid),
                ppid: process_id(system_ppid),
                memory_mb: rss_kb / 1024.0,
                system_pid,
            })
        })
        .collect()
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn gpui_read_native_resource_processes() -> Vec<GpuiNativeResourceProcess> {
    let Ok(output) = Command::new("/bin/ps")
        .args(["-axo", "pid=,ppid=,pcpu=,rss=,command="])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
    let mut processes =
        gpui_parse_native_resource_processes(&String::from_utf8_lossy(&output.stdout), |pid| pid);
    /*
    CDXC:Resources 2026-08-19-12:10:
    `ps rss` is resident-set size, which charges every shared page to each
    process that maps it. The app runs one CEF helper per surface and they all
    map the same ~76 MB Chromium framework, so summing rss over the process
    tree invented tens of GB that no process actually owns, while individual
    helper rows read far below what Activity Monitor shows for the same pid.
    macOS reports process memory as the phys_footprint ledger (what Activity
    Monitor and `top` print), so read that per pid instead. Processes owned by
    another user deny the ledger read and keep their rss sample; those never
    reach a Resources row.
    */
    #[cfg(target_os = "macos")]
    for process in &mut processes {
        if let Some(footprint_mb) = gpui_native_resource_phys_footprint_mb(process.system_pid) {
            process.memory_mb = footprint_mb;
        }
    }
    processes
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_native_resource_phys_footprint_mb(pid: u32) -> Option<f64> {
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
            pid: std::ffi::c_int,
            flavor: std::ffi::c_int,
            buffer: *mut std::ffi::c_void,
        ) -> std::ffi::c_int;
    }

    const RUSAGE_INFO_V0: std::ffi::c_int = 0;
    let mut info = RusageInfoV0::default();
    let status = unsafe {
        proc_pid_rusage(
            pid as std::ffi::c_int,
            RUSAGE_INFO_V0,
            (&raw mut info).cast::<std::ffi::c_void>(),
        )
    };
    (status == 0).then_some(info.ri_phys_footprint as f64 / (1024.0 * 1024.0))
}

/// Marks a Win32 process id so it never collides with a WSL Linux pid in one snapshot.
#[cfg(target_os = "windows")]
pub(crate) const WINDOWS_PROCESS_ID_BIT: u32 = 1 << 31;

#[cfg(target_os = "windows")]
pub(crate) fn gpui_read_native_resource_processes() -> Vec<GpuiNativeResourceProcess> {
    use std::sync::Mutex;
    use sysinfo::{
        MINIMUM_CPU_UPDATE_INTERVAL, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind,
    };

    /*
    Windows has two real process domains: the Win32 Ghostex/CEF tree and the
    selected WSL2 distribution that owns terminals, zmx, agents, and gxserver.
    Sample both instead of executing Unix paths on the Win32 host. The high bit
    keeps Win32 process-tree identities separate from Linux PIDs while rows
    continue to display and act on each process's real system PID.
    */
    static WINDOWS_PROCESS_SYSTEM: std::sync::OnceLock<Mutex<(System, bool)>> =
        std::sync::OnceLock::new();

    let refresh_kind = ProcessRefreshKind::nothing()
        .with_cpu()
        .with_memory()
        .with_cmd(UpdateKind::OnlyIfNotSet)
        .with_exe(UpdateKind::OnlyIfNotSet);
    let mut native_processes = WINDOWS_PROCESS_SYSTEM
        .get_or_init(|| Mutex::new((System::new(), false)))
        .lock()
        .ok()
        .map(|mut state| {
            if !state.1 {
                state
                    .0
                    .refresh_processes_specifics(ProcessesToUpdate::All, true, refresh_kind);
                thread::sleep(MINIMUM_CPU_UPDATE_INTERVAL);
                state.1 = true;
            }
            state
                .0
                .refresh_processes_specifics(ProcessesToUpdate::All, true, refresh_kind);
            state
                .0
                .processes()
                .iter()
                .filter_map(|(pid, process)| {
                    let system_pid = pid.as_u32();
                    if system_pid == 0 || system_pid & WINDOWS_PROCESS_ID_BIT != 0 {
                        return None;
                    }
                    let command = if process.cmd().is_empty() {
                        process
                            .exe()
                            .map(|path| path.to_string_lossy().into_owned())
                            .unwrap_or_else(|| process.name().to_string_lossy().into_owned())
                    } else {
                        process
                            .cmd()
                            .iter()
                            .map(|part| part.to_string_lossy())
                            .collect::<Vec<_>>()
                            .join(" ")
                    };
                    (!command.is_empty()).then(|| GpuiNativeResourceProcess {
                        command,
                        cpu: process.cpu_usage() as f64,
                        pid: system_pid | WINDOWS_PROCESS_ID_BIT,
                        ppid: process
                            .parent()
                            .map(|pid| pid.as_u32() | WINDOWS_PROCESS_ID_BIT)
                            .unwrap_or(WINDOWS_PROCESS_ID_BIT),
                        memory_mb: process.memory() as f64 / (1024.0 * 1024.0),
                        system_pid,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if let Some(output) = windows_terminal_backend::resource_process_snapshot() {
        native_processes.extend(gpui_parse_native_resource_processes(&output, |pid| pid));
    }
    native_processes
}

pub(crate) fn gpui_parse_native_resource_servers(output: &str) -> Vec<GpuiNativeResourceServer> {
    let mut pid = None;
    let mut seen = HashSet::new();
    let mut servers = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (field, value) = line.split_at(1);
        match field {
            "p" => pid = value.parse::<u32>().ok(),
            "n" => {
                let Some(pid) = pid else { continue };
                let endpoint = value.split_whitespace().next().unwrap_or_default();
                let Some(raw_port) = endpoint.rsplit(':').next() else {
                    continue;
                };
                let Ok(port) = raw_port.parse::<u16>() else {
                    continue;
                };
                if !seen.insert((pid, port)) {
                    continue;
                }
                servers.push(GpuiNativeResourceServer {
                    label: format!("localhost:{port}"),
                    pid,
                    port,
                    url: format!("http://localhost:{port}"),
                });
            }
            _ => {}
        }
    }
    servers
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn gpui_read_native_resource_servers() -> Vec<GpuiNativeResourceServer> {
    // Linux distributions install lsof in /usr/bin, macOS in /usr/sbin.
    #[cfg(target_os = "macos")]
    let executable = "/usr/sbin/lsof";
    #[cfg(not(target_os = "macos"))]
    let executable = "lsof";
    let Ok(output) = Command::new(executable)
        .args(["-nP", "-iTCP", "-sTCP:LISTEN", "-F", "pcn"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    gpui_parse_native_resource_servers(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(target_os = "windows")]
pub(crate) fn gpui_read_native_resource_servers() -> Vec<GpuiNativeResourceServer> {
    let mut servers = windows_terminal_backend::resource_server_snapshot()
        .map(|output| gpui_parse_native_resource_servers(&output))
        .unwrap_or_default();
    servers.extend(gpui_read_windows_resource_servers().unwrap_or_default());
    servers
}

/// CDXC:Resources 2026-10-05 WHY:
/// The WSL `lsof` snapshot answers nothing in native PowerShell mode, where agents and their dev servers are Win32 processes, so Windows listeners always come from the system TCP table as well. Their pids carry `WINDOWS_PROCESS_ID_BIT` to match the Win32 rows of `gpui_read_native_resource_processes`.
#[cfg(target_os = "windows")]
pub(crate) fn gpui_read_windows_resource_servers() -> Result<Vec<GpuiNativeResourceServer>, String>
{
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCP6TABLE_OWNER_PID, MIB_TCPROW_OWNER_PID,
        MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_LISTENER,
    };
    const AF_INET: u32 = 2;
    const AF_INET6: u32 = 23;
    const ERROR_INSUFFICIENT_BUFFER: u32 = 122;

    fn listener_table(family: u32) -> Result<Vec<u32>, String> {
        let mut size = 0u32;
        // The table can grow between the size query and the read, so retry a few times.
        for _ in 0..4 {
            let mut buffer = vec![0u32; (size as usize).div_ceil(4)];
            let pointer = if buffer.is_empty() {
                std::ptr::null_mut()
            } else {
                buffer.as_mut_ptr().cast()
            };
            let status = unsafe {
                GetExtendedTcpTable(
                    pointer,
                    &mut size,
                    0,
                    family,
                    TCP_TABLE_OWNER_PID_LISTENER,
                    0,
                )
            };
            match status {
                0 => return Ok(buffer),
                ERROR_INSUFFICIENT_BUFFER => continue,
                _ => break,
            }
        }
        Err("Could not inspect local listening ports.".into())
    }

    fn table_rows<Row: Copy>(table: &[u32], rows_offset: usize) -> Vec<Row> {
        let Some(&count) = table.first() else {
            return Vec::new();
        };
        let available = (table.len() * 4).saturating_sub(rows_offset) / std::mem::size_of::<Row>();
        let base = unsafe { table.as_ptr().cast::<u8>().add(rows_offset) };
        (0..(count as usize).min(available))
            .map(|index| unsafe {
                base.add(index * std::mem::size_of::<Row>())
                    .cast::<Row>()
                    .read_unaligned()
            })
            .collect()
    }

    // dwLocalPort holds the port in network byte order in its low 16 bits.
    fn port(raw: u32) -> u16 {
        (((raw & 0xff) << 8) | ((raw >> 8) & 0xff)) as u16
    }

    let ipv4 = table_rows::<MIB_TCPROW_OWNER_PID>(
        &listener_table(AF_INET)?,
        std::mem::offset_of!(MIB_TCPTABLE_OWNER_PID, table),
    )
    .into_iter()
    .map(|row| (row.dwOwningPid, port(row.dwLocalPort)));
    let ipv6 = table_rows::<MIB_TCP6ROW_OWNER_PID>(
        &listener_table(AF_INET6)?,
        std::mem::offset_of!(MIB_TCP6TABLE_OWNER_PID, table),
    )
    .into_iter()
    .map(|row| (row.dwOwningPid, port(row.dwLocalPort)));
    let mut seen = HashSet::new();
    Ok(ipv4
        .chain(ipv6)
        .filter(|(pid, port)| *pid != 0 && *port != 0 && seen.insert((*pid, *port)))
        .map(|(pid, port)| GpuiNativeResourceServer {
            label: format!("localhost:{port}"),
            pid: pid | WINDOWS_PROCESS_ID_BIT,
            port,
            url: format!("http://localhost:{port}"),
        })
        .collect())
}

/// Program names (`node` for `node.exe`) of Win32 processes keyed by their bit-marked pid. The sampled command line cannot give this reliably, because sysinfo joins arguments with spaces and the program path may contain spaces too.
#[cfg(target_os = "windows")]
pub(crate) fn gpui_read_windows_process_names(pids: &[u32]) -> HashMap<u32, String> {
    gpui_read_windows_process_details(pids, false)
        .into_iter()
        .map(|(pid, (name, _))| (pid, name))
        .collect()
}

#[cfg(target_os = "windows")]
fn gpui_read_windows_process_details(
    pids: &[u32],
    with_cwd: bool,
) -> HashMap<u32, (String, Option<std::path::PathBuf>)> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let system_pids = pids
        .iter()
        .filter(|pid| *pid & WINDOWS_PROCESS_ID_BIT != 0)
        .map(|pid| Pid::from_u32(pid & !WINDOWS_PROCESS_ID_BIT))
        .collect::<Vec<_>>();
    if system_pids.is_empty() {
        return HashMap::new();
    }
    let refresh_kind = if with_cwd {
        ProcessRefreshKind::nothing().with_cwd(UpdateKind::Always)
    } else {
        ProcessRefreshKind::nothing()
    };
    let mut system = System::new();
    system.refresh_processes_specifics(ProcessesToUpdate::Some(&system_pids), true, refresh_kind);
    system_pids
        .iter()
        .filter_map(|pid| {
            let process = system.process(*pid)?;
            let name = std::path::Path::new(process.name())
                .file_stem()?
                .to_string_lossy()
                .into_owned();
            let cwd = process
                .cwd()
                .filter(|cwd| !cwd.as_os_str().is_empty())
                .map(std::path::Path::to_path_buf);
            Some((pid.as_u32() | WINDOWS_PROCESS_ID_BIT, (name, cwd)))
        })
        .collect()
}

pub(crate) fn gpui_parse_native_resource_process_cwds(
    output: &str,
) -> HashMap<u32, std::path::PathBuf> {
    let mut pid = None;
    let mut cwds = HashMap::new();
    for line in output.lines() {
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            continue;
        }
        let (field, value) = line.split_at(1);
        match field {
            "p" => pid = value.parse::<u32>().ok(),
            "n" => {
                let Some(pid) = pid else { continue };
                if value.starts_with('/') {
                    cwds.insert(pid, std::path::PathBuf::from(value));
                }
            }
            _ => {}
        }
    }
    cwds
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn gpui_read_native_resource_process_cwds(
    pids: &[u32],
) -> HashMap<u32, std::path::PathBuf> {
    if pids.is_empty() {
        return HashMap::new();
    }
    let pid_list = pids
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let Ok(output) = Command::new("/usr/sbin/lsof")
        .args(["-nP", "-a", "-d", "cwd", "-p", &pid_list, "-F", "pn"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
    else {
        return HashMap::new();
    };
    /*
    lsof exits non-zero as soon as one of the listed pids has already gone,
    which is routine when sampling a process list a moment after reading it.
    The surviving pids are still printed on stdout, so parse the output on its
    own terms instead of gating on the exit status.
    */
    gpui_parse_native_resource_process_cwds(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(target_os = "windows")]
pub(crate) fn gpui_read_native_resource_process_cwds(
    pids: &[u32],
) -> HashMap<u32, std::path::PathBuf> {
    let wsl_pids = pids
        .iter()
        .copied()
        .filter(|pid| pid & WINDOWS_PROCESS_ID_BIT == 0)
        .collect::<Vec<_>>();
    let mut cwds = windows_terminal_backend::resource_process_cwd_snapshot(&wsl_pids)
        .map(|output| gpui_parse_native_resource_process_cwds(&output))
        .unwrap_or_default();
    cwds.extend(
        gpui_read_windows_process_details(pids, true)
            .into_iter()
            .filter_map(|(pid, (_, cwd))| Some((pid, cwd?))),
    );
    cwds
}

pub(crate) fn gpui_native_resource_children_by_parent(
    processes: &[GpuiNativeResourceProcess],
) -> HashMap<u32, Vec<GpuiNativeResourceProcess>> {
    let mut children = HashMap::<u32, Vec<GpuiNativeResourceProcess>>::new();
    for process in processes {
        children
            .entry(process.ppid)
            .or_default()
            .push(process.clone());
    }
    children
}
