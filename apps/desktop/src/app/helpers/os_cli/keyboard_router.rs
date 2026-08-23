use std::{
    collections::HashMap,
    sync::atomic::Ordering,
};

use crate::app::helpers::*;
use crate::*;

#[cfg(target_os = "macos")]
pub(crate) fn register_gpui_keyboard_router_target(
    gpui_root_view: *mut std::ffi::c_void,
    app: gpui::WeakEntity<GhostexGpuiApp>,
    async_app: gpui::AsyncApp,
) {
    GPUI_KEYBOARD_ROUTER_TARGETS.with(|targets| {
        targets.borrow_mut().insert(
            gpui_root_view as usize,
            GpuiKeyboardRouterCallbackTarget {
                app,
                async_app,
                owner: GpuiKeyboardOwner::FirstResponder(FirstResponderTarget::None),
                owner_generation: 0,
                window_keyboard_id: GPUI_KEYBOARD_ROUTER_NEXT_WINDOW_ID
                    .fetch_add(1, Ordering::Relaxed),
                pressed_keys: HashMap::new(),
            },
        );
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn unregister_gpui_keyboard_router_target(gpui_root_view: *mut std::ffi::c_void) {
    let root_key = gpui_root_view as usize;
    GPUI_KEYBOARD_ROUTER_TARGETS.with(|targets| {
        targets.borrow_mut().remove(&root_key);
    });
    GPUI_KEYBOARD_PRESSED_WINDOW_OWNERS.with(|owners| {
        owners
            .borrow_mut()
            .retain(|_, owner_root_key| *owner_root_key != root_key);
    });
    GPUI_FIRST_RESPONDER_PROGRAMMATIC_DEPTHS.with(|depths| {
        depths.borrow_mut().remove(&root_key);
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn update_gpui_keyboard_router_first_responder(
    gpui_root_view: *mut std::ffi::c_void,
    first_responder: FirstResponderTarget,
) {
    GPUI_KEYBOARD_ROUTER_TARGETS.with(|targets| {
        let mut targets = targets.borrow_mut();
        let Some(target) = targets.get_mut(&(gpui_root_view as usize)) else {
            return;
        };
        let next_owner = match first_responder {
            FirstResponderTarget::GpuiWindow
                if matches!(target.owner, GpuiKeyboardOwner::CompositedTerminal(_)) =>
            {
                return;
            }
            _ => GpuiKeyboardOwner::FirstResponder(first_responder),
        };
        if target.owner != next_owner {
            let previous_owner = target.owner;
            target.owner = next_owner;
            target.owner_generation = target.owner_generation.wrapping_add(1);
            log_gpui_keyboard_owner_change(target, previous_owner, "firstResponderChanged");
        }
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn update_gpui_keyboard_router_composited_terminal_focus(
    gpui_root_view: *mut std::ffi::c_void,
    terminal: GpuiEngineTerminalEventTarget,
    focused: bool,
    first_responder: FirstResponderTarget,
) {
    GPUI_KEYBOARD_ROUTER_TARGETS.with(|targets| {
        let mut targets = targets.borrow_mut();
        let Some(target) = targets.get_mut(&(gpui_root_view as usize)) else {
            return;
        };
        if focused {
            let next_owner = GpuiKeyboardOwner::CompositedTerminal(terminal);
            if target.owner != next_owner {
                let previous_owner = target.owner;
                target.owner = next_owner;
                target.owner_generation = target.owner_generation.wrapping_add(1);
                log_gpui_keyboard_owner_change(target, previous_owner, "compositedTerminalFocused");
            }
            return;
        }
        if target.owner != GpuiKeyboardOwner::CompositedTerminal(terminal) {
            return;
        }
        let previous_owner = target.owner;
        target.owner = GpuiKeyboardOwner::FirstResponder(first_responder);
        target.owner_generation = target.owner_generation.wrapping_add(1);
        log_gpui_keyboard_owner_change(target, previous_owner, "compositedTerminalBlurred");
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn log_gpui_keyboard_owner_change(
    target: &GpuiKeyboardRouterCallbackTarget,
    previous_owner: GpuiKeyboardOwner,
    reason: &'static str,
) {
    support_logs::append(
        support_logs::GpuiSupportLog::TerminalFocus,
        "gpui.keyboardRouter.ownerChanged",
        serde_json::json!({
            "generation": target.owner_generation,
            "owner": format!("{:?}", target.owner),
            "previousOwner": format!("{previous_owner:?}"),
            "reason": reason,
            "windowKeyboardId": target.window_keyboard_id,
        }),
    );
}

#[cfg(target_os = "macos")]
pub(crate) fn log_gpui_keyboard_native_route(
    target: &GpuiKeyboardRouterCallbackTarget,
    native_action: &'static str,
    keycode: u32,
    route: &'static str,
    handled: bool,
    action_id: Option<&str>,
    owner: GpuiKeyboardOwner,
) {
    support_logs::append(
        support_logs::GpuiSupportLog::TerminalFocus,
        "gpui.keyboardRouter.nativeEventDecision",
        serde_json::json!({
            "actionId": action_id,
            "generation": target.owner_generation,
            "handled": handled,
            "keycode": keycode,
            "nativeAction": native_action,
            "owner": format!("{owner:?}"),
            "route": route,
            "windowKeyboardId": target.window_keyboard_id,
        }),
    );
}

#[cfg(target_os = "macos")]
pub(crate) fn route_gpui_native_keyboard_event(
    gpui_root_view: *mut std::ffi::c_void,
    action: std::ffi::c_int,
    keycode: u32,
    modifiers: u64,
    characters_ignoring_modifiers: &str,
    characters: &str,
) -> bool {
    const NATIVE_KEY_PRESS: std::ffi::c_int = 1;
    const NATIVE_KEY_REPEAT: std::ffi::c_int = 2;
    const NATIVE_KEY_RELEASE: std::ffi::c_int = 3;
    const SHIFT: u64 = 1 << 17;
    const CONTROL: u64 = 1 << 18;
    const OPTION: u64 = 1 << 19;
    const COMMAND: u64 = 1 << 20;
    const FUNCTION: u64 = 1 << 23;
    const TAB_KEYCODE: u32 = 48;

    let supplied_root_key = gpui_root_view as usize;
    let root_key = if action == NATIVE_KEY_PRESS {
        supplied_root_key
    } else {
        GPUI_KEYBOARD_PRESSED_WINDOW_OWNERS.with(|owners| {
            owners
                .borrow()
                .get(&keycode)
                .copied()
                .unwrap_or(supplied_root_key)
        })
    };
    if action == NATIVE_KEY_RELEASE {
        GPUI_KEYBOARD_PRESSED_WINDOW_OWNERS.with(|owners| {
            owners.borrow_mut().remove(&keycode);
        });
    }
    let routed = GPUI_KEYBOARD_ROUTER_TARGETS.with(|targets| {
        let mut targets = targets.borrow_mut();
        let target = targets.get_mut(&root_key)?;

        if action == NATIVE_KEY_RELEASE {
            return match target.pressed_keys.remove(&keycode) {
                Some(GpuiCapturedKeyRoute::CompositedTerminalTab { owner, shift }) => {
                    log_gpui_keyboard_native_route(
                        target,
                        "release",
                        keycode,
                        "compositedTerminalTab",
                        true,
                        None,
                        GpuiKeyboardOwner::CompositedTerminal(owner),
                    );
                    Some((
                        target.app.clone(),
                        target.async_app.clone(),
                        Some(GpuiNativeKeyboardDispatch::CompositedTerminalTab {
                            owner,
                            action: ghostty_vt::VtKeyAction::Release,
                            shift,
                        }),
                    ))
                }
                Some(GpuiCapturedKeyRoute::CompositedTerminalBulkText { owner }) => {
                    log_gpui_keyboard_native_route(
                        target,
                        "release",
                        keycode,
                        "compositedTerminalBulkText",
                        true,
                        None,
                        GpuiKeyboardOwner::CompositedTerminal(owner),
                    );
                    Some((target.app.clone(), target.async_app.clone(), None))
                }
                Some(GpuiCapturedKeyRoute::ApplicationCommand { command, owner }) => {
                    log_gpui_keyboard_native_route(
                        target,
                        "release",
                        keycode,
                        "applicationCommand",
                        true,
                        Some(command.log_id()),
                        owner,
                    );
                    Some((target.app.clone(), target.async_app.clone(), None))
                }
                Some(GpuiCapturedKeyRoute::GhostexHotkey { action_id, owner }) => {
                    log_gpui_keyboard_native_route(
                        target,
                        "release",
                        keycode,
                        "ghostexHotkey",
                        true,
                        Some(&action_id),
                        owner,
                    );
                    Some((target.app.clone(), target.async_app.clone(), None))
                }
                None => {
                    if keycode == TAB_KEYCODE {
                        log_gpui_keyboard_native_route(
                            target,
                            "release",
                            keycode,
                            "nativeResponderPassthrough",
                            false,
                            None,
                            target.owner,
                        );
                    }
                    None
                }
            };
        }

        if action == NATIVE_KEY_REPEAT {
            return match target.pressed_keys.get(&keycode).cloned() {
                Some(GpuiCapturedKeyRoute::CompositedTerminalTab { owner, shift }) => {
                    log_gpui_keyboard_native_route(
                        target,
                        "repeat",
                        keycode,
                        "compositedTerminalTab",
                        true,
                        None,
                        GpuiKeyboardOwner::CompositedTerminal(owner),
                    );
                    Some((
                        target.app.clone(),
                        target.async_app.clone(),
                        Some(GpuiNativeKeyboardDispatch::CompositedTerminalTab {
                            owner,
                            action: ghostty_vt::VtKeyAction::Repeat,
                            shift,
                        }),
                    ))
                }
                Some(GpuiCapturedKeyRoute::CompositedTerminalBulkText { owner }) => {
                    log_gpui_keyboard_native_route(
                        target,
                        "repeat",
                        keycode,
                        "compositedTerminalBulkText",
                        true,
                        None,
                        GpuiKeyboardOwner::CompositedTerminal(owner),
                    );
                    Some((target.app.clone(), target.async_app.clone(), None))
                }
                Some(GpuiCapturedKeyRoute::ApplicationCommand { command, owner }) => {
                    log_gpui_keyboard_native_route(
                        target,
                        "repeat",
                        keycode,
                        "applicationCommand",
                        true,
                        Some(command.log_id()),
                        owner,
                    );
                    Some((target.app.clone(), target.async_app.clone(), None))
                }
                Some(GpuiCapturedKeyRoute::GhostexHotkey { action_id, owner }) => {
                    log_gpui_keyboard_native_route(
                        target,
                        "repeat",
                        keycode,
                        "ghostexHotkey",
                        true,
                        Some(&action_id),
                        owner,
                    );
                    Some((target.app.clone(), target.async_app.clone(), None))
                }
                None => {
                    if keycode == TAB_KEYCODE {
                        log_gpui_keyboard_native_route(
                            target,
                            "repeat",
                            keycode,
                            "nativeResponderPassthrough",
                            false,
                            None,
                            target.owner,
                        );
                    }
                    None
                }
            };
        }

        if action != NATIVE_KEY_PRESS {
            return None;
        }

        let owner = target.owner;
        let native_hotkey_text =
            gpui_native_hotkey_text(keycode, modifiers, characters_ignoring_modifiers);
        let renderer_passthrough_route = if matches!(
            owner,
            GpuiKeyboardOwner::FirstResponder(FirstResponderTarget::CefSurface(
                FirstResponderCefSurface::SessionChat(_)
            ))
        ) && native_hotkey_text.as_deref() == Some("cmd+f")
        {
            Some("sessionChatRendererPassthrough")
        } else if gpui_keyboard_owner_uses_docs_editor_hotkeys(owner)
            && matches!(
                native_hotkey_text.as_deref(),
                Some("cmd+f" | "cmd+alt+f" | "cmd+y")
            )
        {
            Some("docsEditorRendererPassthrough")
        } else {
            None
        };
        if let Some(route) = renderer_passthrough_route {
            log_gpui_keyboard_native_route(target, "press", keycode, route, false, None, owner);
            return None;
        }
        if let Some(command) = native_hotkey_text
            .as_deref()
            .and_then(gpui_application_keyboard_command_for_native_text)
        {
            target.pressed_keys.insert(
                keycode,
                GpuiCapturedKeyRoute::ApplicationCommand { command, owner },
            );
            GPUI_KEYBOARD_PRESSED_WINDOW_OWNERS.with(|owners| {
                owners.borrow_mut().insert(keycode, root_key);
            });
            log_gpui_keyboard_native_route(
                target,
                "press",
                keycode,
                "applicationCommand",
                true,
                Some(command.log_id()),
                owner,
            );
            return Some((
                target.app.clone(),
                target.async_app.clone(),
                Some(GpuiNativeKeyboardDispatch::ApplicationCommand(command)),
            ));
        }
        let configured_action_id = native_hotkey_text
            .and_then(|text| gpui_configured_hotkey_action_id_for_native_text(&text));
        if let Some(action_id) = configured_action_id {
            if !gpui_keyboard_owner_allows_hotkey(owner, &action_id) {
                log_gpui_keyboard_native_route(
                    target,
                    "press",
                    keycode,
                    "ownerPolicyPassthrough",
                    false,
                    Some(&action_id),
                    owner,
                );
                return None;
            }
            target.pressed_keys.insert(
                keycode,
                GpuiCapturedKeyRoute::GhostexHotkey {
                    action_id: action_id.clone(),
                    owner,
                },
            );
            GPUI_KEYBOARD_PRESSED_WINDOW_OWNERS.with(|owners| {
                owners.borrow_mut().insert(keycode, root_key);
            });
            log_gpui_keyboard_native_route(
                target,
                "press",
                keycode,
                "ghostexHotkey",
                true,
                Some(&action_id),
                owner,
            );
            return Some((
                target.app.clone(),
                target.async_app.clone(),
                Some(GpuiNativeKeyboardDispatch::GhostexHotkey { action_id, owner }),
            ));
        }

        let only_shift = modifiers & (CONTROL | OPTION | COMMAND | FUNCTION) == 0;
        let GpuiKeyboardOwner::CompositedTerminal(owner) = owner else {
            if keycode == TAB_KEYCODE {
                log_gpui_keyboard_native_route(
                    target,
                    "press",
                    keycode,
                    "nativeResponderPassthrough",
                    false,
                    None,
                    owner,
                );
            }
            return None;
        };
        /*
        CDXC:GPUICompositedTerminalBulkUnicode 2026-07-27:
        Dictation and automation tools can post one CGEvent whose Unicode
        payload contains the whole committed string. GPUI derives Keystroke
        text from that event's physical keycode, reducing a keycode-zero bulk
        event to the layout's "a", while AppKit's NSTextInputClient path would
        receive the full event.characters string. Claim only multi-scalar,
        otherwise-unmodified text for the exact focused composited-terminal
        owner and deliver it as committed text before GPUI parses the keycode.
        Hardware keys and ordinary one-scalar terminal key events remain on
        the existing GPUI/libghostty key path.
        */
        if only_shift && characters.chars().count() > 1 {
            target.pressed_keys.insert(
                keycode,
                GpuiCapturedKeyRoute::CompositedTerminalBulkText { owner },
            );
            GPUI_KEYBOARD_PRESSED_WINDOW_OWNERS.with(|owners| {
                owners.borrow_mut().insert(keycode, root_key);
            });
            log_gpui_keyboard_native_route(
                target,
                "press",
                keycode,
                "compositedTerminalBulkText",
                true,
                None,
                GpuiKeyboardOwner::CompositedTerminal(owner),
            );
            return Some((
                target.app.clone(),
                target.async_app.clone(),
                Some(GpuiNativeKeyboardDispatch::CompositedTerminalBulkText {
                    owner,
                    text: characters.to_string(),
                }),
            ));
        }
        if keycode != TAB_KEYCODE || !only_shift {
            if keycode == TAB_KEYCODE {
                log_gpui_keyboard_native_route(
                    target,
                    "press",
                    keycode,
                    "nativeResponderPassthrough",
                    false,
                    None,
                    GpuiKeyboardOwner::CompositedTerminal(owner),
                );
            }
            return None;
        }
        let shift = modifiers & SHIFT != 0;
        target.pressed_keys.insert(
            keycode,
            GpuiCapturedKeyRoute::CompositedTerminalTab { owner, shift },
        );
        GPUI_KEYBOARD_PRESSED_WINDOW_OWNERS.with(|owners| {
            owners.borrow_mut().insert(keycode, root_key);
        });
        log_gpui_keyboard_native_route(
            target,
            "press",
            keycode,
            "compositedTerminalTab",
            true,
            None,
            GpuiKeyboardOwner::CompositedTerminal(owner),
        );
        Some((
            target.app.clone(),
            target.async_app.clone(),
            Some(GpuiNativeKeyboardDispatch::CompositedTerminalTab {
                owner,
                action: ghostty_vt::VtKeyAction::Press,
                shift,
            }),
        ))
    });
    let Some((app, mut async_app, dispatch)) = routed else {
        return false;
    };
    let Some(dispatch) = dispatch else {
        return true;
    };
    let foreground = async_app.foreground_executor().clone();
    foreground
        .spawn(async move {
            let _ = app.update_in(&mut async_app, |this, window, cx| match dispatch {
                GpuiNativeKeyboardDispatch::CompositedTerminalTab {
                    owner,
                    action,
                    shift,
                } => {
                    let _ = this.send_tab_key_to_gpui_engine_terminal(owner, action, shift, cx);
                }
                GpuiNativeKeyboardDispatch::CompositedTerminalBulkText { owner, text } => {
                    let view = this.gpui_engine_terminal_view_for_target(owner);
                    support_logs::append_temporary(
                        support_logs::GpuiSupportLog::TerminalFocus,
                        "TEMP.gpui.fluidVoice.bulkTextDispatch",
                        serde_json::json!({
                            "targetFound": view.is_some(),
                            "text": support_logs::temporary_fluid_voice_text_shape(&text),
                        }),
                    );
                    if let Some(view) = view {
                        view.update(cx, |view, cx| view.send_text_input(&text, cx));
                    }
                }
                GpuiNativeKeyboardDispatch::ApplicationCommand(command) => {
                    this.dispatch_window_scoped_application_keyboard_command(command, window, cx);
                }
                GpuiNativeKeyboardDispatch::GhostexHotkey { action_id, owner } => {
                    this.dispatch_window_scoped_ghostex_hotkey(&action_id, owner, window, cx);
                }
            });
        })
        .detach();
    true
}

