//! Crashes the panic hook never sees: a bad memory access or an abort in native code (AppKit,
//! CEF, Ghostty). On macOS the terminal library Ghostex embeds runs Sentry's Breakpad handler,
//! which catches these, writes a `.ghosttycrash` file (a Sentry envelope holding the event and a
//! minidump) to `$XDG_STATE_HOME/ghostty/crash`, and ends the process without a macOS crash
//! report. At the next open this reads the files written since it last looked, keeps the ones
//! from this app, and turns each into a [`CrashRecord`]: the exception, and the crashing thread's
//! frames (walked by frame pointer) as `module+offset`. Builds carry no symbol table
//! (CDXC:Build 2026-10-11 DECISION in .cargo/config.toml), so Ghostex's own frames are named by
//! rebuilding the report's commit with `--debug-info` (stripping does not move code) and looking
//! the offsets up there.
//!
//! CDXC:CrashReports 2026-11-17 WHY:
//! Ghostty's handler takes the crash before any signal handler of ours could (Mach exception
//! ports come first on macOS, and its SIGABRT handler exits), so reading its file is the one way
//! to see these crashes; a second handler would only fight it. It writes the file at the next
//! launch (its event keeps the crash's own time), and a panic also ends in an abort it writes a
//! file for, so a file within [`SAME_CRASH_WINDOW_MS`] after a saved, sent or dismissed panic or
//! exception is that crash, not a new one.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::record::{CrashKind, CrashRecord, STACK_MAX_FRAMES};

/// A crash file this soon after a recorded panic or exception is the abort that followed it.
pub(crate) const SAME_CRASH_WINDOW_MS: u64 = 15_000;
/// Files larger than this are not read (a minidump is usually 1 to 3 MB).
const MAX_CRASH_FILE_BYTES: u64 = 64 * 1024 * 1024;

fn ghostty_crash_dir() -> Option<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
        })?;
    Some(state.join("ghostty").join("crash"))
}

/// Crash files written after `after_ms` (their modification time), oldest first, with that time.
pub(crate) fn crash_files_after(after_ms: u64) -> Vec<(PathBuf, u64)> {
    let Some(dir) = ghostty_crash_dir() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<(PathBuf, u64)> = entries
        .flatten()
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "ghosttycrash")
        })
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            if metadata.len() > MAX_CRASH_FILE_BYTES {
                return None;
            }
            let modified = metadata
                .modified()
                .ok()?
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_millis() as u64;
            (modified > after_ms).then_some((entry.path(), modified))
        })
        .collect();
    files.sort_by_key(|(_, modified)| *modified);
    files
}

/// The crash record for `path`, when it is a crash of this app (Ghostty itself writes to the same
/// folder).
pub(crate) fn read_crash_file(path: &Path, written_ms: u64) -> Option<CrashRecord> {
    let bytes = std::fs::read(path).ok()?;
    let envelope = Envelope::parse(&bytes)?;
    let images = Images::from_event(&envelope.event);
    let exe = std::env::current_exe().ok()?;
    let exe_name = exe.file_name()?.to_string_lossy().to_string();
    // The first image is the main executable.
    let main = images.list.first()?;
    if main.name != exe_name {
        return None;
    }
    let crash = envelope
        .minidump
        .as_deref()
        .and_then(Minidump::crashing_thread);
    let frames: Vec<String> = crash
        .as_ref()
        .map(|crash| {
            crash
                .frames
                .iter()
                .take(STACK_MAX_FRAMES)
                .map(|address| images.describe(*address))
                .collect()
        })
        .unwrap_or_default();
    // Where: the first frame in Ghostex's own code, else the first frame.
    let location = crash
        .as_ref()
        .and_then(|crash| {
            crash
                .frames
                .iter()
                .find(|address| {
                    images
                        .image_of(**address)
                        .is_some_and(|image| image.name == exe_name)
                })
                .or_else(|| crash.frames.first())
                .map(|address| images.describe(*address))
        })
        .unwrap_or_else(|| "unknown".to_string());
    let message = crash
        .as_ref()
        .map(|crash| exception_text(crash.code, crash.flags, crash.address))
        .unwrap_or_else(|| "Stopped by the system".to_string());
    let at_ms = envelope
        .event
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(|time| chrono::DateTime::parse_from_rfc3339(time).ok())
        .map(|time| time.timestamp_millis() as u64)
        .unwrap_or(written_ms);
    let same_binary = std::fs::metadata(&exe)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .is_some_and(|modified| (modified.as_millis() as u64) <= at_ms);
    Some(CrashRecord {
        kind: CrashKind::Native,
        // The file says which build of the terminal library crashed, not which Ghostex: this
        // build is the one that crashed only when its binary was in place before the crash.
        version: if same_binary {
            super::system::APP_VERSION.to_string()
        } else {
            "unknown".to_string()
        },
        commit: same_binary
            .then(super::system::build_commit)
            .flatten()
            .map(str::to_string),
        at_ms,
        uptime_ms: None,
        location,
        message,
        stack: frames,
        log_tail: super::record::log_tail_before(at_ms),
        thread: crash.as_ref().map(|crash| {
            let ghostty_name = tag(&envelope.event, "thread-type");
            match (crash.is_main_thread, ghostty_name) {
                (true, _) => "main".to_string(),
                (false, Some(name)) => format!("another thread (terminal {name})"),
                (false, None) => "another thread".to_string(),
            }
        }),
        screen: super::context::screen_text(
            tag(&envelope.event, super::context::VIEW_TAG).unwrap_or_default(),
            tag(&envelope.event, super::context::FOCUS_TAG).unwrap_or_default(),
        ),
        recent_events: recent_events(&envelope.event),
        build_id: main.debug_id.clone().filter(|_| same_binary),
    })
}

/// A tag of the crash event, kept only when it is a short plain label.
fn tag<'a>(event: &'a Value, name: &str) -> Option<&'a str> {
    event
        .pointer("/tags")
        .and_then(|tags| tags.get(name))
        .and_then(Value::as_str)
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 60
                && value
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || " .,_:-()".contains(c))
        })
}

/// The breadcrumbs context.rs left in the crash handler's scope, oldest first, as
/// `13:33:02 gpui.cefRuntime.ready`.
fn recent_events(event: &Value) -> Vec<String> {
    // Sentry writes them as a list, or as `{ "values": [...] }`.
    let list = event.get("breadcrumbs").and_then(|crumbs| {
        crumbs
            .as_array()
            .or_else(|| crumbs.get("values")?.as_array())
    });
    let Some(list) = list else {
        return Vec::new();
    };
    let events: Vec<String> = list
        .iter()
        .filter_map(|crumb| {
            let message = crumb.get("message")?.as_str()?;
            let clean: String = message
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-'))
                .take(80)
                .collect();
            if clean.is_empty() {
                return None;
            }
            let clock = crumb
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(|time| chrono::DateTime::parse_from_rfc3339(time).ok())
                .map(|time| time.format("%H:%M:%S ").to_string())
                .unwrap_or_default();
            Some(format!("{clock}{clean}"))
        })
        .collect();
    let skip = events
        .len()
        .saturating_sub(super::context::RECENT_EVENTS_MAX);
    events[skip..].to_vec()
}

/// `EXC_BAD_ACCESS reading 0x10` and the like, from the minidump's Mac exception codes.
fn exception_text(code: u32, flags: u32, address: u64) -> String {
    match code {
        1 => format!("Bad memory access (EXC_BAD_ACCESS, code {flags:#x}) at {address:#x}"),
        2 => "Bad instruction (EXC_BAD_INSTRUCTION)".to_string(),
        3 => "Arithmetic error (EXC_ARITHMETIC)".to_string(),
        // Breakpad reports SIGABRT as EXC_SOFTWARE with MD_EXCEPTION_CODE_MAC_ABORT.
        5 if flags == 0x0001_0002 => "Aborted (SIGABRT)".to_string(),
        5 => format!("Software exception (EXC_SOFTWARE, code {flags:#x})"),
        6 => "Breakpoint or trap (EXC_BREAKPOINT)".to_string(),
        10 => "Crashed (EXC_CRASH)".to_string(),
        11 => "Resource limit (EXC_RESOURCE)".to_string(),
        12 => "Guard violation (EXC_GUARD)".to_string(),
        other => format!("Exception {other:#x}, code {flags:#x}"),
    }
}

/// The two items of a `.ghosttycrash` Sentry envelope this reads.
struct Envelope {
    event: Value,
    minidump: Option<Vec<u8>>,
}

impl Envelope {
    /// A header line, then items: a JSON header line with `length`, then that many bytes.
    fn parse(bytes: &[u8]) -> Option<Self> {
        let mut position = bytes.iter().position(|byte| *byte == b'\n')? + 1;
        let mut event = None;
        let mut minidump = None;
        while position < bytes.len() {
            let end = position + bytes[position..].iter().position(|byte| *byte == b'\n')?;
            let header: Value = serde_json::from_slice(&bytes[position..end]).ok()?;
            position = end + 1;
            let length = header.get("length").and_then(Value::as_u64)? as usize;
            let payload = bytes.get(position..position.checked_add(length)?)?;
            position += length;
            match (
                header.get("type").and_then(Value::as_str),
                header.get("attachment_type").and_then(Value::as_str),
            ) {
                (Some("event"), _) => event = serde_json::from_slice(payload).ok(),
                (Some("attachment"), Some("event.minidump")) => minidump = Some(payload.to_vec()),
                _ => {}
            }
            if bytes.get(position) == Some(&b'\n') {
                position += 1;
            }
        }
        Some(Self {
            event: event?,
            minidump,
        })
    }
}

struct Image {
    base: u64,
    size: u64,
    name: String,
    /// The Mach-O UUID (`debug_id`, dashes removed), which names the exact binary.
    debug_id: Option<String>,
}

struct Images {
    list: Vec<Image>,
}

impl Images {
    fn from_event(event: &Value) -> Self {
        let list = event
            .pointer("/debug_meta/images")
            .and_then(Value::as_array)
            .map(|images| {
                images
                    .iter()
                    .filter_map(|image| {
                        let base = u64::from_str_radix(
                            image.get("image_addr")?.as_str()?.trim_start_matches("0x"),
                            16,
                        )
                        .ok()?;
                        let size = image.get("image_size")?.as_u64()?;
                        // Only the file name: the folder can name the user.
                        let name = image
                            .get("code_file")?
                            .as_str()?
                            .rsplit(['/', '\\'])
                            .next()?
                            .to_string();
                        let debug_id = image
                            .get("debug_id")
                            .and_then(Value::as_str)
                            .filter(|id| {
                                id.len() <= 40
                                    && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
                            })
                            // Without dashes: plain hex is what the report's scrubber keeps
                            // (a dashed id reads as a key to it).
                            .map(|id| id.replace('-', ""));
                        Some(Image {
                            base,
                            size,
                            name,
                            debug_id,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self { list }
    }

    fn image_of(&self, address: u64) -> Option<&Image> {
        self.list
            .iter()
            .find(|image| address >= image.base && address - image.base < image.size)
    }

    fn describe(&self, address: u64) -> String {
        match self.image_of(address) {
            Some(image) => format!("{}+{:#x}", image.name, address - image.base),
            None => format!("{address:#x}"),
        }
    }
}

/// The crashing thread of a minidump: its exception and its frames.
struct CrashingThread {
    code: u32,
    flags: u32,
    address: u64,
    frames: Vec<u64>,
    /// The first thread of the process (the one AppKit and GPUI run on) crashed.
    is_main_thread: bool,
}

struct Minidump;

impl Minidump {
    const EXCEPTION_STREAM: u32 = 6;
    const THREAD_LIST_STREAM: u32 = 3;
    /// `MD_CONTEXT_ARM64`; the x86-64 layout is read too (`MD_CONTEXT_AMD64`).
    const CONTEXT_ARM64: u32 = 0x8000_0000;
    const CONTEXT_AMD64: u32 = 0x0010_0000;

    fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
        Some(u32::from_le_bytes(
            bytes.get(offset..offset + 4)?.try_into().ok()?,
        ))
    }

    fn u64_at(bytes: &[u8], offset: usize) -> Option<u64> {
        Some(u64::from_le_bytes(
            bytes.get(offset..offset + 8)?.try_into().ok()?,
        ))
    }

    /// The (size, offset) of the first stream of `kind`.
    fn stream(bytes: &[u8], kind: u32) -> Option<(usize, usize)> {
        if bytes.get(0..4)? != b"MDMP" {
            return None;
        }
        let count = Self::u32_at(bytes, 8)? as usize;
        let directory = Self::u32_at(bytes, 12)? as usize;
        (0..count.min(1024)).find_map(|index| {
            let entry = directory + index * 12;
            (Self::u32_at(bytes, entry)? == kind).then(|| {
                Some((
                    Self::u32_at(bytes, entry + 4)? as usize,
                    Self::u32_at(bytes, entry + 8)? as usize,
                ))
            })?
        })
    }

    fn crashing_thread(bytes: &[u8]) -> Option<CrashingThread> {
        // MDRawExceptionStream: thread id, padding, the exception record (code, flags, record,
        // address, ...: 152 bytes), then the thread's context location.
        let (_, exception) = Self::stream(bytes, Self::EXCEPTION_STREAM)?;
        let thread_id = Self::u32_at(bytes, exception)?;
        let code = Self::u32_at(bytes, exception + 8)?;
        let flags = Self::u32_at(bytes, exception + 12)?;
        let address = Self::u64_at(bytes, exception + 24)?;
        let context_size = Self::u32_at(bytes, exception + 160)? as usize;
        let context_at = Self::u32_at(bytes, exception + 164)? as usize;
        let context = bytes.get(context_at..context_at.checked_add(context_size)?)?;
        let (pc, lr, fp, address_mask) = Self::registers(context)?;
        let stack = Self::thread_stack(bytes, thread_id);
        let is_main_thread = Self::first_thread_id(bytes) == Some(thread_id);
        let mut frames = vec![pc];
        if let Some(lr) = lr {
            frames.push(lr);
        }
        if let Some((start, memory)) = stack {
            let read = |address: u64| {
                let offset = usize::try_from(address.checked_sub(start)?).ok()?;
                Self::u64_at(memory, offset)
            };
            let mut frame = fp;
            for _ in 0..STACK_MAX_FRAMES * 2 {
                let (Some(next), Some(ret)) = (read(frame), read(frame.wrapping_add(8))) else {
                    break;
                };
                let ret = ret & address_mask;
                if ret == 0 {
                    break;
                }
                frames.push(ret);
                if next <= frame {
                    break;
                }
                frame = next;
            }
        }
        Some(CrashingThread {
            code,
            flags,
            address,
            frames,
            is_main_thread,
        })
    }

    /// The id of the thread list's first thread: the process's main thread (Breakpad lists the
    /// task's threads in the order the kernel gives them, main first).
    fn first_thread_id(bytes: &[u8]) -> Option<u32> {
        let (size, list) = Self::stream(bytes, Self::THREAD_LIST_STREAM)?;
        let count = Self::u32_at(bytes, list)? as usize;
        if count == 0 {
            return None;
        }
        let first = (list + size).checked_sub(count.checked_mul(48)?)?;
        Self::u32_at(bytes, first)
    }

    /// The program counter, the link register (arm64), the frame pointer, and the mask that
    /// clears pointer authentication bits from saved return addresses (arm64 only).
    fn registers(context: &[u8]) -> Option<(u64, Option<u64>, u64, u64)> {
        if Self::u32_at(context, 0)? & Self::CONTEXT_ARM64 != 0 {
            // context_flags, cpsr, then x0..x28, fp (29), lr (30), sp (31), pc (32).
            let register = |index: usize| Self::u64_at(context, 8 + index * 8);
            return Some((
                register(32)?,
                Some(register(30)?),
                register(29)?,
                0x0000_000f_ffff_ffff,
            ));
        }
        // MDRawContextAMD64: six home slots, then context_flags at 0x30; rbp at 0xa0, rip at 0xf8.
        if Self::u32_at(context, 0x30)? & Self::CONTEXT_AMD64 != 0 {
            return Some((
                Self::u64_at(context, 0xf8)?,
                None,
                Self::u64_at(context, 0xa0)?,
                u64::MAX,
            ));
        }
        None
    }

    /// The start address and bytes of `thread_id`'s saved stack.
    fn thread_stack(bytes: &[u8], thread_id: u32) -> Option<(u64, &[u8])> {
        // MDRawThreadList: a count, then 48-byte MDRawThread entries that end the stream (the
        // writer may pad after the count, so they are found from the end).
        let (size, list) = Self::stream(bytes, Self::THREAD_LIST_STREAM)?;
        let count = Self::u32_at(bytes, list)? as usize;
        let first = (list + size).checked_sub(count.checked_mul(48)?)?;
        (0..count).find_map(|index| {
            let entry = first + index * 48;
            if Self::u32_at(bytes, entry)? != thread_id {
                return None;
            }
            let start = Self::u64_at(bytes, entry + 24)?;
            let length = Self::u32_at(bytes, entry + 32)? as usize;
            let at = Self::u32_at(bytes, entry + 36)? as usize;
            Some((start, bytes.get(at..at.checked_add(length)?)?))
        })
    }
}
