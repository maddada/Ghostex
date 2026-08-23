// C1 wave-4 re-cluster: further split out of app/render.rs (~7,340
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: project-editor surface/runtime CEF surface, source/kanban/automate/manage workarea surfaces, and the editor sleeping/loading placeholders.

use gpui::AnyElement;
use gpui::Entity;
use gpui::FontWeight;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::ParentElement as _;
use gpui::Styled as _;
use gpui::Window;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui::rgb;
use gpui_component::h_flex;
use gpui_component::v_flex;

use crate::app::consts::*;
use crate::app::element::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn render_project_editor_surface(
        &mut self,
        mode: TitlebarMode,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        if mode.is_project_editor_mode() && !self.project_editor_shell.is_mode_awake(mode) {
            return self.render_project_editor_sleeping_placeholder(mode, cx);
        }

        match mode {
            TitlebarMode::Agents => self.render_agents_workspace(window, cx),
            TitlebarMode::Browser => self.render_browser_workspace(window, cx),
            TitlebarMode::Source => self.render_source_workarea_surface(cx),
            TitlebarMode::Kanban => self.render_kanban_workarea_surface(cx),
            TitlebarMode::Automate => self.render_automate_workarea_surface(cx),
            TitlebarMode::Manage => self.render_manage_workarea_surface(cx),
        }
    }

    pub(crate) fn render_project_workarea_runtime_cef_surface(
        &self,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
        surface: Entity<CefSurface>,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:GPUIProjectWorkareaRuntimeCefSurfaces 2026-06-24-10:12:
        Real Source, Kanban, Automate, and Manage CEF panes render as normal-layout GPUI children only after the app-owned slot already has a CefSurface and the corresponding gate permits placeholder replacement. Focus uses the existing project-editor surface path; no overlay, hidden child view, hit-test routing, WKWebView/WebKit path, temporary page, or fallback URL is involved.
        */
        let mode = slot_key.titlebar_mode();
        div()
            .id(format!(
                "ghostex-gpui-project-workarea-runtime-cef-surface-{}",
                slot_key.privacy_label()
            ))
            .relative()
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .bg(if slot_key == ProjectWorkareaCefSurfaceSlotKey::Source {
                source_view_background_color()
            } else {
                workspace_terminal_placeholder_color()
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.focus_project_editor_surface(mode, window, cx);
                }),
            )
            .child(surface)
            .into_any_element()
    }

    pub(crate) fn source_workarea_placeholder_signature(&self) -> ProjectEditorPlaceholderSignature {
        let fallback = ProjectEditorPlaceholderSignature::for_mode(TitlebarMode::Source)
            .expect("Source placeholder signature must exist");
        let Some(snapshot) = self.latest_sidebar_project_snapshot.as_ref() else {
            return fallback;
        };
        let Some(target) = self.source_code_server_runtime_target(snapshot) else {
            return fallback;
        };
        let settings = SourceCodeServerRuntimeSettings::from_sidebar_runtime_settings(
            &self.sidebar_runtime_settings_snapshot,
        );
        if self.source_code_server_runtime.target.as_ref() != Some(&target)
            || self.source_code_server_runtime.settings.as_ref() != Some(&settings)
        {
            return fallback;
        }
        match self.source_code_server_runtime.state {
            SourceCodeServerRuntimeLaunchState::InstallRequired => {
                return ProjectEditorPlaceholderSignature {
                    mode: TitlebarMode::Source,
                    title: None,
                    message: SOURCE_CODE_SERVER_INSTALL_PROMPT.to_string(),
                    actions: vec![
                        ProjectEditorPlaceholderAction::HideCodeViewTab,
                        ProjectEditorPlaceholderAction::InstallSourceComponent,
                    ],
                };
            }
            SourceCodeServerRuntimeLaunchState::Installing => {
                let message = match self.source_code_server_runtime.install_progress {
                    Some(component_store::ComponentStoreProgressPhase::Checking) => {
                        "Checking the component…"
                    }
                    Some(component_store::ComponentStoreProgressPhase::Downloading) => {
                        "Downloading the component…"
                    }
                    Some(component_store::ComponentStoreProgressPhase::Verifying) => {
                        "Verifying the download…"
                    }
                    Some(component_store::ComponentStoreProgressPhase::Installing) => {
                        "Installing the component…"
                    }
                    Some(component_store::ComponentStoreProgressPhase::Pruning) => {
                        "Finishing installation…"
                    }
                    Some(component_store::ComponentStoreProgressPhase::Ready) | None => {
                        "Preparing Source…"
                    }
                };
                return ProjectEditorPlaceholderSignature {
                    mode: TitlebarMode::Source,
                    title: Some("Installing VS Code IDE component".to_string()),
                    message: message.to_string(),
                    actions: Vec::new(),
                };
            }
            SourceCodeServerRuntimeLaunchState::Failed => {
                return ProjectEditorPlaceholderSignature {
                    mode: TitlebarMode::Source,
                    title: Some("Source needs another try".to_string()),
                    message: self
                        .source_code_server_runtime
                        .failure
                        .unwrap_or(SourceCodeServerRuntimeFailure::Launch)
                        .placeholder_message()
                        .to_string(),
                    actions: vec![ProjectEditorPlaceholderAction::RetrySourceLoad],
                };
            }
            _ => {}
        }
        ProjectEditorPlaceholderSignature::for_source_code_server_launch_state(
            self.source_code_server_runtime.state,
            self.source_code_server_runtime
                .started_at
                .map(|started_at| started_at.elapsed()),
        )
        .unwrap_or(fallback)
    }

    pub(crate) fn render_source_workarea_surface(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        /*
        CDXC:GPUISourceWorkarea 2026-06-23-12:16:
        Source has its own render dispatch instead of sharing the generic Kanban/Automate/Manage placeholder branch. Source must not synthesize readiness from project paths, URLs, localhost values, or native constants.

        CDXC:GPUISourceWorkarea 2026-06-23-12:25:
        The normal sidebar project payload can provide sourceWorkareaId, but missing or malformed Source identity remains a placeholder-only block. Do not recover by inventing Source ids or readiness from paths, titles, fixture names, group ids, filesystem probes, URLs, or localhost constants.

        CDXC:GPUISourceWorkarea 2026-06-23-14:41:
        Loading and load-failed Source code-server states may alter only the static placeholder title/message. Runtime states still cannot create fallback URLs, logs, overlays, or private shell-state fields from the render path.

        CDXC:GPUIProjectWorkareaRuntimeCleanup 2026-06-29-00:02:
        Source readiness messages no longer make the runtime available. Only the app-owned code-server state for the current explicit project target can drive loading/error placeholder copy, and only a direct runtime URL plus owned CEF surface can replace the placeholder.

        CDXC:GPUIProjectWorkareaRuntimeCefSurfaces 2026-06-24-10:12:
        Source now checks the permanent app-owned CEF surface map first. When a real Source runtime URL has already produced an owned CefSurface and the gate permits replacement, render returns that normal-layout CEF child; otherwise the placeholder remains because real URL/process/surface authority is still absent.

        CDXC:GPUIProjectWorkareaRuntimeCefSurfaces 2026-06-28-17:09:
        Source render no longer constructs source-proof CEF/code-server objects. The placeholder changes only when the direct runtime URL gate plus an owned normal-layout CefSurface already exist for the current explicit project.

        CDXC:GPUIProjectWorkareaRuntimeCleanup 2026-06-29-00:02:
        Source placeholder loading/error copy now comes only from the app-owned code-server runtime target for the current sidebar snapshot. Legacy Source readiness messages are compatibility no-ops and cannot make Source ready or failed.
        */
        let slot_key = ProjectWorkareaCefSurfaceSlotKey::Source;
        if let Some(surface) = self.project_workarea_runtime_cef_surface_for_render(slot_key) {
            return self.render_project_workarea_runtime_cef_surface(slot_key, surface, cx);
        }
        let signature = self.source_workarea_placeholder_signature();
        self.render_project_editor_placeholder(signature, cx)
    }

    pub(crate) fn render_kanban_workarea_surface(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        /*
        CDXC:GPUIProjectWorkareaRuntimeCefSurfaces 2026-06-24-10:12:
        Kanban now checks the permanent app-owned CEF surface map first. When a real Kanban runtime URL has already produced an owned CefSurface and the gate permits replacement, render returns that normal-layout CEF child; otherwise the placeholder remains because real navigable URL authority is absent.

        CDXC:GPUIProjectWorkareaRuntimeCefSurfaces 2026-06-28-17:09:
        Kanban render no longer builds source-proof CEF mount objects. The placeholder changes only when the direct bundled runtime URL gate plus an owned normal-layout CefSurface already exist for the current explicit project.

        CDXC:GPUIProjectWorkareaRuntimeCleanup 2026-06-29-00:02:
        Kanban has no readiness store in the render path. If the direct URL/owned-CEF gate cannot produce a surface, render the static Kanban placeholder and let the active awake runtime edge try creation.
        */
        let slot_key = ProjectWorkareaCefSurfaceSlotKey::Kanban;
        if let Some(surface) = self.project_workarea_runtime_cef_surface_for_render(slot_key) {
            return self.render_project_workarea_runtime_cef_surface(slot_key, surface, cx);
        }
        let signature = ProjectEditorPlaceholderSignature::for_mode(TitlebarMode::Kanban)
            .expect("Kanban placeholder signature must exist");
        self.render_project_editor_placeholder(signature, cx)
    }

    pub(crate) fn render_automate_workarea_surface(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        /*
        CDXC:GPUIAutomateWorkarea 2026-07-04-23:18:
        Automate uses the bundled Kanban/tasks page as a first-party CEF workarea with `surface=automations`, matching macOS. It may replace the placeholder only through the same direct runtime URL plus owned CEF surface gate as Kanban; Quick/projectless contexts and missing Automate identity stay on the static placeholder.
        */
        let slot_key = ProjectWorkareaCefSurfaceSlotKey::Automate;
        if let Some(surface) = self.project_workarea_runtime_cef_surface_for_render(slot_key) {
            return self.render_project_workarea_runtime_cef_surface(slot_key, surface, cx);
        }
        let signature = ProjectEditorPlaceholderSignature::for_mode(TitlebarMode::Automate)
            .expect("Automate placeholder signature must exist");
        self.render_project_editor_placeholder(signature, cx)
    }

    pub(crate) fn render_manage_workarea_surface(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        /*
        CDXC:GPUIProjectWorkareaRuntimeCefSurfaces 2026-06-24-10:12:
        Manage now checks the permanent app-owned CEF surface map first. When a real Manage runtime URL has already produced an owned CefSurface and the CEF/file-bridge gate permits replacement, render returns that normal-layout CEF child; otherwise the placeholder remains because real navigable URL and file-bridge authority are absent.

        CDXC:GPUIProjectWorkareaRuntimeCefSurfaces 2026-06-28-17:09:
        Manage render no longer builds source-proof CEF/file-bridge mount objects. The placeholder changes only when the direct bundled runtime URL gate plus an owned normal-layout CefSurface already exist; file operations remain owned by the separate sanitized Manage bridge path.

        CDXC:GPUIProjectWorkareaRuntimeCleanup 2026-06-29-00:02:
        Manage has no readiness/file-proof store in the render path. If the direct URL/owned-CEF gate cannot produce a surface, render the static Manage placeholder while first-party file requests remain handled by the project-workarea CEF bridge.
        */
        let slot_key = ProjectWorkareaCefSurfaceSlotKey::Manage;
        if let Some(surface) = self.project_workarea_runtime_cef_surface_for_render(slot_key) {
            return self.render_project_workarea_runtime_cef_surface(slot_key, surface, cx);
        }
        let signature = ProjectEditorPlaceholderSignature::for_mode(TitlebarMode::Manage)
            .expect("Docs placeholder signature must exist");
        self.render_project_editor_placeholder(signature, cx)
    }

    pub(crate) fn render_project_editor_sleeping_placeholder(
        &self,
        mode: TitlebarMode,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let signature = ProjectEditorSleepingPlaceholderSignature::for_mode(mode)
            .expect("selected sleeping project-editor placeholders exclude Agents");
        let mode = signature.mode;

        v_flex()
            .id(format!(
                "ghostex-gpui-project-editor-sleeping-placeholder-{}",
                mode.element_slug()
            ))
            .flex_1()
            .min_w_0()
            .min_h_0()
            .w_full()
            .h_full()
            .items_center()
            .justify_center()
            .bg(if mode == TitlebarMode::Source {
                source_view_background_color()
            } else {
                workspace_background_color()
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.focus_project_editor_surface(mode, window, cx);
                    cx.notify();
                }),
            )
            .child(
                v_flex()
                    .max_w(px(430.0))
                    .min_w_0()
                    .items_center()
                    .justify_center()
                    .px(px(24.0))
                    .text_center()
                    .child(
                        div()
                            .text_center()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(workspace_terminal_placeholder_message_color())
                            .child(signature.title),
                    )
                    .child(
                        div()
                            .mt(px(5.0))
                            .max_w(px(430.0))
                            .text_center()
                            .text_size(px(12.0))
                            .line_height(px(17.0))
                            .text_color(workspace_terminal_placeholder_message_color())
                            .child(signature.message),
                    ),
            )
            .into_any_element()
    }

    pub(crate) fn render_project_editor_placeholder(
        &self,
        signature: ProjectEditorPlaceholderSignature,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let ProjectEditorPlaceholderSignature {
            mode,
            title,
            message,
            actions,
        } = signature;
        let has_title = title.is_some();
        let has_actions = !actions.is_empty();
        let mut action_row = h_flex()
            .mt(px(16.0))
            .items_center()
            .justify_center()
            .gap(px(8.0));
        for action in actions {
            let (id, label) = match action {
                ProjectEditorPlaceholderAction::HideCodeViewTab => {
                    ("ghostex-gpui-source-hide-code-tab", "Hide “Code” tab")
                }
                ProjectEditorPlaceholderAction::InstallSourceComponent => {
                    ("ghostex-gpui-source-install-component", "Install")
                }
                ProjectEditorPlaceholderAction::RetrySourceLoad => {
                    ("ghostex-gpui-source-load-retry", "Retry")
                }
            };
            action_row = action_row.child(
                div()
                    .id(id)
                    .flex()
                    .h(px(29.0))
                    .items_center()
                    .justify_center()
                    .rounded(px(5.0))
                    .border_1()
                    .border_color(rgb(0xffffff).opacity(0.18))
                    .bg(
                        if action == ProjectEditorPlaceholderAction::InstallSourceComponent {
                            rgb(0xffffff).opacity(0.14)
                        } else {
                            rgb(0xffffff).opacity(0.08)
                        },
                    )
                    .px(px(12.0))
                    .text_size(px(12.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0xffffff).opacity(0.9))
                    .cursor_pointer()
                    .hover(|this| this.bg(rgb(0xffffff).opacity(0.18)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            match action {
                                ProjectEditorPlaceholderAction::HideCodeViewTab => {
                                    this.hide_code_view_tab(cx);
                                }
                                ProjectEditorPlaceholderAction::InstallSourceComponent => {
                                    this.install_source_code_server_component(cx);
                                }
                                ProjectEditorPlaceholderAction::RetrySourceLoad => {
                                    this.retry_source_code_server_load(cx);
                                }
                            }
                        }),
                    )
                    .child(label),
            );
        }
        v_flex()
            .id(format!(
                "ghostex-gpui-project-editor-placeholder-{}",
                mode.element_slug()
            ))
            .flex_1()
            .min_w_0()
            .min_h_0()
            .w_full()
            .h_full()
            .items_center()
            .justify_center()
            .bg(if mode == TitlebarMode::Source {
                source_view_background_color()
            } else {
                workspace_background_color()
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.focus_project_editor_surface(mode, window, cx);
                    cx.notify();
                }),
            )
            .child(
                v_flex()
                    .max_w(px(430.0))
                    .min_w_0()
                    .items_center()
                    .justify_center()
                    .px(px(24.0))
                    .text_center()
                    .when_some(title, |this, title| {
                        this.child(
                            div()
                                .text_center()
                                .text_size(px(12.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(workspace_terminal_placeholder_message_color())
                                .child(title),
                        )
                    })
                    .when(!message.is_empty(), |this| {
                        this.child(
                            div()
                                .when(has_title, |this| this.mt(px(5.0)))
                                .max_w(px(430.0))
                                .text_center()
                                .text_size(px(12.0))
                                .line_height(px(17.0))
                                .text_color(workspace_terminal_placeholder_message_color())
                                .child(message),
                        )
                    })
                    .when(has_actions, |this| this.child(action_row)),
            )
            .into_any_element()
    }

}
