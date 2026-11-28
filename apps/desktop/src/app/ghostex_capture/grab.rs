//! Reading pixels off the screen: a screen's area, or the front window of the app the user was in.
//!
//! macOS goes through ScreenCaptureKit (the native shim), Windows through GDI and X11 through
//! `GetImage` on the root window. Images come back full resolution, so a Retina or 150% screen
//! yields more pixels than its size in points.

use gpui::{Bounds, Pixels};
use image::RgbaImage;

use super::platform::FrontmostApp;

/// A picture of an app's front window, and where that window was.
pub(crate) struct GrabbedWindow {
    pub(crate) image: RgbaImage,
    pub(crate) frame: Bounds<Pixels>,
}

/// Whether Ghostex may read other apps' pixels; `request` asks the OS (macOS shows its prompt).
pub(crate) fn screen_access(request: bool) -> bool {
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn GhostexGpuiCaptureScreenAccess(request: bool) -> bool;
        }
        unsafe { GhostexGpuiCaptureScreenAccess(request) }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = request;
        true
    }
}

/// Everything on screen inside `frame` (GPUI global space). `scale` is the screen's scale factor.
pub(crate) fn grab_area(frame: Bounds<Pixels>, scale: f32) -> Option<RgbaImage> {
    #[cfg(target_os = "macos")]
    {
        let _ = scale;
        mac::grab_area(frame)
    }
    #[cfg(target_os = "windows")]
    {
        windows::grab_area(frame, scale)
    }
    #[cfg(target_os = "linux")]
    {
        x11::grab_area(frame, scale)
    }
}

/// The front window of `app`.
pub(crate) fn grab_app(app: FrontmostApp, scale: f32) -> Option<GrabbedWindow> {
    if app.id == 0 {
        return None;
    }
    #[cfg(target_os = "macos")]
    {
        let _ = scale;
        mac::grab_app(app)
    }
    #[cfg(target_os = "windows")]
    {
        windows::grab_app(app, scale)
    }
    #[cfg(target_os = "linux")]
    {
        x11::grab_app(app, scale)
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::CString;
    use std::path::PathBuf;

    use gpui::{Bounds, point, px, size};
    use image::RgbaImage;

    use super::{FrontmostApp, GrabbedWindow};

    unsafe extern "C" {
        fn GhostexGpuiCaptureRectPng(
            x: f64,
            y: f64,
            width: f64,
            height: f64,
            path: *const std::ffi::c_char,
        ) -> bool;
        fn GhostexGpuiCaptureAppWindowPng(
            pid: i32,
            path: *const std::ffi::c_char,
            app_name: *mut std::ffi::c_char,
            app_name_capacity: usize,
            frame: *mut f64,
        ) -> bool;
    }

    /// The shim writes a PNG; it is read back and removed right away.
    fn scratch_path() -> PathBuf {
        let directory = std::env::temp_dir().join("ghostex-capture");
        let _ = std::fs::create_dir_all(&directory);
        directory.join(format!("grab-{}.png", uuid::Uuid::new_v4()))
    }

    fn take(path: &PathBuf) -> Option<RgbaImage> {
        let image = image::open(path).ok().map(|image| image.into_rgba8());
        let _ = std::fs::remove_file(path);
        image
    }

    pub(super) fn grab_area(frame: Bounds<gpui::Pixels>) -> Option<RgbaImage> {
        let path = scratch_path();
        let target = CString::new(path.to_string_lossy().as_bytes()).ok()?;
        let written = unsafe {
            GhostexGpuiCaptureRectPng(
                f32::from(frame.origin.x) as f64,
                f32::from(frame.origin.y) as f64,
                f32::from(frame.size.width) as f64,
                f32::from(frame.size.height) as f64,
                target.as_ptr(),
            )
        };
        written.then(|| take(&path)).flatten()
    }

    pub(super) fn grab_app(app: FrontmostApp) -> Option<GrabbedWindow> {
        let path = scratch_path();
        let target = CString::new(path.to_string_lossy().as_bytes()).ok()?;
        let mut name = [0 as std::ffi::c_char; 256];
        let mut frame = [0f64; 4];
        let written = unsafe {
            GhostexGpuiCaptureAppWindowPng(
                app.id as i32,
                target.as_ptr(),
                name.as_mut_ptr(),
                name.len(),
                frame.as_mut_ptr(),
            )
        };
        if !written {
            return None;
        }
        let image = take(&path)?;
        Some(GrabbedWindow {
            image,
            frame: Bounds::new(
                point(px(frame[0] as f32), px(frame[1] as f32)),
                size(px(frame[2] as f32), px(frame[3] as f32)),
            ),
        })
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use gpui::{Bounds, Pixels, point, px, size};
    use image::RgbaImage;
    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleBitmap,
        CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC,
        SRCCOPY, SelectObject,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::IsWindow;

    use super::{FrontmostApp, GrabbedWindow};

    /// Copies a rectangle of the desktop, in device pixels.
    fn grab_device(x: i32, y: i32, width: i32, height: i32) -> Option<RgbaImage> {
        if width <= 0 || height <= 0 {
            return None;
        }
        unsafe {
            let screen = GetDC(std::ptr::null_mut());
            let memory = CreateCompatibleDC(screen);
            let bitmap = CreateCompatibleBitmap(screen, width, height);
            let previous = SelectObject(memory, bitmap);
            let copied = BitBlt(
                memory,
                0,
                0,
                width,
                height,
                screen,
                x,
                y,
                SRCCOPY | CAPTUREBLT,
            );
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..std::mem::zeroed()
            };
            let mut pixels = vec![0u8; width as usize * height as usize * 4];
            let lines = GetDIBits(
                memory,
                bitmap,
                0,
                height as u32,
                pixels.as_mut_ptr().cast(),
                &mut info,
                DIB_RGB_COLORS,
            );
            SelectObject(memory, previous);
            DeleteObject(bitmap);
            DeleteDC(memory);
            ReleaseDC(std::ptr::null_mut(), screen);
            if copied == 0 || lines == 0 {
                return None;
            }
            for pixel in pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
                pixel[3] = 255;
            }
            RgbaImage::from_raw(width as u32, height as u32, pixels)
        }
    }

    pub(super) fn grab_area(frame: Bounds<Pixels>, scale: f32) -> Option<RgbaImage> {
        let device = |value: Pixels| (f32::from(value) * scale).round() as i32;
        grab_device(
            device(frame.origin.x),
            device(frame.origin.y),
            device(frame.size.width),
            device(frame.size.height),
        )
    }

    pub(super) fn grab_app(app: FrontmostApp, scale: f32) -> Option<GrabbedWindow> {
        let hwnd = app.id as HWND;
        if unsafe { IsWindow(hwnd) } == 0 {
            return None;
        }
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let read = unsafe {
            DwmGetWindowAttribute(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS as u32,
                (&mut rect as *mut RECT).cast(),
                std::mem::size_of::<RECT>() as u32,
            )
        };
        if read != 0 {
            return None;
        }
        let image = grab_device(
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
        )?;
        Some(GrabbedWindow {
            image,
            frame: Bounds::new(
                point(px(rect.left as f32 / scale), px(rect.top as f32 / scale)),
                size(
                    px((rect.right - rect.left) as f32 / scale),
                    px((rect.bottom - rect.top) as f32 / scale),
                ),
            ),
        })
    }
}

#[cfg(target_os = "linux")]
mod x11 {
    use gpui::{Bounds, Pixels, point, px, size};
    use image::RgbaImage;
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{ConnectionExt, ImageFormat};

    use super::{FrontmostApp, GrabbedWindow};

    fn grab_device(x: i16, y: i16, width: u16, height: u16) -> Option<RgbaImage> {
        let (connection, screen) = x11rb::connect(None).ok()?;
        let root = connection.setup().roots.get(screen)?.root;
        let reply = connection
            .get_image(ImageFormat::Z_PIXMAP, root, x, y, width, height, !0)
            .ok()?
            .reply()
            .ok()?;
        let expected = width as usize * height as usize * 4;
        if reply.data.len() < expected {
            return None;
        }
        let mut pixels = reply.data[..expected].to_vec();
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            pixel[3] = 255;
        }
        RgbaImage::from_raw(width as u32, height as u32, pixels)
    }

    pub(super) fn grab_area(frame: Bounds<Pixels>, scale: f32) -> Option<RgbaImage> {
        let device = |value: Pixels| (f32::from(value) * scale).round();
        grab_device(
            device(frame.origin.x) as i16,
            device(frame.origin.y) as i16,
            device(frame.size.width) as u16,
            device(frame.size.height) as u16,
        )
    }

    pub(super) fn grab_app(app: FrontmostApp, scale: f32) -> Option<GrabbedWindow> {
        let (connection, screen) = x11rb::connect(None).ok()?;
        let root = connection.setup().roots.get(screen)?.root;
        let window = app.id as u32;
        let geometry = connection.get_geometry(window).ok()?.reply().ok()?;
        let origin = connection
            .translate_coordinates(window, root, 0, 0)
            .ok()?
            .reply()
            .ok()?;
        let image = grab_device(origin.dst_x, origin.dst_y, geometry.width, geometry.height)?;
        Some(GrabbedWindow {
            image,
            frame: Bounds::new(
                point(
                    px(origin.dst_x as f32 / scale),
                    px(origin.dst_y as f32 / scale),
                ),
                size(
                    px(geometry.width as f32 / scale),
                    px(geometry.height as f32 / scale),
                ),
            ),
        })
    }
}
