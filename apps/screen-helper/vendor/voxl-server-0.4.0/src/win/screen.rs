//! Dynamic resolution on Windows: switches the captured display to the mode that best
//! fits the viewer's window. Displays only offer fixed modes (a VM's virtual display
//! offers many), so the desktop takes the largest one that fits rather than the exact
//! size. Changes are temporary (CDS_FULLSCREEN): Windows restores the user's mode
//! when voxl exits, even on a crash.

use parking_lot::Mutex;
use std::time::{Duration, Instant};
use windows::core::PCWSTR;
use windows::Win32::Graphics::Gdi::{
    ChangeDisplaySettingsExW, EnumDisplaySettingsW, CDS_FULLSCREEN, DEVMODEW,
    DISP_CHANGE_SUCCESSFUL, DM_BITSPERPEL, DM_DISPLAYFREQUENCY, DM_PELSHEIGHT, DM_PELSWIDTH,
    ENUM_CURRENT_SETTINGS, ENUM_DISPLAY_SETTINGS_MODE,
};

/// Smallest desktop a client may ask for.
pub const MIN: (u32, u32) = (640, 480);

/// Most pixels a mode may have to be offered (x264's cost grows with area), unless it
/// is the current one.
const MAX_PIXELS: u64 = 2560 * 1600;

/// Sizes of the selectable modes, for `fit`.
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

pub struct Screen {
    device: Vec<u16>,
    modes: Vec<DEVMODEW>,
    max: (u32, u32),
    current: (u32, u32),
}

unsafe impl Send for Screen {}

fn mode(device: &[u16], i: ENUM_DISPLAY_SETTINGS_MODE) -> Option<DEVMODEW> {
    let mut m = DEVMODEW {
        dmSize: std::mem::size_of::<DEVMODEW>() as u16,
        ..Default::default()
    };
    unsafe { EnumDisplaySettingsW(PCWSTR(device.as_ptr()), i, &mut m) }
        .as_bool()
        .then_some(m)
}

fn size(m: &DEVMODEW) -> (u32, u32) {
    (m.dmPelsWidth, m.dmPelsHeight)
}

/// Current size of the display.
fn current(device: &[u16]) -> Option<(u32, u32)> {
    mode(device, ENUM_CURRENT_SETTINGS).map(|m| size(&m))
}

impl Screen {
    pub fn open(_display: Option<&str>) -> Result<Self, String> {
        let out = super::output().ok_or("no display")?;
        let device = out.device_name();
        let cur = mode(&device, ENUM_CURRENT_SETTINGS).ok_or("no current display mode")?;
        let mut modes: Vec<DEVMODEW> = Vec::new();
        let mut i = 0;
        while let Some(m) = mode(&device, ENUM_DISPLAY_SETTINGS_MODE(i)) {
            i += 1;
            let s = size(&m);
            // Same colour depth and refresh as now, one entry per size.
            if m.dmBitsPerPel != cur.dmBitsPerPel
                || m.dmDisplayFrequency != cur.dmDisplayFrequency
                || s.0 < MIN.0
                || s.1 < MIN.1
                || s.0 % 2 != 0
                || s.1 % 2 != 0
                || (s.0 as u64 * s.1 as u64 > MAX_PIXELS && s != size(&cur))
                || modes.iter().any(|x| size(x) == s)
            {
                continue;
            }
            modes.push(m);
        }
        if modes.len() < 2 {
            return Err("the display offers no other modes".into());
        }
        modes.sort_by_key(|m| m.dmPelsWidth as u64 * m.dmPelsHeight as u64);
        *SIZES.lock() = modes.iter().map(size).collect();
        let max = size(modes.last().unwrap());
        log::info!(
            "display modes: {}",
            modes
                .iter()
                .map(|m| format!("{}x{}", m.dmPelsWidth, m.dmPelsHeight))
                .collect::<Vec<_>>()
                .join(", ")
        );
        Ok(Screen {
            current: size(&cur),
            device,
            modes,
            max,
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
        if let Some(c) = current(&self.device) {
            self.current = c;
        }
        if want == self.current {
            return Ok(());
        }
        let mut m = *self
            .modes
            .iter()
            .find(|m| size(m) == want)
            .ok_or("no such mode")?;
        m.dmFields = DM_PELSWIDTH | DM_PELSHEIGHT | DM_BITSPERPEL | DM_DISPLAYFREQUENCY;
        let r = unsafe {
            ChangeDisplaySettingsExW(
                PCWSTR(self.device.as_ptr()),
                Some(&m),
                None,
                CDS_FULLSCREEN,
                None,
            )
        };
        if r != DISP_CHANGE_SUCCESSFUL {
            return Err(format!("ChangeDisplaySettingsEx: {}", r.0));
        }
        // Capture restarts right after; let the new size settle first.
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(2) && current(&self.device) != Some(want) {
            std::thread::sleep(Duration::from_millis(20));
        }
        self.current = want;
        log::info!("display set to {}x{}", want.0, want.1);
        Ok(())
    }
}
