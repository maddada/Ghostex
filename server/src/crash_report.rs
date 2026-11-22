//! What a dying gxserver leaves behind, and the per-session steps a crash keeps interrupting.
//!
//! CDXC:Diagnostics 2026-10-24 WHY:
//! A native Windows gxserver that died of an access violation (GitHub #289) left nothing: no `gxserver.jsonl` line, no stderr for `gxserver --foreground`, and no Windows Error Reporting event, because the daemon sets SEM_NOGPFAULTERRORBOX for its helpers (`suppress_helper_error_dialogs`). The daemon now records its own fatal crashes: an unhandled-exception filter writes the exception code, the faulting module and offset and the session steps that were running to `gxserver-crash.json` in the logs folder and to stderr, and the next start logs that record as an error. A panic is logged where it happens.
//!
//! CDXC:ServerDaemon 2026-10-24 WHY:
//! The same session that crashed gxserver was restored on every start and crashed it again, so the daemon never stayed up. Short native steps that run for one session are wrapped in `step`; a step that was running when gxserver crashed twice on this build is skipped for that session from then on and logged as quarantined at every start, so the session still loses only that step and the fault stays visible. A new build starts with a clean slate.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, OnceLock,
    },
};

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::logging::{GxserverLogInput, GxserverLogger, LogLevel};
use crate::paths::GxserverPaths;

const CRASH_FILE_NAME: &str = "gxserver-crash.json";
const STRIKES_FILE_NAME: &str = "crash-strikes.json";
/// Crashes on one build during the same step for the same session before that step is skipped.
pub(crate) const QUARANTINE_STRIKES: u32 = 2;
/// Longest subject a breadcrumb keeps; session keys are far shorter.
const SUBJECT_BYTES: usize = 64;

/// Read only by the Windows crash filter.
#[cfg_attr(not(windows), allow(dead_code))]
struct ActiveStep {
    id: u64,
    subject: String,
    step: &'static str,
    thread: u32,
}

static ACTIVE_STEPS: Mutex<Vec<ActiveStep>> = Mutex::new(Vec::new());
static NEXT_STEP_ID: AtomicU64 = AtomicU64::new(1);
/// `subject|step` keys skipped on this build, read once at startup.
static QUARANTINED: OnceLock<Vec<String>> = OnceLock::new();
static PANIC_LOGGER: OnceLock<GxserverLogger> = OnceLock::new();

/// A running per-session step; dropping it ends the step.
pub(crate) struct StepGuard {
    id: u64,
}

impl Drop for StepGuard {
    fn drop(&mut self) {
        if let Ok(mut steps) = ACTIVE_STEPS.lock() {
            steps.retain(|step| step.id != self.id);
        }
    }
}

/// Marks `step` as running for `subject` (a session key such as `P1abc/G2def`) until the guard is dropped, or `None` when that step crashed gxserver twice on this build and must be skipped.
pub(crate) fn step(subject: &str, step: &'static str) -> Option<StepGuard> {
    let subject = sanitize_subject(subject);
    if is_quarantined(&subject, step) {
        return None;
    }
    let id = NEXT_STEP_ID.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut steps) = ACTIVE_STEPS.lock() {
        steps.push(ActiveStep {
            id,
            subject,
            step,
            thread: current_thread_id(),
        });
    }
    Some(StepGuard { id })
}

fn is_quarantined(subject: &str, step: &str) -> bool {
    QUARANTINED
        .get()
        .is_some_and(|keys| keys.iter().any(|key| *key == strike_key(subject, step)))
}

fn strike_key(subject: &str, step: &str) -> String {
    format!("{subject}|{step}")
}

/// Breadcrumbs are written by the crash filter without escaping, so a subject keeps only characters that need none.
fn sanitize_subject(subject: &str) -> String {
    subject
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '/'))
        .take(SUBJECT_BYTES)
        .collect()
}

#[cfg(windows)]
fn current_thread_id() -> u32 {
    unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() }
}

#[cfg(not(windows))]
fn current_thread_id() -> u32 {
    0
}

/// The record the crash filter writes; read back by the next start.
#[derive(Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CrashRecord {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub exception_code: String,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub module: String,
    #[serde(default)]
    pub module_offset: String,
    #[serde(default)]
    pub access: String,
    #[serde(default)]
    pub thread_id: u32,
    #[serde(default)]
    pub steps: Vec<CrashStep>,
}

#[derive(Debug, Default, Deserialize, Serialize, PartialEq)]
pub(crate) struct CrashStep {
    pub subject: String,
    pub step: String,
    #[serde(default)]
    pub thread: u32,
}

impl CrashRecord {
    /// The steps the crash interrupted: the ones on the faulting thread, or every running step when none ran there (a fault on a thread no step names).
    fn blamed_steps(&self) -> Vec<&CrashStep> {
        let on_thread: Vec<_> = self
            .steps
            .iter()
            .filter(|step| self.thread_id != 0 && step.thread == self.thread_id)
            .collect();
        if on_thread.is_empty() {
            self.steps.iter().collect()
        } else {
            on_thread
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Strikes {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub strikes: BTreeMap<String, u32>,
}

impl Strikes {
    /// Counts one crash against every blamed step; strikes from another build are dropped first.
    pub(crate) fn record(&mut self, version: &str, crash: &CrashRecord) {
        if self.version != version {
            self.version = version.to_string();
            self.strikes.clear();
        }
        for step in crash.blamed_steps() {
            *self
                .strikes
                .entry(strike_key(&sanitize_subject(&step.subject), &step.step))
                .or_default() += 1;
        }
    }

    /// The `subject|step` keys to skip on `version`.
    pub(crate) fn quarantined(&self, version: &str) -> Vec<String> {
        if self.version != version {
            return Vec::new();
        }
        self.strikes
            .iter()
            .filter(|(_, count)| **count >= QUARANTINE_STRIKES)
            .map(|(key, _)| key.clone())
            .collect()
    }
}

fn crash_file(paths: &GxserverPaths) -> PathBuf {
    paths.logs_dir.join(CRASH_FILE_NAME)
}

fn strikes_file(paths: &GxserverPaths) -> PathBuf {
    paths.root_dir.join(STRIKES_FILE_NAME)
}

/// Installs the panic hook and, on Windows, the fatal-crash filter. Call once, early in a daemon start, after `report_previous_crash` has read the last record.
pub(crate) fn install(paths: &GxserverPaths) {
    let hook_paths = paths.clone();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|text| text.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_default();
        let location = info
            .location()
            .map(|location| format!("{}:{}", location.file(), location.line()))
            .unwrap_or_default();
        let logger = PANIC_LOGGER.get_or_init(|| GxserverLogger::new(hook_paths.clone()));
        let _ = logger.log(GxserverLogInput {
            level: LogLevel::Error,
            event: "gxserverPanic".to_string(),
            server_id: None,
            request_id: None,
            client: None,
            duration_ms: None,
            error: Some(message),
            details: Some(json!({
                "location": location,
                "thread": std::thread::current().name().unwrap_or("unnamed"),
            })),
        });
        previous(info);
    }));
    #[cfg(windows)]
    native::install(&crash_file(paths));
}

/// Logs the crash the previous run recorded (if any), counts it against the steps it interrupted, and loads the steps this build must skip. Call before `install`, which starts a fresh crash file.
pub(crate) fn report_previous_crash(paths: &GxserverPaths, logger: &GxserverLogger, version: &str) {
    let crash_path = crash_file(paths);
    let strikes_path = strikes_file(paths);
    let mut strikes = read_json::<Strikes>(&strikes_path).unwrap_or_default();
    if let Some(crash) =
        read_json::<CrashRecord>(&crash_path).filter(|crash| !crash.exception_code.is_empty())
    {
        let _ = logger.log(GxserverLogInput {
            level: LogLevel::Error,
            event: "gxserverCrashed".to_string(),
            server_id: None,
            request_id: None,
            client: None,
            duration_ms: None,
            error: Some(format!(
                "gxserver {} stopped with exception {} in {}+{}.",
                crash.version, crash.exception_code, crash.module, crash.module_offset
            )),
            details: Some(json!({
                "access": crash.access,
                "address": crash.address,
                "exceptionCode": crash.exception_code,
                "module": crash.module,
                "moduleOffset": crash.module_offset,
                "steps": crash.steps.iter().map(|step| json!({
                    "onFaultingThread": crash.thread_id != 0 && step.thread == crash.thread_id,
                    "step": step.step,
                    "subject": step.subject,
                })).collect::<Vec<_>>(),
                "version": crash.version,
            })),
        });
        // A crash of an older build never counts toward this one's quarantine.
        let crashed_version = if crash.version.is_empty() {
            version
        } else {
            crash.version.as_str()
        };
        strikes.record(crashed_version, &crash);
        if let Some(parent) = strikes_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(text) = serde_json::to_string_pretty(&strikes) {
            let _ = fs::write(&strikes_path, text);
        }
    }
    let _ = fs::remove_file(&crash_path);
    let quarantined = strikes.quarantined(version);
    if !quarantined.is_empty() {
        let _ = logger.log(GxserverLogInput {
            level: LogLevel::Warn,
            event: "sessionStepsQuarantined".to_string(),
            server_id: None,
            request_id: None,
            client: None,
            duration_ms: None,
            error: Some(format!(
                "These session steps crashed gxserver {QUARANTINE_STRIKES} or more times on this build and are skipped until Ghostex is updated."
            )),
            details: Some(json!({ "quarantined": quarantined, "version": version })),
        });
    }
    let _ = QUARANTINED.set(quarantined);
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

/// The crash filter runs on the faulting thread with the heap possibly corrupt, so everything it needs is opened up front and it formats into a fixed buffer.
#[cfg(windows)]
mod native {
    use std::{
        fmt::Write as _, fs::File, io::Write as _, mem::ManuallyDrop,
        os::windows::io::FromRawHandle, path::Path, sync::OnceLock,
    };

    use windows_sys::Win32::{
        Foundation::{HMODULE, INVALID_HANDLE_VALUE},
        System::{
            Console::{GetStdHandle, STD_ERROR_HANDLE},
            Diagnostics::Debug::{SetUnhandledExceptionFilter, EXCEPTION_POINTERS},
            LibraryLoader::{
                GetModuleFileNameW, GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
                GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            },
        },
    };

    const EXCEPTION_CONTINUE_SEARCH: i32 = 0;
    const STATUS_ACCESS_VIOLATION: u32 = 0xC000_0005;
    const STATUS_IN_PAGE_ERROR: u32 = 0xC000_0006;

    static CRASH_FILE: OnceLock<File> = OnceLock::new();

    pub(super) fn install(crash_file: &Path) {
        if let Some(parent) = crash_file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(file) = File::create(crash_file) {
            let _ = CRASH_FILE.set(file);
        }
        unsafe {
            SetUnhandledExceptionFilter(Some(filter));
        }
    }

    struct Buffer {
        bytes: [u8; 4096],
        len: usize,
    }

    impl std::fmt::Write for Buffer {
        fn write_str(&mut self, text: &str) -> std::fmt::Result {
            let room = self.bytes.len() - self.len;
            let take = text.len().min(room);
            self.bytes[self.len..self.len + take].copy_from_slice(&text.as_bytes()[..take]);
            self.len += take;
            Ok(())
        }
    }

    /// The faulting module's file name and its base address.
    fn module_of(address: usize, name: &mut Buffer) -> usize {
        let mut module: HMODULE = std::ptr::null_mut();
        let found = unsafe {
            GetModuleHandleExW(
                GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS
                    | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                address as *const u16,
                &mut module,
            )
        };
        if found == 0 || module.is_null() {
            let _ = name.write_str("unknown");
            return 0;
        }
        let mut path = [0u16; 520];
        let length =
            unsafe { GetModuleFileNameW(module, path.as_mut_ptr(), path.len() as u32) } as usize;
        let path = &path[..length.min(path.len())];
        let file = path
            .iter()
            .rposition(|unit| *unit == u16::from(b'\\'))
            .map_or(path, |slash| &path[slash + 1..]);
        for ch in char::decode_utf16(file.iter().copied()) {
            let ch = ch.unwrap_or('?');
            if ch == '"' || ch == '\\' || ch.is_control() {
                continue;
            }
            let _ = name.write_char(ch);
        }
        module as usize
    }

    unsafe extern "system" fn filter(pointers: *const EXCEPTION_POINTERS) -> i32 {
        let record = if pointers.is_null() {
            std::ptr::null()
        } else {
            (*pointers).ExceptionRecord
        };
        if record.is_null() {
            return EXCEPTION_CONTINUE_SEARCH;
        }
        let code = (*record).ExceptionCode as u32;
        let address = (*record).ExceptionAddress as usize;
        let mut module = Buffer {
            bytes: [0; 4096],
            len: 0,
        };
        let base = module_of(address, &mut module);
        let module = std::str::from_utf8(&module.bytes[..module.len]).unwrap_or("unknown");
        let mut access = Buffer {
            bytes: [0; 4096],
            len: 0,
        };
        if matches!(code, STATUS_ACCESS_VIOLATION | STATUS_IN_PAGE_ERROR)
            && (*record).NumberParameters >= 2
        {
            let kind = match (*record).ExceptionInformation[0] {
                0 => "read",
                1 => "write",
                8 => "execute",
                _ => "access",
            };
            let _ = write!(access, "{kind} of {:#x}", (*record).ExceptionInformation[1]);
        }
        let access = std::str::from_utf8(&access.bytes[..access.len]).unwrap_or("");
        let thread = super::current_thread_id();

        let mut line = Buffer {
            bytes: [0; 4096],
            len: 0,
        };
        let _ = write!(
            line,
            "{{\"version\":\"{}\",\"exceptionCode\":\"{code:#010X}\",\"address\":\"{address:#x}\",\"module\":\"{module}\",\"moduleOffset\":\"{:#x}\",\"access\":\"{access}\",\"threadId\":{thread},\"steps\":[",
            crate::constants::GXSERVER_VERSION,
            address.saturating_sub(base),
        );
        // try_lock: a thread that holds the list must never stall the crash record.
        if let Ok(steps) = super::ACTIVE_STEPS.try_lock() {
            for (index, step) in steps.iter().enumerate() {
                let _ = write!(
                    line,
                    "{}{{\"subject\":\"{}\",\"step\":\"{}\",\"thread\":{}}}",
                    if index == 0 { "" } else { "," },
                    step.subject,
                    step.step,
                    step.thread
                );
            }
        }
        let _ = line.write_str("]}\n");
        if let Some(mut file) = CRASH_FILE.get() {
            let _ = file.write_all(&line.bytes[..line.len]);
            let _ = file.flush();
        }
        let stderr = GetStdHandle(STD_ERROR_HANDLE);
        if !stderr.is_null() && stderr != INVALID_HANDLE_VALUE {
            let mut stderr = ManuallyDrop::new(File::from_raw_handle(stderr));
            let mut text = Buffer {
                bytes: [0; 4096],
                len: 0,
            };
            let _ = write!(
                text,
                "gxserver crashed: exception {code:#010X} in {module}+{:#x}{}{access}\n",
                address.saturating_sub(base),
                if access.is_empty() { "" } else { ", " },
            );
            let _ = stderr.write_all(&text.bytes[..text.len]);
        }
        EXCEPTION_CONTINUE_SEARCH
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crash(thread_id: u32, steps: &[(&str, &str, u32)]) -> CrashRecord {
        CrashRecord {
            exception_code: "0xC0000005".to_string(),
            thread_id,
            steps: steps
                .iter()
                .map(|(subject, step, thread)| CrashStep {
                    subject: subject.to_string(),
                    step: step.to_string(),
                    thread: *thread,
                })
                .collect(),
            ..CrashRecord::default()
        }
    }

    #[test]
    fn a_step_is_quarantined_after_two_crashes_on_the_same_build() {
        let mut strikes = Strikes::default();
        let record = crash(
            7,
            &[
                ("P1/G1", "codexFleetRead", 7),
                ("P1/G2", "codexFleetRead", 9),
            ],
        );
        strikes.record("10.33.0", &record);
        assert!(strikes.quarantined("10.33.0").is_empty());
        strikes.record("10.33.0", &record);
        assert_eq!(strikes.quarantined("10.33.0"), vec!["P1/G1|codexFleetRead"]);
    }

    #[test]
    fn a_new_build_starts_without_strikes() {
        let mut strikes = Strikes::default();
        let record = crash(7, &[("P1/G1", "codexFleetRead", 7)]);
        strikes.record("10.33.0", &record);
        strikes.record("10.33.0", &record);
        assert!(strikes.quarantined("10.34.0").is_empty());
        strikes.record("10.34.0", &record);
        assert!(strikes.quarantined("10.34.0").is_empty());
    }

    #[test]
    fn a_crash_on_an_unnamed_thread_counts_against_every_running_step() {
        let mut strikes = Strikes::default();
        let record = crash(3, &[("P1/G1", "a", 7), ("P1/G2", "b", 9)]);
        strikes.record("v", &record);
        strikes.record("v", &record);
        assert_eq!(strikes.quarantined("v"), vec!["P1/G1|a", "P1/G2|b"]);
    }

    #[test]
    fn the_crash_filter_record_parses() {
        let text = r#"{"version":"10.33.0","exceptionCode":"0xC0000005","address":"0x7ffb1234","module":"ntdll.dll","moduleOffset":"0x1234","access":"read of 0x0","threadId":12,"steps":[{"subject":"P1/G1","step":"codexFleetRead","thread":12}]}"#;
        let record: CrashRecord = serde_json::from_str(text).expect("parse");
        assert_eq!(record.module, "ntdll.dll");
        assert_eq!(record.blamed_steps().len(), 1);
    }

    #[test]
    fn subjects_keep_only_characters_the_filter_writes_unescaped() {
        assert_eq!(sanitize_subject("P1/G\"1\\x"), "P1/G1x");
    }
}
