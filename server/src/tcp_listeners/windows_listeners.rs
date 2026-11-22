//! Listening TCP sockets on native Windows, read from the system TCP table.
use std::collections::HashMap;
use std::mem::{offset_of, size_of, zeroed};
use std::net::{Ipv4Addr, Ipv6Addr};

use anyhow::{bail, Result};
use windows_sys::Win32::{
    Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
    NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCP6TABLE_OWNER_PID, MIB_TCPROW_OWNER_PID,
        MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_LISTENER,
    },
    System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    },
};

use super::listener_discovery::TcpListenerDetail;

const AF_INET: u32 = 2;
const AF_INET6: u32 = 23;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;

/// CDXC:Resources 2026-10-05 WHY:
/// gxserver runs its scripts through PowerShell on native Windows, which cannot run the POSIX lsof/ss script, and Windows has neither tool. The TCP table gives the same rows `ghostex ports` prints elsewhere: wildcard binds read `0.0.0.0` and `::` as `ss` prints them, and the command is the process name without `.exe`, as `Get-Process` shows it.
pub(crate) fn read_windows_tcp_listeners() -> Result<Vec<TcpListenerDetail>> {
    let names = process_names();
    let command = |pid: u32| names.get(&pid).cloned();
    let pid = |pid: u32| (pid != 0).then_some(pid);
    let mut listeners = table_rows::<MIB_TCPROW_OWNER_PID>(
        &listener_table(AF_INET)?,
        offset_of!(MIB_TCPTABLE_OWNER_PID, table),
    )
    .into_iter()
    .map(|row| TcpListenerDetail {
        address: Ipv4Addr::from(row.dwLocalAddr.to_ne_bytes()).to_string(),
        command: command(row.dwOwningPid),
        pid: pid(row.dwOwningPid),
        port: network_port(row.dwLocalPort),
    })
    .collect::<Vec<_>>();
    listeners.extend(
        table_rows::<MIB_TCP6ROW_OWNER_PID>(
            &listener_table(AF_INET6)?,
            offset_of!(MIB_TCP6TABLE_OWNER_PID, table),
        )
        .into_iter()
        .map(|row| TcpListenerDetail {
            address: Ipv6Addr::from(row.ucLocalAddr).to_string(),
            command: command(row.dwOwningPid),
            pid: pid(row.dwOwningPid),
            port: network_port(row.dwLocalPort),
        }),
    );
    listeners.retain(|listener| listener.port != 0);
    Ok(listeners)
}

/// The table, as `u32` words so its rows are aligned. It can grow between the size query and the
/// read, so the read is retried a few times.
fn listener_table(family: u32) -> Result<Vec<u32>> {
    let mut size = 0u32;
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
            status => bail!("Listing listening TCP ports failed with Windows error {status}."),
        }
    }
    bail!("The listening TCP port list kept changing while it was being read.")
}

fn table_rows<Row: Copy>(table: &[u32], rows_offset: usize) -> Vec<Row> {
    let Some(&count) = table.first() else {
        return Vec::new();
    };
    let available = (table.len() * 4).saturating_sub(rows_offset) / size_of::<Row>();
    let base = unsafe { table.as_ptr().cast::<u8>().add(rows_offset) };
    (0..(count as usize).min(available))
        .map(|index| unsafe {
            base.add(index * size_of::<Row>())
                .cast::<Row>()
                .read_unaligned()
        })
        .collect()
}

/// `dwLocalPort` holds the port in network byte order in its low 16 bits.
fn network_port(raw: u32) -> u16 {
    (((raw & 0xff) << 8) | ((raw >> 8) & 0xff)) as u16
}

fn process_names() -> HashMap<u32, String> {
    let mut names = HashMap::new();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return names;
        }
        let mut entry: PROCESSENTRY32W = zeroed();
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut found = Process32FirstW(snapshot, &mut entry) != 0;
        while found {
            let length = entry
                .szExeFile
                .iter()
                .position(|unit| *unit == 0)
                .unwrap_or(entry.szExeFile.len());
            let exe = String::from_utf16_lossy(&entry.szExeFile[..length]);
            let name = exe
                .len()
                .checked_sub(4)
                .filter(|stem| {
                    exe.get(*stem..)
                        .is_some_and(|suffix| suffix.eq_ignore_ascii_case(".exe"))
                })
                .map_or(exe.as_str(), |stem| &exe[..stem]);
            if !name.is_empty() {
                names.insert(entry.th32ProcessID, name.to_string());
            }
            found = Process32NextW(snapshot, &mut entry) != 0;
        }
        CloseHandle(snapshot);
    }
    names
}
