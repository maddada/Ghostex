use super::*;
use crate::cef::linux_x11_lost_scroll::{
    LostScroll, LostScrollTracker, ScrollAxis, cef_event_flags,
};
use std::collections::HashSet;
use x11rb::protocol::xinput::{
    ChangeReason, ConnectionExt as _, Device, DeviceClassData, EventMask, Fp3232, NotifyDetail,
    NotifyMode, ScrollType, XIEventMask,
};

static OWNERS: Mutex<Option<HashMap<X11Window, X11Window>>> = Mutex::new(None);
/// GPUI's scale for each CEF window, from `set_native_view_frame`; Chromium draws at the same scale
/// (`append_platform_command_line_switches`), so it turns X pixels into the page's view coordinates.
static SCALES: Mutex<Option<HashMap<X11Window, f64>>> = Mutex::new(None);

enum PointerMessage {
    Press {
        owner: X11Window,
        child: X11Window,
        time: u32,
    },
    LostScroll {
        owner: X11Window,
        x: f64,
        y: f64,
        modifiers: u32,
        lost: LostScroll,
    },
}

/// CDXC:FocusRouting 2026-09-19 WHY:
/// A window manager can activate GPUI's parent on a Chromium click while Chromium retains only logical focus; keys then follow the pointer or the stale address input.
/// Observe XI2 presses on Chromium's own input child, without grabbing, suppressing, or redispatching pointer events, and grant that child native focus using the original event timestamp.
/// The same thread also watches the XI2 scroll events Chromium measures, so the scrolls Chromium drops under a window manager's button grab reach the page (CDXC:PlatformSupport 2026-10-27 in linux_x11_lost_scroll.rs).
pub(super) fn install(cx: &gpui::App) {
    let (connection, screen_index) = x11_connection();
    // XI 2.1 is the first version that reports scroll classes and raw events during grabs.
    connection
        .xinput_xi_query_version(2, 2)
        .expect("XI2 query")
        .reply()
        .expect("XI2 is required for CEF pointer focus");
    let root = connection.setup().roots[*screen_index].root;
    connection
        .xinput_xi_select_events(
            root,
            &[
                EventMask {
                    deviceid: Device::ALL_MASTER.into(),
                    mask: vec![XIEventMask::RAW_MOTION],
                },
                EventMask {
                    deviceid: Device::ALL.into(),
                    mask: vec![XIEventMask::HIERARCHY | XIEventMask::DEVICE_CHANGED],
                },
            ],
        )
        .expect("CEF XI2 root selection")
        .check()
        .expect("CEF XI2 scroll observation");
    let (tx, mut rx) = mpsc::unbounded();
    std::thread::Builder::new()
        .name("cef-x11-focus".into())
        .spawn(move || {
            let mut scrolls = LostScrollTracker::default();
            scrolls.set_devices(scroll_axes_by_device(connection), touchpads(connection));
            while let Ok(event) = connection.wait_for_event() {
                let message = match event {
                    x11rb::protocol::Event::XinputButtonPress(event) => {
                        let Some(owner) = owner_of(event.event) else {
                            continue;
                        };
                        if (4..=7).contains(&event.detail) {
                            scrolls.device_event(
                                event.sourceid,
                                event.time,
                                false,
                                &set_valuators(&event.valuator_mask),
                            );
                        }
                        if event.detail != 1 {
                            continue;
                        }
                        PointerMessage::Press {
                            owner,
                            child: event.event,
                            time: event.time,
                        }
                    }
                    x11rb::protocol::Event::XinputMotion(event) => {
                        let Some(owner) = owner_of(event.event) else {
                            continue;
                        };
                        let Some(lost) = scrolls.device_event(
                            event.sourceid,
                            event.time,
                            true,
                            &set_valuators(&event.valuator_mask),
                        ) else {
                            continue;
                        };
                        PointerMessage::LostScroll {
                            owner,
                            x: fp1616(event.event_x),
                            y: fp1616(event.event_y),
                            modifiers: cef_event_flags(
                                event.mods.effective,
                                event.button_mask.first().copied().unwrap_or(0),
                            ),
                            lost,
                        }
                    }
                    x11rb::protocol::Event::XinputEnter(event) => {
                        if owner_of(event.event).is_some() {
                            scrolls.enter(
                                event.detail == NotifyDetail::INFERIOR,
                                event.mode == NotifyMode::PASSIVE_UNGRAB,
                            );
                        }
                        continue;
                    }
                    x11rb::protocol::Event::XinputRawMotion(event) => {
                        let values: Vec<(u16, f64)> = set_valuators(&event.valuator_mask)
                            .into_iter()
                            .zip(event.axisvalues.iter().map(fp3232))
                            .collect();
                        scrolls.raw_motion(event.sourceid, event.time, &values);
                        continue;
                    }
                    x11rb::protocol::Event::XinputDeviceChanged(event) => {
                        if event.reason == ChangeReason::SLAVE_SWITCH {
                            scrolls.slave_switch(event.sourceid);
                        } else {
                            scrolls.set_devices(
                                scroll_axes_by_device(connection),
                                touchpads(connection),
                            );
                        }
                        continue;
                    }
                    x11rb::protocol::Event::XinputHierarchy(_) => {
                        scrolls
                            .set_devices(scroll_axes_by_device(connection), touchpads(connection));
                        continue;
                    }
                    _ => continue,
                };
                if tx.unbounded_send(message).is_err() {
                    break;
                }
            }
        })
        .expect("CEF pointer event thread");
    let app = cx.to_async();
    cx.foreground_executor()
        .spawn(async move {
            while let Some(message) = rx.next().await {
                match message {
                    PointerMessage::Press { owner, child, time } => {
                        receive_press(&app, owner, child, time)
                    }
                    PointerMessage::LostScroll {
                        owner,
                        x,
                        y,
                        modifiers,
                        lost,
                    } => deliver_lost_scroll(owner, x, y, modifiers, lost),
                }
            }
        })
        .detach();
}

fn receive_press(app: &gpui::AsyncApp, owner: X11Window, child: X11Window, time: u32) {
    let native_view = owner as usize as *mut c_void;
    if super::super::shell::cef_native_view_is_hidden(native_view) {
        return;
    }
    let Some(host) = embed_host_for_cef_window(owner) else {
        return;
    };
    // A click in a page never reaches GPUI's own mouse-down, so it clicks away from open popups
    // here (popup_dismissal.rs): the popups in other windows, and those drawn inside the window
    // the page is in (the sidebar's menus, the titlebar's dropdowns).
    if let Some(window) = gpui_window_of_embed_host(host) {
        app.update(|cx| crate::app::window::popup_dismissal::note_page_pointer_press(window, cx));
    }
    let (connection, _) = x11_connection();
    let _ = connection.set_input_focus(InputFocus::PARENT, child, time);
    let _ = connection.flush();
    // X rejects an event older than a newer chrome focus request.
    if native_view_owns_first_responder(native_view) {
        super::super::shell::receive_native_pointer_focus(native_view);
    }
}

/// Sends the scroll Chromium dropped to the page under the pointer, at the pointer's position in
/// the browser's view coordinates (the input window covers the whole browser view).
fn deliver_lost_scroll(owner: X11Window, x: f64, y: f64, modifiers: u32, lost: LostScroll) {
    let native_view = owner as usize as *mut c_void;
    if super::super::shell::cef_native_view_is_hidden(native_view) {
        return;
    }
    let scale = SCALES
        .lock()
        .expect("CEF pointer scale registry")
        .as_ref()
        .and_then(|scales| scales.get(&owner).copied())
        .unwrap_or(1.0);
    super::super::shell::send_native_view_mouse_wheel(
        native_view,
        (x / scale).round() as i32,
        (y / scale).round() as i32,
        // EVENTFLAG_PRECISION_SCROLLING_DELTA: applied at once, like Chromium's own touchpad scrolls.
        modifiers | if lost.precise { 1 << 14 } else { 0 },
        lost.x.round() as i32,
        lost.y.round() as i32,
    );
}

fn owner_of(child: X11Window) -> Option<X11Window> {
    OWNERS
        .lock()
        .expect("CEF pointer registry")
        .as_ref()
        .and_then(|owners| owners.get(&child).copied())
}

/// Valuator numbers set in an XI2 valuator mask, in ascending order (the order of its values).
fn set_valuators(mask: &[u32]) -> Vec<u16> {
    mask.iter()
        .enumerate()
        .flat_map(|(word, bits)| {
            (0..32)
                .filter(move |bit| bits & (1 << bit) != 0)
                .map(move |bit| (word * 32 + bit) as u16)
        })
        .collect()
}

fn fp1616(value: i32) -> f64 {
    value as f64 / 65536.0
}

fn fp3232(value: &Fp3232) -> f64 {
    value.integral as f64 + value.frac as f64 / 4_294_967_296.0
}

fn scroll_axes_by_device(connection: &RustConnection) -> HashMap<u16, Vec<ScrollAxis>> {
    let Some(reply) = connection
        .xinput_xi_query_device(Device::ALL)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
    else {
        return HashMap::new();
    };
    reply
        .infos
        .into_iter()
        .map(|info| {
            let axes = info
                .classes
                .into_iter()
                .filter_map(|class| match class.data {
                    DeviceClassData::Scroll(scroll) => Some(ScrollAxis {
                        number: scroll.number,
                        horizontal: scroll.scroll_type == ScrollType::HORIZONTAL,
                        increment: fp3232(&scroll.increment),
                    }),
                    _ => None,
                })
                .collect();
            (info.deviceid, axes)
        })
        .collect()
}

/// Devices Chromium treats as touchpads: XI1 device type `TOUCHPAD`, as
/// `DeviceDataManagerX11::UpdateDeviceList` checks it.
fn touchpads(connection: &RustConnection) -> HashSet<u16> {
    let touchpad = connection
        .intern_atom(true, b"TOUCHPAD")
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .map(|reply| reply.atom);
    let Some(devices) = connection
        .xinput_list_input_devices()
        .ok()
        .and_then(|cookie| cookie.reply().ok())
    else {
        return HashSet::new();
    };
    devices
        .devices
        .into_iter()
        .filter(|device| touchpad.is_some_and(|atom| atom != 0 && device.device_type == atom))
        .map(|device| u16::from(device.device_id))
        .collect()
}

/// The GPUI window an embed host was created in (its X11 parent).
fn gpui_window_of_embed_host(host: X11Window) -> Option<gpui::WindowId> {
    let (connection, _) = x11_connection();
    let parent = connection.query_tree(host).ok()?.reply().ok()?.parent;
    super::super::shell::gpui_window_of_native_parent(parent as usize)
}

pub(super) fn observe(owner: X11Window) {
    let (connection, _) = x11_connection();
    let children = connection
        .query_tree(owner)
        .expect("CEF input children query")
        .reply()
        .expect("CEF input children")
        .children;
    for child in children {
        connection
            .xinput_xi_select_events(
                child,
                &[EventMask {
                    deviceid: Device::ALL_MASTER.into(),
                    mask: vec![
                        XIEventMask::BUTTON_PRESS | XIEventMask::MOTION | XIEventMask::ENTER,
                    ],
                }],
            )
            .expect("CEF XI2 selection")
            .check()
            .expect("CEF XI2 pointer observation");
        OWNERS
            .lock()
            .expect("CEF pointer registry")
            .get_or_insert_with(HashMap::new)
            .insert(child, owner);
    }
    let _ = connection.flush();
}

pub(super) fn note_scale(owner: X11Window, scale: f64) {
    SCALES
        .lock()
        .expect("CEF pointer scale registry")
        .get_or_insert_with(HashMap::new)
        .insert(owner, scale);
}

pub(super) fn forget(owner: X11Window) {
    if let Some(owners) = OWNERS.lock().expect("CEF pointer registry").as_mut() {
        owners.retain(|_, registered_owner| *registered_owner != owner);
    }
    if let Some(scales) = SCALES.lock().expect("CEF pointer scale registry").as_mut() {
        scales.remove(&owner);
    }
}
