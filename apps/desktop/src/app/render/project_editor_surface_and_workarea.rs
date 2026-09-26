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
use crate::app::render::sleeping_card::{sleeping_card, view_card_button, view_card_frame};
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn render_project_editor_surface(
        &mut self,
        mode: TitlebarMode,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        // CDXC:Workarea 2026-09-20 WHY:
        // An extension view that loses availability mid-frame closes the panel; the Agents column
        // beside it is already rendering, so there is nothing to draw in its place here.
        if matches!(mode, TitlebarMode::Extension(_)) && !self.titlebar_mode_available(mode) {
            let _ = self.set_active_mode(TitlebarMode::Agents, window, cx);
            return gpui::div().into_any_element();
        }
        if mode.is_project_editor_mode() && !self.project_editor_shell.is_mode_awake(mode) {
            return self.render_project_editor_sleeping_placeholder(mode, cx);
        }

        match mode {
            // The Agents workspace is never the view panel's occupant: it is the column beside it.
            TitlebarMode::Agents => gpui::div().into_any_element(),
            TitlebarMode::Browser => self.render_browser_workspace(window, cx),
            TitlebarMode::Terminal => self.render_terminal_view_surface(cx),
            TitlebarMode::Source => self.render_source_workarea_surface(cx),
            TitlebarMode::Kanban => self.render_kanban_workarea_surface(window, cx),
            TitlebarMode::Automate => self.render_automate_workarea_surface(window, cx),
            TitlebarMode::Manage => self.render_manage_workarea_surface(window, cx),
            TitlebarMode::Extension(id) => self.render_extension_workarea_surface(id, window, cx),
        }
    }

    pub(crate) fn render_project_workarea_runtime_cef_surface(
        &self,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
        surface: Entity<CefSurface>,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:Workarea 2026-06-24-10:12:
        Real Source, Kanban, Automate, and Manage CEF panes render as normal-layout GPUI children only after the app-owned slot already has a CefSurface and the corresponding gate permits placeholder replacement. Focus uses the existing project-editor surface path; no overlay, hidden child view, hit-test routing, WKWebView/WebKit path, temporary page, or fallback URL is involved.
        */
        let mode = slot_key.titlebar_mode();
        // The child view stays hidden until its first load (see `project_workarea_page_load_end_handler`); the skeleton is what shows meanwhile.
        let page_ready = self
            .project_workarea_runtime_cef_surfaces
            .get(&slot_key)
            .is_none_or(|owned| owned.page_ready());
        /*
        CDXC:Theming 2026-09-23 DECISION:
        User: "please stop making any of the views have this 6px margin from all sides when we're in glass mode we don't need it". Under window glass these pages, which cannot be see-through, fill the view panel edge to edge like the opaque window. Supersedes the same day's inset solid card.

        CDXC:Theming 2026-09-23 WHY:
        A windowed CEF page cannot be made transparent (DevTools background override and clearing its layers were both tried) and cannot be rounded either: corner radius and a mask layer on the page's native views, all the way down to Chromium's own, left its corners square. Do not retry those.
        */
        let card = div()
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
                chrome_color(0x000000, 0xffffff).into()
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
            .when(!page_ready, |this| {
                this.child(
                    div()
                        .absolute()
                        .inset_0()
                        .child(self.render_view_skeleton(mode)),
                )
            });
        card.into_any_element()
    }

    pub(crate) fn source_workarea_placeholder_signature(
        &self,
    ) -> ProjectEditorPlaceholderSignature {
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

    pub(crate) fn render_source_workarea_surface(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:CodeEditor 2026-06-23-12:16:
        Source has its own render dispatch instead of sharing the generic Kanban/Automate/Manage placeholder branch. Source must not synthesize readiness from project paths, URLs, localhost values, or native constants.

        CDXC:CodeEditor 2026-06-23-12:25:
        The normal sidebar project payload can provide sourceWorkareaId, but missing or malformed Source identity remains a placeholder-only block. Do not recover by inventing Source ids or readiness from paths, titles, fixture names, group ids, filesystem probes, URLs, or localhost constants.

        CDXC:CodeEditor 2026-06-23-14:41:
        Loading and load-failed Source code-server states may alter only the static placeholder title/message. Runtime states still cannot create fallback URLs, logs, overlays, or private shell-state fields from the render path.

        CDXC:Workarea 2026-06-29-00:02:
        Source readiness messages no longer make the runtime available. Only the app-owned code-server state for the current explicit project target can drive loading/error placeholder copy, and only a direct runtime URL plus owned CEF surface can replace the placeholder.

        CDXC:Workarea 2026-06-24-10:12:
        Source now checks the permanent app-owned CEF surface map first. When a real Source runtime URL has already produced an owned CefSurface and the gate permits replacement, render returns that normal-layout CEF child; otherwise the placeholder remains because real URL/process/surface authority is still absent.

        CDXC:Workarea 2026-06-28-17:09:
        Source render no longer constructs source-proof CEF/code-server objects. The placeholder changes only when the direct runtime URL gate plus an owned normal-layout CefSurface already exist for the current explicit project.

        CDXC:Workarea 2026-06-29-00:02:
        Source placeholder loading/error copy now comes only from the app-owned code-server runtime target for the current sidebar snapshot. Legacy Source readiness messages are compatibility no-ops and cannot make Source ready or failed.
        */
        let slot_key = ProjectWorkareaCefSurfaceSlotKey::Source;
        if let Some(surface) = self.project_workarea_runtime_cef_surface_for_render(slot_key) {
            return self.render_project_workarea_runtime_cef_surface(slot_key, surface, cx);
        }
        let signature = self.source_workarea_placeholder_signature();
        self.render_project_editor_placeholder(signature, cx)
    }

    /// Kanban is the native GPUI board in app/native_kanban/ (see the DECISION on
    /// `render_native_kanban`); the CEF Kanban slot is no longer created on desktop. Contexts with
    /// no Kanban keep the static placeholder.
    pub(crate) fn render_kanban_workarea_surface(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        if let Some(board) = self.render_native_kanban(window, cx) {
            return board;
        }
        let signature = ProjectEditorPlaceholderSignature::for_mode(TitlebarMode::Kanban)
            .expect("Kanban placeholder signature must exist");
        self.render_project_editor_placeholder(signature, cx)
    }

    /// Automate is the native GPUI page in app/native_automate/ (see the DECISION on its
    /// `Render`); the CEF Automate slot is no longer created on desktop. Projectless contexts and
    /// missing Automate identity still show the static placeholder.
    pub(crate) fn render_automate_workarea_surface(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        self.render_native_automate_surface(window, cx)
    }

    pub(crate) fn render_manage_workarea_surface(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        if let Some(docs) = self.render_native_docs(window, cx) {
            return docs;
        }
        /*
        CDXC:Workarea 2026-06-24-10:12:
        Manage now checks the permanent app-owned CEF surface map first. When a real Manage runtime URL has already produced an owned CefSurface and the CEF/file-bridge gate permits replacement, render returns that normal-layout CEF child; otherwise the placeholder remains because real navigable URL and file-bridge authority are absent.

        CDXC:Workarea 2026-06-28-17:09:
        Manage render no longer builds source-proof CEF/file-bridge mount objects. The placeholder changes only when the direct bundled runtime URL gate plus an owned normal-layout CefSurface already exist; file operations remain owned by the separate sanitized Manage bridge path.

        CDXC:Workarea 2026-06-29-00:02:
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

    pub(crate) fn render_extension_workarea_surface(
        &mut self,
        id: ExtensionId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        if self.website_home_setup_visible(TitlebarMode::Extension(id)) {
            return self.render_website_home_setup(id, window, cx);
        }
        let slot_key = ProjectWorkareaCefSurfaceSlotKey::Extension(id);
        if let Some(surface) = self.project_workarea_runtime_cef_surface_for_render(slot_key) {
            return self.render_project_workarea_runtime_cef_surface(slot_key, surface, cx);
        }
        self.render_project_editor_placeholder(self.extension_view_placeholder_signature(id), cx)
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
            .bg(glass_clear(if mode == TitlebarMode::Source {
                source_view_background_color()
            } else if CHROME_LIGHT_APPEARANCE.load(std::sync::atomic::Ordering::Relaxed) {
                rgb(0xffffff).into()
            } else {
                workspace_background_color()
            }))
            .p(px(16.0))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.focus_project_editor_surface(mode, window, cx);
                    cx.notify();
                }),
            )
            .child(if mode == TitlebarMode::Browser {
                self.render_browser_sleeping_card()
            } else {
                sleeping_card(
                    Some(
                        titlebar_svg_icon(
                            mode.tab_icon(),
                            crate::app::render::sleeping_card::SLEEPING_CARD_ICON_SIZE,
                            chrome_ink().opacity(0.8).into(),
                        )
                        .into_any_element(),
                    ),
                    mode.tab_label(),
                    true,
                )
            })
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
        let project_view = matches!(mode, TitlebarMode::Extension(id) if gpui_custom_view(id).is_some_and(|v| v.definition.get("source").is_some()));
        let has_actions = !actions.is_empty();
        // A loading state carries at most a progress title ("Starting Storybook…", "Loading source...") and no explanation; it is drawn as the view's skeleton. Errors, prompts, and explanations keep their words and buttons.
        let loading = message.is_empty()
            && title
                .as_deref()
                .is_none_or(|title| title.ends_with('…') || title.ends_with("..."));
        if loading && !has_actions {
            return self.render_view_skeleton(mode);
        }
        let mut action_row = h_flex()
            .mt(px(20.0))
            .items_center()
            .justify_center()
            .gap(px(8.0));
        for action in actions {
            let (id, label) = match action {
                ProjectEditorPlaceholderAction::ProjectViewRetry => {
                    ("project-view-retry", "Start / Retry")
                }
                ProjectEditorPlaceholderAction::ProjectViewStop => ("project-view-stop", "Stop"),
                ProjectEditorPlaceholderAction::ProjectViewOutput => {
                    ("project-view-output", "Command output")
                }
                ProjectEditorPlaceholderAction::ProjectViewOpen => {
                    ("project-view-open", "Open view")
                }
                ProjectEditorPlaceholderAction::ProjectViewConfigure => {
                    ("project-view-configure", "Configure")
                }
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
                view_card_button(label)
                    .id(id)
                    .when(
                        action == ProjectEditorPlaceholderAction::InstallSourceComponent,
                        |this| this.bg(chrome_ink().opacity(0.14)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            match action {
                                ProjectEditorPlaceholderAction::ProjectViewRetry
                                | ProjectEditorPlaceholderAction::ProjectViewStop
                                | ProjectEditorPlaceholderAction::ProjectViewOutput
                                | ProjectEditorPlaceholderAction::ProjectViewOpen
                                | ProjectEditorPlaceholderAction::ProjectViewConfigure => {
                                    if let TitlebarMode::Extension(id) = mode {
                                        let operation = match action {
                                            ProjectEditorPlaceholderAction::ProjectViewRetry => {
                                                "restart"
                                            }
                                            ProjectEditorPlaceholderAction::ProjectViewStop => {
                                                "stop"
                                            }
                                            ProjectEditorPlaceholderAction::ProjectViewOutput => {
                                                "output"
                                            }
                                            ProjectEditorPlaceholderAction::ProjectViewOpen => {
                                                "open"
                                            }
                                            _ => "configure",
                                        };
                                        this.project_view_command(
                                            &crate::app::project_views::ProjectViewCommand {
                                                id: id.as_str().into(),
                                                operation: operation.into(),
                                            },
                                            window,
                                            cx,
                                        );
                                    }
                                }
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
                    ),
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
            .bg(glass_clear(if mode == TitlebarMode::Source {
                source_view_background_color()
            } else if CHROME_LIGHT_APPEARANCE.load(std::sync::atomic::Ordering::Relaxed) {
                rgb(0xffffff).into()
            } else {
                workspace_background_color()
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.focus_project_editor_surface(mode, window, cx);
                    cx.notify();
                }),
            )
            .p(px(16.0))
            /*
            CDXC:Theming 2026-09-23 DECISION:
            User: a view's startup and status screens ("Preparing Storybook…" with Stop and Command output, errors, setup prompts) appear in a centred card in the Resume card's style: the view's icon, its title and message, and the card's soft buttons.
            */
            .child(
                view_card_frame()
                    .when(project_view, |this| this.w(px(430.0)))
                    .text_center()
                    .child(titlebar_svg_icon(
                        mode.tab_icon(),
                        34.0,
                        chrome_ink().opacity(0.8).into(),
                    ))
                    .when_some(title, |this, title| {
                        this.child(
                            div()
                                .mt(px(8.0))
                                .text_center()
                                .text_size(px(15.0))
                                .line_height(px(21.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(chrome_ink().opacity(0.9))
                                .child(title),
                        )
                    })
                    .when(!message.is_empty(), |this| {
                        this.child(
                            div()
                                .id("project-editor-placeholder-message")
                                .when(project_view, |this| {
                                    this.max_h(px(300.0)).overflow_y_scroll()
                                })
                                .mt(px(if has_title { 6.0 } else { 10.0 }))
                                .max_w_full()
                                .text_center()
                                .when(project_view, |this| this.text_left())
                                .text_size(px(12.5))
                                .line_height(px(18.0))
                                .text_color(chrome_ink().opacity(0.55))
                                .child(message),
                        )
                    })
                    .when(has_actions, |this| this.child(action_row)),
            )
            .into_any_element()
    }
}
