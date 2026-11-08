//! The microphone and speaker on macOS: `native/macos/GpuiVoiceAudio.m` (AVAudioEngine; a
//! conversation turns on the voice-processing I/O unit, whose echo canceller keeps the agent's voice
//! from the speakers out of the microphone, and AVAudioUnitTimePitch plays it faster).

use std::sync::Mutex;

use super::audio::{AudioStartError, CaptureFn, DeviceChangedFn};
use super::audio_backend::VoiceAudioBackend;

/// What the audio thread calls back into. Lives at a fixed address for as long as the engine runs.
struct CaptureContext {
    capture: Mutex<Option<CaptureFn>>,
    device_changed: Mutex<Option<DeviceChangedFn>>,
}

/// A running AVAudioEngine. Dropping it stops the engine.
pub(crate) struct MacVoiceAudio {
    handle: usize,
    context: *const CaptureContext,
}

// The native calls synchronize on the engine object; the context is only read through its mutexes.
unsafe impl Send for MacVoiceAudio {}
unsafe impl Sync for MacVoiceAudio {}

mod ffi {
    use std::ffi::{c_char, c_void};

    pub(super) type Capture = extern "C" fn(*mut c_void, *const i16, usize);
    pub(super) type Event = extern "C" fn(*mut c_void, i32);

    unsafe extern "C" {
        pub(super) fn GhostexVoiceAudioStart(
            voice_processing: i32,
            playback: i32,
            input_name: *const c_char,
            capture: Capture,
            event: Event,
            context: *mut c_void,
            out: *mut *mut c_void,
            error_buffer: *mut c_char,
            error_length: usize,
        ) -> i32;
        pub(super) fn GhostexVoiceAudioEnqueue(
            handle: *mut c_void,
            samples: *const i16,
            count: usize,
        );
        pub(super) fn GhostexVoiceAudioFlush(handle: *mut c_void) -> i64;
        pub(super) fn GhostexVoiceAudioSetRate(handle: *mut c_void, rate: f32);
        pub(super) fn GhostexVoiceAudioStop(handle: *mut c_void);
        pub(super) fn GhostexVoiceInputDevices(buffer: *mut c_char, length: usize) -> usize;
    }
}

extern "C" fn on_capture(context: *mut std::ffi::c_void, samples: *const i16, count: usize) {
    if context.is_null() || samples.is_null() || count == 0 {
        return;
    }
    let context = unsafe { &*(context as *const CaptureContext) };
    let samples = unsafe { std::slice::from_raw_parts(samples, count) };
    let Ok(capture) = context.capture.lock() else {
        return;
    };
    if let Some(capture) = capture.as_ref() {
        capture(samples);
    }
}

extern "C" fn on_event(context: *mut std::ffi::c_void, _event: i32) {
    if context.is_null() {
        return;
    }
    let context = unsafe { &*(context as *const CaptureContext) };
    if let Ok(device_changed) = context.device_changed.lock()
        && let Some(device_changed) = device_changed.as_ref()
    {
        device_changed();
    }
}

impl MacVoiceAudio {
    /// Opens the microphone (with echo cancellation when `voice_processing`) and, when `playback`,
    /// the speaker. Blocks while macOS asks for microphone access the first time.
    pub(crate) fn start(
        voice_processing: bool,
        playback: bool,
        microphone: Option<String>,
        capture: CaptureFn,
        device_changed: DeviceChangedFn,
    ) -> Result<Self, AudioStartError> {
        let context = Box::into_raw(Box::new(CaptureContext {
            capture: Mutex::new(Some(capture)),
            device_changed: Mutex::new(Some(device_changed)),
        }));
        match Self::start_native(voice_processing, playback, microphone, context) {
            Ok(handle) => Ok(Self { handle, context }),
            Err(error) => {
                drop(unsafe { Box::from_raw(context) });
                Err(error)
            }
        }
    }

    fn start_native(
        voice_processing: bool,
        playback: bool,
        microphone: Option<String>,
        context: *mut CaptureContext,
    ) -> Result<usize, AudioStartError> {
        // A name with a NUL in it can't name a device: the default one is used.
        let microphone = microphone
            .filter(|name| !name.is_empty())
            .and_then(|name| std::ffi::CString::new(name).ok());
        let mut handle = std::ptr::null_mut();
        let mut error = [0 as std::ffi::c_char; 512];
        let status = unsafe {
            ffi::GhostexVoiceAudioStart(
                voice_processing as i32,
                playback as i32,
                microphone
                    .as_ref()
                    .map_or(std::ptr::null(), |name| name.as_ptr()),
                on_capture,
                on_event,
                context as *mut std::ffi::c_void,
                &mut handle,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        match status {
            0 if !handle.is_null() => Ok(handle as usize),
            1 => Err(AudioStartError::PermissionDenied),
            2 => Err(AudioStartError::NoInput),
            _ => {
                let message = unsafe { std::ffi::CStr::from_ptr(error.as_ptr()) }
                    .to_string_lossy()
                    .trim()
                    .to_string();
                Err(AudioStartError::Failed(if message.is_empty() {
                    "The microphone could not start.".to_string()
                } else {
                    message
                }))
            }
        }
    }
}

impl VoiceAudioBackend for MacVoiceAudio {
    fn enqueue(&self, samples: &[i16]) {
        unsafe {
            ffi::GhostexVoiceAudioEnqueue(
                self.handle as *mut std::ffi::c_void,
                samples.as_ptr(),
                samples.len(),
            )
        };
    }

    fn set_rate(&self, rate: f32) {
        unsafe { ffi::GhostexVoiceAudioSetRate(self.handle as *mut std::ffi::c_void, rate) };
    }

    fn flush(&self) -> u64 {
        unsafe { ffi::GhostexVoiceAudioFlush(self.handle as *mut std::ffi::c_void) }.max(0) as u64
    }
}

impl Drop for MacVoiceAudio {
    fn drop(&mut self) {
        unsafe { ffi::GhostexVoiceAudioStop(self.handle as *mut std::ffi::c_void) };
        // The engine is stopped, but an audio thread may still be finishing a callback: forget the
        // callbacks now and free their context a moment later.
        let context = unsafe { &*self.context };
        if let Ok(mut capture) = context.capture.lock() {
            *capture = None;
        }
        if let Ok(mut device_changed) = context.device_changed.lock() {
            *device_changed = None;
        }
        let address = self.context as usize;
        let _ = std::thread::Builder::new()
            .name("ghostex-voice-audio-release".into())
            .spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(2));
                drop(unsafe { Box::from_raw(address as *mut CaptureContext) });
            });
    }
}

/// The microphones' names, for Settings › Dictation's picker, and the default one's (CoreAudio's input
/// devices, `GhostexVoiceInputDevices`).
pub(crate) fn input_names() -> (Vec<String>, Option<String>) {
    let mut buffer = vec![0 as std::ffi::c_char; 16 * 1024];
    let written = unsafe { ffi::GhostexVoiceInputDevices(buffer.as_mut_ptr(), buffer.len()) };
    if written == 0 {
        return (Vec::new(), None);
    }
    let text = unsafe { std::ffi::CStr::from_ptr(buffer.as_ptr()) }.to_string_lossy();
    parse_input_devices(&text)
}

/// `GhostexVoiceInputDevices`' lines: `D\t<name>` for the default input, `-\t<name>` for the others.
fn parse_input_devices(text: &str) -> (Vec<String>, Option<String>) {
    let mut names = Vec::new();
    let mut default = None;
    for line in text.lines() {
        let Some((mark, name)) = line.split_once('\t') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        if mark == "D" {
            default = Some(name.to_string());
        }
        names.push(name.to_string());
    }
    (names, default)
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_device_list_marks_the_default_input() {
        let (names, default) = super::parse_input_devices(
            "-\tStudio Display Microphone\nD\tMacBook Pro Microphone\nbroken line\n-\t\n",
        );
        assert_eq!(
            names,
            ["Studio Display Microphone", "MacBook Pro Microphone"]
        );
        assert_eq!(default.as_deref(), Some("MacBook Pro Microphone"));
    }
}
