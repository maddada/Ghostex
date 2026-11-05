use std::{sync::Arc, time::Duration};

use super::*;
use serde_json::{Map, Value};

#[derive(Clone, Debug, PartialEq)]
pub struct SharedSidebarSettingsSnapshot {
    pub(crate) revision: u64,
    pub(crate) content_hash: u64,
    // Arc keeps snapshot clones cheap: hot callers (per-frame surface sync,
    // per-log scenario gating) clone the snapshot on every read, so the
    // settings object must not be deep-copied each time.
    object: Arc<Map<String, Value>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TerminalContentWidth {
    MaxWidth(f32),
    Percent(f32),
}

impl SharedSidebarSettingsSnapshot {
    pub fn empty() -> Self {
        Self {
            revision: 0,
            content_hash: hash_bytes(&[]),
            object: Arc::new(Map::new()),
        }
    }

    pub fn from_object(mut object: Map<String, Value>) -> Self {
        // Every reader (the chats, the Settings page) sees Default to Detailed view filled in from
        // the default keys it replaced (CDXC:SessionChat 2026-11-02 in the settings catalog).
        ghostex_settings_catalog::migrate_chat_view_default(&mut object);
        let content_hash = hash_settings_object(&object);
        Self {
            revision: 0,
            content_hash,
            object: Arc::new(object),
        }
    }

    pub(crate) fn with_signal(
        mut object: Map<String, Value>,
        revision: u64,
        content_hash: u64,
    ) -> Self {
        ghostex_settings_catalog::migrate_chat_view_default(&mut object);
        Self {
            revision,
            content_hash,
            object: Arc::new(object),
        }
    }

    #[allow(dead_code)]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    #[allow(dead_code)]
    pub fn content_hash(&self) -> u64 {
        self.content_hash
    }

    pub fn object(&self) -> &Map<String, Value> {
        &self.object
    }

    pub fn debugging_mode(&self) -> bool {
        strict_bool_field(&self.object, "debuggingMode") == Some(true)
    }

    /*
    CDXC:AgentLauncher 2026-08-01-16:00:
    Which built-in buttons the Agents tab strip draws. Hiding is opt-in per
    button, so a settings file written before this feature keeps every control.
    The pane overflow button has no toggle: it is the only route to the
    remaining pane actions, and hiding it would strand them.
    */
    /*
    CDXC:Extensions 2026-08-23:
    Turning Browser off in Settings → Customize also retires New Browser Tab
    from the tab strip. The per-button hide toggle stays independent — it is
    the user's own layout choice — but a button whose only outcome is a
    workarea that no longer exists has nothing left to do.
    */
    /// CDXC:Settings 2026-10-29 DECISION (cleanup Q14): the old tab strip is gone, so its Hide New
    /// Terminal / Hide New Browser Tab settings went with it; only a hidden Browser view still
    /// removes the New Browser button.
    pub fn tab_strip_built_in_buttons(&self) -> SharedTabStripBuiltInButtons {
        SharedTabStripBuiltInButtons {
            show_new_browser: strict_bool_field(&self.object, "browserViewTabHidden") != Some(true),
            show_new_terminal: true,
        }
    }

    pub fn show_beta_features(&self) -> bool {
        strict_bool_field(&self.object, "showBetaFeatures") == Some(true)
    }

    pub fn keep_awake_titlebar_settings(&self) -> SharedKeepAwakeTitlebarSettings {
        /*
        CDXC:KeepAwake 2026-06-24-13:16:
        GPUI consumes only the shared Keep Awake fields needed for the titlebar runtime. Match the TypeScript Settings defaults exactly: beta is strict boolean true only, hide and allow-display-sleep are strict booleans with false defaults, and duration normalizes only to 0, 120, or 300 minutes with 0 as the default.

        CDXC:KeepAwake 2026-06-25-23:49:
        GPUI Keep Awake automation consumes the advanced shared Settings fields with the same strict boolean defaults and battery-threshold clamp as `ghostex-settings.ts`. Keep the parsed snapshot narrow and runtime-owned: renderer payloads do not supply commands, paths, shell text, probe output, or persisted Keep Awake state.
        */
        SharedKeepAwakeTitlebarSettings {
            feature_enabled: self.show_beta_features(),
            hide_titlebar_control: strict_bool_field(&self.object, "hideKeepAwakeTitlebarControl")
                .unwrap_or(DEFAULT_HIDE_KEEP_AWAKE_TITLEBAR_CONTROL),
            activate_on_external_display: strict_bool_field(
                &self.object,
                "keepAwakeActivateOnExternalDisplay",
            )
            .unwrap_or(DEFAULT_KEEP_AWAKE_ACTIVATE_ON_EXTERNAL_DISPLAY),
            activate_on_launch: strict_bool_field(&self.object, "keepAwakeActivateOnLaunch")
                .unwrap_or(DEFAULT_KEEP_AWAKE_ACTIVATE_ON_LAUNCH),
            battery_threshold_percent: normalize_keep_awake_battery_threshold_percent(
                self.object.get("keepAwakeBatteryThresholdPercent"),
            ),
            deactivate_below_battery_threshold: normalize_keep_awake_battery_threshold_percent(
                self.object.get("keepAwakeBatteryThresholdPercent"),
            ) > 0.0,
            deactivate_on_low_power_mode: strict_bool_field(
                &self.object,
                "keepAwakeDeactivateOnLowPowerMode",
            )
            .unwrap_or(DEFAULT_KEEP_AWAKE_DEACTIVATE_ON_LOW_POWER_MODE),
            deactivate_on_user_switch: strict_bool_field(
                &self.object,
                "keepAwakeDeactivateOnUserSwitch",
            )
            .unwrap_or(DEFAULT_KEEP_AWAKE_DEACTIVATE_ON_USER_SWITCH),
            default_duration_minutes: normalize_keep_awake_duration_minutes(
                self.object.get("keepAwakeDefaultDurationMinutes"),
            ),
            allow_display_sleep: strict_bool_field(&self.object, "keepAwakeAllowDisplaySleep")
                .unwrap_or(DEFAULT_KEEP_AWAKE_ALLOW_DISPLAY_SLEEP),
            prevent_lid_sleep: strict_bool_field(&self.object, "keepAwakePreventLidSleep")
                .unwrap_or(DEFAULT_KEEP_AWAKE_PREVENT_LID_SLEEP),
            while_working_sessions: strict_bool_field(
                &self.object,
                "keepAwakeWhileWorkingSessions",
            )
            .unwrap_or(DEFAULT_KEEP_AWAKE_WHILE_WORKING_SESSIONS),
        }
    }

    pub fn gxserver_agent_settings(&self) -> SharedGxserverAgentSettings {
        SharedGxserverAgentSettings {
            agent_accept_all_enabled: strict_bool_field(&self.object, "agentAcceptAllEnabled")
                .unwrap_or(DEFAULT_AGENT_ACCEPT_ALL_ENABLED),
            default_prompt_agent_id: normalize_default_prompt_agent_id(
                self.object
                    .get("defaultPromptAgentId")
                    .and_then(Value::as_str),
            ),
        }
    }

    /// Where new threads run unless the user picks another location: `"local"`,
    /// `"agentbox:<provider>"` or `"agentbox:docker:<sshHost>"`, already in the
    /// `/api/createAgentSession` `runLocation` form.
    /// This computer while Cloud Boxes is off (Settings > Extensions), whatever default was saved.
    pub fn agentbox_default_location(&self) -> String {
        if !self.cloud_boxes_enabled() {
            return DEFAULT_AGENTBOX_LOCATION.to_string();
        }
        normalize_agentbox_location(
            self.object
                .get("agentboxDefaultLocation")
                .and_then(Value::as_str),
        )
    }

    /// Whether the Cloud Boxes built-in extension is on (never on Windows).
    pub fn cloud_boxes_enabled(&self) -> bool {
        ghostex_settings_catalog::built_in_extensions::enabled(
            &self.object,
            ghostex_settings_catalog::built_in_extensions::CLOUD_BOXES,
        )
    }

    /// Whether the Claude built-in extension is on (app/claude_web/).
    pub fn claude_web_enabled(&self) -> bool {
        ghostex_settings_catalog::built_in_extensions::enabled(
            &self.object,
            ghostex_settings_catalog::built_in_extensions::CLAUDE_WEB,
        )
    }

    /// Whether the floating Ghostex Capture button, its hotkeys and its capture tools are on.
    pub fn ghostex_capture_enabled(&self) -> bool {
        strict_bool_field(&self.object, "ghostexCaptureEnabled")
            .unwrap_or(DEFAULT_GHOSTEX_CAPTURE_ENABLED)
    }

    /// Whether a prompt sent from Ghostex Capture also shows its session in the Ghostex window.
    pub fn ghostex_capture_switch_to_session(&self) -> bool {
        strict_bool_field(&self.object, "ghostexCaptureSwitchToSession")
            .unwrap_or(DEFAULT_GHOSTEX_CAPTURE_SWITCH_TO_SESSION)
    }

    pub fn external_editor_settings(&self) -> SharedDefaultEditorSettings {
        /*
        Generic external project actions still read legacy saved editor choices
        without exposing them in Settings. Agents Hub does not use this path;
        it opens catalog-validated files in the owned Source workbench.
        */
        let default_editor_command = normalize_default_editor_command(
            self.object
                .get("defaultEditorCommand")
                .and_then(Value::as_str),
        );
        let custom_editor_command = normalize_custom_default_editor_command(
            self.object
                .get("customDefaultEditorCommand")
                .and_then(Value::as_str),
        );
        let editor_command = if default_editor_command == SharedDefaultEditorCommand::Other {
            if custom_editor_command.is_empty() {
                DEFAULT_DEFAULT_EDITOR_COMMAND.to_string()
            } else {
                custom_editor_command
            }
        } else {
            default_editor_command.as_str().to_string()
        };

        SharedDefaultEditorSettings {
            default_editor_command,
            editor_command,
        }
    }

    pub fn sidebar_default_width_px(&self) -> Option<f32> {
        self.object
            .get("sidebarDefaultWidthPx")
            .and_then(json_value_to_f32)
    }

    /// `None` when the keep-alive slider is at 0, which restores release-on-switch.
    pub fn project_switch_keep_alive(&self) -> Option<std::time::Duration> {
        let minutes = self
            .object
            .get("projectSwitchKeepAliveMinutes")
            .and_then(json_value_to_f32)
            .map(|minutes| minutes.clamp(0.0, 60.0))
            .unwrap_or(10.0);
        (minutes > 0.0).then(|| std::time::Duration::from_secs_f32(minutes * 60.0))
    }

    pub fn sidebar_visibility_memory(&self) -> SharedSidebarVisibilityMemory {
        SharedSidebarVisibilityMemory::from_settings_value(
            self.object
                .get("sidebarVisibilityMemory")
                .and_then(Value::as_str),
        )
    }

    pub fn command_pane_auto_minimize_delay(&self) -> Option<std::time::Duration> {
        if !self
            .object
            .get("commandsPanelAutoMinimize")
            .and_then(Value::as_bool)
            .unwrap_or(true)
        {
            return None;
        }
        let seconds = self
            .object
            .get("commandsPanelAutoMinimizeDelaySeconds")
            .and_then(Value::as_f64)
            .filter(|seconds| [15.0, 30.0, 60.0, 120.0, 300.0].contains(seconds))
            .unwrap_or(60.0);
        Some(std::time::Duration::from_secs(seconds as u64))
    }

    pub fn command_pane_side(&self) -> SharedCommandPaneSide {
        SharedCommandPaneSide::from_settings_value(
            self.object.get("commandsPanelSide").and_then(Value::as_str),
        )
    }

    pub fn gpui_terminal_engine_settings(&self) -> SharedGpuiTerminalEngineSettings {
        let font_weight = read_finite_number_field(
            &self.object,
            "terminalFontWeight",
            DEFAULT_TERMINAL_FONT_WEIGHT,
        )
        .clamp(MIN_TERMINAL_FONT_WEIGHT, MAX_TERMINAL_FONT_WEIGHT);
        let scrollback_limit_mb = read_finite_number_field(
            &self.object,
            "terminalScrollbackLimitMb",
            DEFAULT_TERMINAL_SCROLLBACK_LIMIT_MB,
        )
        .clamp(
            MIN_GHOSTTY_SCROLLBACK_LIMIT_MB,
            MAX_GHOSTTY_SCROLLBACK_LIMIT_MB,
        );
        SharedGpuiTerminalEngineSettings {
            clipboard_trim_trailing_spaces: read_bool_field(
                &self.object,
                "terminalClipboardTrimTrailingSpaces",
                DEFAULT_TERMINAL_CLIPBOARD_TRIM_TRAILING_SPACES,
            ),
            copy_on_select: normalize_ghostty_copy_on_select(read_string_field(
                &self.object,
                "terminalCopyOnSelect",
                DEFAULT_TERMINAL_COPY_ON_SELECT,
            )) == "clipboard",
            selection_clipboard_enabled: normalize_ghostty_copy_on_select(read_string_field(
                &self.object,
                "terminalCopyOnSelect",
                DEFAULT_TERMINAL_COPY_ON_SELECT,
            )) != "false",
            cursor_style: normalize_terminal_cursor_style(read_string_field(
                &self.object,
                "terminalCursorStyle",
                DEFAULT_TERMINAL_CURSOR_STYLE,
            )),
            cursor_style_blink: read_bool_field(
                &self.object,
                "terminalCursorStyleBlink",
                DEFAULT_TERMINAL_CURSOR_STYLE_BLINK,
            ),
            font_family: normalize_ghostty_font_family(read_string_field(
                &self.object,
                "terminalFontFamily",
                DEFAULT_TERMINAL_FONT_FAMILY,
            )),
            font_size: normalize_terminal_font_size(
                // The size Interface scale gives the terminal (CDXC:Settings 2026-11-05 in the catalog).
                ghostex_settings_catalog::interface_scale::scaled_area_value(
                    &self.object,
                    "terminalFontSize",
                )
                .map(|size| size as f32),
            ),
            font_weight: font_weight as f32,
            ghostty_theme: normalize_ghostty_theme(read_string_field(
                &self.object,
                "terminalGhosttyTheme",
                DEFAULT_TERMINAL_GHOSTTY_THEME,
            )),
            color_scheme: effective_content_color_scheme(&self.object, "terminalColorScheme")
                .to_string(),
            light_theme: read_string_field(
                &self.object,
                "terminalGhosttyLightTheme",
                "GitHub Light Default",
            )
            .to_string(),
            terminal_background: terminal_background(&self.object),
            background_image_path: read_string_field(
                &self.object,
                "terminalBackgroundImage",
                DEFAULT_TERMINAL_BACKGROUND_IMAGE,
            )
            .trim()
            .to_string(),
            background_image_opacity: read_finite_number_field(
                &self.object,
                "terminalBackgroundImageOpacity",
                DEFAULT_TERMINAL_BACKGROUND_IMAGE_OPACITY,
            )
            .clamp(0.0, 1.0) as f32,
            // CDXC:Settings 2026-10-29 DECISION (cleanup Q16): the image always covers the pane; the
            // Background Image Fit setting is gone.
            background_image_fit: normalize_terminal_background_image_fit(
                DEFAULT_TERMINAL_BACKGROUND_IMAGE_FIT,
            ),
            letter_spacing: read_finite_number_field(
                &self.object,
                "terminalLetterSpacing",
                DEFAULT_TERMINAL_LETTER_SPACING,
            )
            .clamp(MIN_TERMINAL_LETTER_SPACING, MAX_TERMINAL_LETTER_SPACING)
                as f32,
            line_height: read_finite_number_field(
                &self.object,
                "terminalLineHeight",
                DEFAULT_TERMINAL_LINE_HEIGHT,
            )
            .clamp(MIN_TERMINAL_LINE_HEIGHT, MAX_TERMINAL_LINE_HEIGHT)
                as f32,
            mouse_hide_while_typing: read_bool_field(
                &self.object,
                "terminalMouseHideWhileTyping",
                DEFAULT_TERMINAL_MOUSE_HIDE_WHILE_TYPING,
            ),
            mouse_scroll_multiplier_discrete: read_finite_number_field(
                &self.object,
                "terminalMouseScrollMultiplierDiscrete",
                DEFAULT_TERMINAL_MOUSE_SCROLL_MULTIPLIER_DISCRETE,
            )
            .clamp(
                MIN_GHOSTTY_MOUSE_SCROLL_MULTIPLIER,
                MAX_GHOSTTY_MOUSE_SCROLL_MULTIPLIER,
            ) as f32,
            mouse_scroll_multiplier_precision: read_finite_number_field(
                &self.object,
                "terminalMouseScrollMultiplierPrecision",
                DEFAULT_TERMINAL_MOUSE_SCROLL_MULTIPLIER_PRECISION,
            )
            .clamp(
                MIN_GHOSTTY_MOUSE_SCROLL_MULTIPLIER,
                MAX_GHOSTTY_MOUSE_SCROLL_MULTIPLIER,
            ) as f32,
            scrollbar_visible: normalize_ghostty_scrollbar(read_string_field(
                &self.object,
                "terminalScrollbar",
                DEFAULT_TERMINAL_SCROLLBAR,
            )) != "never",
            scrollback_limit_bytes: (scrollback_limit_mb * 1_000_000.0).round().max(1.0) as u64,
            scroll_to_bottom_when_typing: read_bool_field(
                &self.object,
                "terminalScrollToBottomWhenTyping",
                DEFAULT_TERMINAL_SCROLL_TO_BOTTOM_WHEN_TYPING,
            ),
            bold_uses_heavier_font: read_bool_field(
                &self.object,
                "terminalBoldUsesHeavierFont",
                DEFAULT_TERMINAL_BOLD_USES_HEAVIER_FONT,
            ),
            confirm_close_surface: SharedTerminalConfirmCloseSurface::from_normalized(
                &normalize_ghostty_confirm_close_surface(read_string_field(
                    &self.object,
                    "terminalConfirmCloseSurface",
                    DEFAULT_TERMINAL_CONFIRM_CLOSE_SURFACE,
                )),
            ),
        }
    }

    pub fn terminal_paste_previewable_images(&self) -> bool {
        strict_bool_field(&self.object, "terminalPastePreviewableImages")
            .unwrap_or(DEFAULT_TERMINAL_PASTE_PREVIEWABLE_IMAGES)
    }

    pub fn terminal_clipboard_paste_protection(&self) -> bool {
        strict_bool_field(&self.object, "terminalClipboardPasteProtection")
            .unwrap_or(DEFAULT_TERMINAL_CLIPBOARD_PASTE_PROTECTION)
    }

    /*
    CDXC:Navigation 2026-08-19:
    Command-clicked terminal links, session chat links, and detected dev-server
    rows share one destination. The settings file is written by the sidebar, so
    an install that has not saved settings since the merge still carries the two
    legacy keys: read them in the same precedence the TypeScript normalizer
    uses, or every one of those users would silently jump to whichever default
    this accessor happened to pick.
    */
    pub fn web_links_open_in_app(&self) -> bool {
        web_links_open_in_app_from_object(&self.object)
    }

    /// Where an HTML, Markdown or picture file opens.
    ///
    /// Until the user picks in the new dropdown (`documentFileOpenTarget` unsaved), each kind keeps
    /// following its old key exactly as before: Markdown and HTML in Files or Code, images in Files
    /// or the system app (CDXC:Navigation 2026-10-17 in packages/settings-catalog/src/file_opening.rs).
    pub fn document_file_open_target(&self, kind: SharedDocumentFileKind) -> SharedFileOpenTarget {
        if let Some(value) = self
            .object
            .get("documentFileOpenTarget")
            .and_then(Value::as_str)
        {
            return SharedFileOpenTarget::from_settings_value(Some(value));
        }
        legacy_document_file_open_target(&self.object, kind)
    }

    /// Where every other file (code and text) opens.
    pub fn other_file_open_target(&self) -> SharedFileOpenTarget {
        SharedFileOpenTarget::from_settings_value(
            self.object
                .get("otherFileOpenTarget")
                .and_then(Value::as_str),
        )
    }

    /// `fileChangesOpenTarget`: whether file-change links prefer the VS Code (code-server) view
    /// over the Changes view.
    pub fn file_changes_open_in_code(&self) -> bool {
        self.object
            .get("fileChangesOpenTarget")
            .and_then(Value::as_str)
            == Some("code")
    }

    /// Files shows its open files as tabs above the document instead of the Open Files list.
    pub fn docs_open_files_as_tabs(&self) -> bool {
        strict_bool_field(&self.object, "docsOpenFilesAsTabs").unwrap_or(true)
    }

    /// The Browser is one tab in the side panel's strip, with its pages in a row under it.
    pub fn browser_pages_in_view_row(&self) -> bool {
        strict_bool_field(&self.object, "browserPagesInViewRow").unwrap_or(false)
    }

    /// Side panel views without the three-view limit, every page of a project that is left kept
    /// for its return, and no sleep on a workspace switch
    /// (`ProjectEditorAutoSleepPolicySnapshot::keep_views_loaded`).
    pub fn keep_views_loaded(&self) -> bool {
        strict_bool_field(&self.object, "keepViewsLoaded").unwrap_or(false)
    }

    pub fn video_file_open_target(&self) -> SharedMediaFileOpenTarget {
        SharedMediaFileOpenTarget::from_settings_value(
            self.object
                .get("videoFileOpenTarget")
                .and_then(Value::as_str),
        )
    }

    pub fn audio_file_open_target(&self) -> SharedMediaFileOpenTarget {
        SharedMediaFileOpenTarget::from_settings_value(
            self.object
                .get("audioFileOpenTarget")
                .and_then(Value::as_str),
        )
    }

    pub fn terminal_pane_padding_px(&self) -> (f32, f32) {
        (
            read_finite_number_field(
                &self.object,
                "terminalPaneHorizontalPaddingPx",
                DEFAULT_TERMINAL_PANE_HORIZONTAL_PADDING_PX,
            )
            .clamp(MIN_TERMINAL_PANE_PADDING_PX, MAX_TERMINAL_PANE_PADDING_PX) as f32,
            read_finite_number_field(
                &self.object,
                "terminalPaneVerticalPaddingPx",
                DEFAULT_TERMINAL_PANE_VERTICAL_PADDING_PX,
            )
            .clamp(MIN_TERMINAL_PANE_PADDING_PX, MAX_TERMINAL_PANE_PADDING_PX) as f32,
        )
    }

    pub fn terminal_pane_layout(
        &self,
        apply_width_mode: bool,
    ) -> (f32, f32, Option<TerminalContentWidth>) {
        let (horizontal_padding, vertical_padding) = self.terminal_pane_padding_px();
        let width = if apply_width_mode {
            let legacy_mode = if strict_bool_field(&self.object, "terminalNarrowerViewEnabled")
                .unwrap_or(false)
            {
                "custom"
            } else {
                DEFAULT_TERMINAL_VIEW_WIDTH_MODE
            };
            match self
                .object
                .get("terminalViewWidthMode")
                .and_then(Value::as_str)
                .unwrap_or(legacy_mode)
            {
                "match-chat" => {
                    if strict_bool_field(&self.object, "sessionChatCustomTranscriptWidthEnabled")
                        .unwrap_or(false)
                    {
                        Some(TerminalContentWidth::Percent(
                            read_finite_number_field(
                                &self.object,
                                "sessionChatTranscriptWidthPercent",
                                DEFAULT_TERMINAL_VIEW_WIDTH_PERCENT,
                            )
                            .clamp(
                                MIN_TERMINAL_VIEW_WIDTH_PERCENT,
                                MAX_TERMINAL_VIEW_WIDTH_PERCENT,
                            ) as f32,
                        ))
                    } else {
                        Some(TerminalContentWidth::MaxWidth(
                            DEFAULT_CHAT_CONTENT_MAX_WIDTH_PX,
                        ))
                    }
                }
                "custom" => Some(TerminalContentWidth::Percent(
                    read_finite_number_field(
                        &self.object,
                        "terminalViewWidthPercent",
                        DEFAULT_TERMINAL_VIEW_WIDTH_PERCENT,
                    )
                    .clamp(
                        MIN_TERMINAL_VIEW_WIDTH_PERCENT,
                        MAX_TERMINAL_VIEW_WIDTH_PERCENT,
                    ) as f32,
                )),
                _ => None,
            }
        } else {
            None
        };
        (horizontal_padding, vertical_padding, width)
    }

    pub fn terminal_width_applies_to_command_pane_terminals(&self) -> bool {
        strict_bool_field(&self.object, "terminalWidthApplyToCommandPaneTerminals")
            .unwrap_or(DEFAULT_TERMINAL_WIDTH_APPLY_TO_COMMAND_PANE_TERMINALS)
    }

    pub fn show_session_id_in_terminal_panes(&self) -> bool {
        strict_bool_field(&self.object, "showSessionIdInTerminalPanes").unwrap_or(false)
    }

    pub fn close_side_panel_with_last_tab(&self) -> bool {
        strict_bool_field(&self.object, "closeSidePanelWithLastTab").unwrap_or(true)
    }

    pub fn auto_sleep_duration(&self, target: SharedSettingsAutoSleepTarget) -> Option<Duration> {
        let minutes_key = match target {
            SharedSettingsAutoSleepTarget::CodeEditor => "autoSleepCodeEditorIdleMinutes",
            SharedSettingsAutoSleepTarget::Browser => "autoSleepBrowserIdleMinutes",
            SharedSettingsAutoSleepTarget::ProjectEditor => "autoSleepProjectEditorIdleMinutes",
        };

        let minutes = normalize_project_editor_auto_sleep_idle_minutes(
            self.object.get(minutes_key).and_then(json_value_to_f32),
        );
        (minutes > 0.0).then(|| Duration::from_secs_f64(minutes * 60.0))
    }
}
