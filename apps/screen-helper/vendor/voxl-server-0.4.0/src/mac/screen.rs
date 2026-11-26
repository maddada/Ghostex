//! Dynamic resolution on macOS: switches the captured display to the mode that best
//! fits the viewer's window. Displays only offer fixed modes, so the desktop takes
//! the largest one that fits rather than the exact size. The change is made for this
//! process only: macOS restores the user's mode when voxl exits, even on a crash.

use super::ffi::*;
use parking_lot::Mutex;
use std::time::{Duration, Instant};

/// Smallest desktop a client may ask for.
pub const MIN: (u32, u32) = (640, 480);

/// Streamed sizes of the selectable modes, for `fit`.
static SIZES: Mutex<Vec<(u32, u32)>> = Mutex::new(Vec::new());

/// The size the desktop would take for a request of `w`x`h` pixels: the largest
/// mode that fits, or the smallest mode if none does.
pub fn fit(w: u32, h: u32, max: (u32, u32)) -> (u32, u32) {
    let sizes = SIZES.lock();
    let area = |s: &&(u32, u32)| s.0 as u64 * s.1 as u64;
    sizes
        .iter()
        .filter(|s| s.0 <= w && s.1 <= h)
        .max_by_key(area)
        .or_else(|| sizes.iter().min_by_key(area))
        .copied()
        .unwrap_or(max)
}

struct Mode {
    mode: CGDisplayModeRef,
    points: (usize, usize),
    size: (u32, u32),
}

pub struct Screen {
    display: u32,
    original: Mode,
    resized: bool,
    modes: Vec<Mode>,
    max: (u32, u32),
    current: (u32, u32),
}

unsafe impl Send for Screen {}

impl Screen {
    pub fn open(_display: Option<&str>) -> Result<Self, String> {
        let display = super::display();
        let mut modes: Vec<Mode> = Vec::new();
        let original;
        unsafe {
            let cur = CGDisplayCopyDisplayMode(display);
            if cur.is_null() {
                return Err("no display mode".into());
            }
            let hidpi = CGDisplayModeGetPixelWidth(cur) > CGDisplayModeGetWidth(cur);
            let current = (CGDisplayModeGetWidth(cur), CGDisplayModeGetHeight(cur));
            let rate = CGDisplayModeGetRefreshRate(cur);
            original = Mode {
                mode: cur,
                points: current,
                size: super::size_for(current.0 as f64, current.1 as f64),
            };
            let key = kCGDisplayShowDuplicateLowResolutionModes;
            let opts = objc2_core_foundation::CFDictionary::from_slices(
                &[&*(key as *const objc2_core_foundation::CFString)],
                &[objc2_core_foundation::CFBoolean::new(true)],
            );
            let all = CGDisplayCopyAllDisplayModes(display, &*opts as *const _ as CFDictionaryRef);
            if all.is_null() {
                return Err("no display modes".into());
            }
            for i in 0..CFArrayGetCount(all) {
                let m = CFArrayGetValueAtIndex(all, i) as CGDisplayModeRef;
                let (w, h) = (CGDisplayModeGetWidth(m), CGDisplayModeGetHeight(m));
                // Same kind of mode as now (Retina or not, same refresh), usable as a desktop.
                if !CGDisplayModeIsUsableForDesktopGUI(m)
                    || (CGDisplayModeGetPixelWidth(m) > w) != hidpi
                    || (CGDisplayModeGetRefreshRate(m) - rate).abs() > 0.5
                    || modes.iter().any(|x| x.points == (w, h))
                {
                    continue;
                }
                // Modes past the pixel budget would cost more to encode than the
                // current one (which always stays available).
                let size = super::size_for(w as f64, h as f64);
                let area = size.0 as f64 * size.1 as f64;
                if size.0 < MIN.0
                    || size.1 < MIN.1
                    || (area > super::max_pixels() * 1.02 && (w, h) != current)
                {
                    continue;
                }
                CFRetain(m as CFTypeRef);
                modes.push(Mode {
                    mode: m,
                    points: (w, h),
                    size,
                });
            }
            CFRelease(all);
        }
        if modes.len() < 2 {
            return Err("the display offers no other modes".into());
        }
        modes.sort_by_key(|m| m.size.0 as u64 * m.size.1 as u64);
        *SIZES.lock() = modes.iter().map(|m| m.size).collect();
        let max = modes.last().unwrap().size;
        log::info!(
            "display modes: {}",
            modes
                .iter()
                .map(|m| format!("{}x{}", m.points.0, m.points.1))
                .collect::<Vec<_>>()
                .join(", ")
        );
        Ok(Screen {
            display,
            original,
            resized: false,
            modes,
            max,
            current: super::capture_size(display),
        })
    }

    pub fn max(&self) -> (u32, u32) {
        self.max
    }

    pub fn fit(&self, w: u32, h: u32) -> (u32, u32) {
        fit(w, h, self.max)
    }

    pub fn resize(&mut self, w: u32, h: u32) -> Result<(), String> {
        let want = self.fit(w, h);
        self.current = super::capture_size(self.display);
        if want == self.current {
            return Ok(());
        }
        let m = self
            .modes
            .iter()
            .find(|m| m.size == want)
            .ok_or("no such mode")?;
        set_mode(self.display, m)?;
        self.resized = true;
        self.current = want;
        log::info!(
            "display set to {}x{} points ({}x{} streamed)",
            m.points.0,
            m.points.1,
            want.0,
            want.1
        );
        Ok(())
    }

    pub fn restore(&mut self) -> Result<(), String> {
        if self.resized {
            set_mode(self.display, &self.original)?;
            self.current = self.original.size;
            self.resized = false;
        }
        Ok(())
    }
}

fn set_mode(display: u32, m: &Mode) -> Result<(), String> {
    unsafe {
        let mut cfg: CGDisplayConfigRef = std::ptr::null_mut();
        if CGBeginDisplayConfiguration(&mut cfg) != 0 {
            return Err("CGBeginDisplayConfiguration failed".into());
        }
        let e = CGConfigureDisplayWithDisplayMode(cfg, display, m.mode, std::ptr::null());
        if e != 0 {
            CGCancelDisplayConfiguration(cfg);
            return Err(format!("CGConfigureDisplayWithDisplayMode: {e}"));
        }
        let e = CGCompleteDisplayConfiguration(cfg, kCGConfigureForAppOnly);
        if e != 0 {
            return Err(format!("CGCompleteDisplayConfiguration: {e}"));
        }
    }
    // Capture restarts right after; let the new geometry settle first.
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(2) {
        let b = unsafe { CGDisplayBounds(display) };
        if (b.size.width as usize, b.size.height as usize) == m.points {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

impl Drop for Mode {
    fn drop(&mut self) {
        unsafe { CGDisplayModeRelease(self.mode) };
    }
}
