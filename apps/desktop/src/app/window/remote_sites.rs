//! CDXC:Browser 2026-09-06 DECISION:
//! User: name the native globe dropdown Dev servers and move the local servers here from Resources, with this computer always first, followed by remote computers with page titles and favicons.
//! Its proportions now match Resources exactly through resources_style.rs, superseding the HTML mockup sizing.
use super::resources_style::*;
use crate::app::helpers::*;
use crate::*;
use futures::{FutureExt as _, StreamExt as _};
use gpui::img;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone)]
struct MachineGroup {
    id: String,
    local: bool,
    name: String,
    transport: &'static str,
    connected: bool,
    loading: bool,
    error: Option<String>,
    sites: Vec<RemoteBrowserSite>,
}

impl MachineGroup {
    fn local() -> Self {
        Self {
            id: String::new(),
            local: true,
            name: "This computer".into(),
            transport: "Local",
            connected: true,
            loading: false,
            error: None,
            sites: Vec::new(),
        }
    }
}

pub(crate) struct RemoteSitesPanel {
    main_app: gpui::WeakEntity<GhostexGpuiApp>,
    groups: Vec<MachineGroup>,
    scroll: ScrollHandle,
    epoch: u64,
    local_checked_at: Option<std::time::Instant>,
    info_open: bool,
    copied: Option<(String, std::time::Instant)>,
    expanded: HashSet<String>,
    canceled: Arc<AtomicBool>,
}

impl RemoteSitesPanel {
    pub(crate) fn new(
        main_app: gpui::WeakEntity<GhostexGpuiApp>,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        cx.spawn(async |this, cx| {
            let _ = this.update(cx, |this, cx| this.refresh(cx));
        })
        .detach();
        cx.spawn(async |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if this
                    .update(cx, |panel, cx| {
                        panel.update_loaded_page_metadata(cx);
                        if panel
                            .local_checked_at
                            .is_some_and(|checked| checked.elapsed() >= Duration::from_secs(3))
                        {
                            panel.refresh_local(cx);
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        Self {
            main_app,
            groups: vec![MachineGroup::local()],
            scroll: ScrollHandle::new(),
            epoch: 0,
            local_checked_at: None,
            info_open: false,
            copied: None,
            expanded: HashSet::new(),
            canceled: Arc::new(AtomicBool::new(false)),
        }
    }

    fn refresh(&mut self, cx: &mut gpui::Context<Self>) {
        self.canceled.store(true, Ordering::Relaxed);
        self.canceled = Arc::new(AtomicBool::new(false));
        self.epoch += 1;
        let epoch = self.epoch;
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        let machines = settings
            .object()
            .get("remoteMachines")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        self.groups = machines
            .iter()
            .filter_map(|machine| {
                let id = gpui_remote_machine_id_from_value(machine)?;
                let name = machine
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or("Remote computer")
                    .to_string();
                Some(MachineGroup {
                    id,
                    local: false,
                    name,
                    transport: if machine.get("transport").and_then(serde_json::Value::as_str)
                        == Some("easyConnect")
                    {
                        "Easy Connect"
                    } else {
                        "SSH"
                    },
                    connected: false,
                    loading: false,
                    error: None,
                    sites: Vec::new(),
                })
            })
            .collect();
        self.groups.sort_by_key(|group| group.name.to_lowercase());
        self.groups.insert(0, MachineGroup::local());
        for group in self.groups.iter_mut().filter(|group| !group.local) {
            let id = group.id.clone();
            let connection = self
                .main_app
                .update(cx, |app, cx| {
                    let target = app
                        .remote_gxserver_connections
                        .get(&id)?
                        .execution_target
                        .clone();
                    let config = gpui_remote_machine_config_from_settings(settings.object(), &id)?;
                    let generation = app.remote_gxserver_connect_generations.get(&id).copied();
                    app.ensure_remote_browser_tunnel(&id, true, cx);
                    Some((config, target, generation))
                })
                .ok()
                .flatten();
            let Some((config, target, generation)) = connection else {
                continue;
            };
            group.connected = true;
            group.loading = true;
            let main_app = self.main_app.clone();
            let background = cx.background_executor().clone();
            let canceled = self.canceled.clone();
            cx.spawn(async move |this, cx| {
                let mut route = Err("The browser tunnel did not become ready. Recheck to try again.".to_string());
                for _ in 0..60 {
                    if !this.update(cx, |panel, _| panel.epoch == epoch).unwrap_or(false) { return; }
                    let state = main_app.update(cx, |app, _| {
                        if !app.remote_gxserver_connections.contains_key(&id) || app.remote_gxserver_connect_generations.get(&id).copied() != generation { return Some(Err("Machine disconnected during the check.".into())); }
                        if let Some(error) = app.remote_browser.errors.get(&id) { return Some(Err(error.clone())); }
                        app.remote_browser.tunnels.get(&id).cloned().map(Ok)
                    }).ok().flatten();
                    if let Some(state) = state { route = state; break; }
                    background.timer(Duration::from_millis(250)).await;
                }
                let result = match route {
                    Ok(tunnel) => {
                        let (tx, mut rx) = futures::channel::mpsc::unbounded();
                        let mut work = background.spawn(async move { discover_remote_browser_sites(&config, &target, tunnel, canceled, tx) }).fuse();
                        loop {
                            futures::select! {
                                result = work => break result,
                                site = rx.next() => {
                                    let Some(site) = site else { break work.await; };
                                    let _ = this.update(cx, |panel, cx| {
                                        if panel.epoch != epoch { return; }
                                        if let Some(group) = panel.groups.iter_mut().find(|group| group.id == id) {
                                            group.sites.push(site);
                                            group.sites.sort_by_key(|site| site.port);
                                        }
                                        cx.notify();
                                    });
                                }
                            }
                        }
                    },
                    Err(error) => Err(error),
                };
                let still_connected = main_app.update(cx, |app, _| app.remote_gxserver_connections.contains_key(&id) && app.remote_gxserver_connect_generations.get(&id).copied() == generation).unwrap_or(false);
                let _ = this.update(cx, |panel, cx| {
                    if panel.epoch != epoch { return; }
                    if let Some(group) = panel.groups.iter_mut().find(|group| group.id == id) {
                        group.loading = false;
                        group.connected = still_connected;
                        if !still_connected { group.error = Some("Machine disconnected during the check.".into()); }
                        else { match result { Ok(sites) => group.sites = sites, Err(error) => group.error = Some(error) } }
                    }
                    cx.notify();
                });
            }).detach();
        }
        self.refresh_local(cx);
        cx.notify();
    }

    fn refresh_local(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(group) = self.groups.iter_mut().find(|group| group.local) else {
            return;
        };
        if group.loading {
            return;
        }
        group.loading = true;
        group.error = None;
        let epoch = self.epoch;
        let canceled = self.canceled.clone();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let (tx, mut rx) = futures::channel::mpsc::unbounded();
            let mut work = background.spawn(async move { discover_local_dev_servers(canceled, tx) }).fuse();
            let result = loop {
                futures::select! {
                    result = work => break result,
                    site = rx.next() => {
                        let Some(site) = site else { break work.await; };
                        let _ = this.update(cx, |panel, cx| {
                            if panel.epoch != epoch { return; }
                            if let Some(group) = panel.groups.iter_mut().find(|group| group.local) {
                                if let Some(existing) = group.sites.iter_mut().find(|existing| existing.port == site.port) { *existing = site; }
                                else { group.sites.push(site); group.sites.sort_by_key(|site| site.port); }
                            }
                            cx.notify();
                        });
                    }
                }
            };
            let _ = this.update(cx, |panel, cx| {
                if panel.epoch != epoch { return; }
                panel.local_checked_at = Some(std::time::Instant::now());
                if let Some(group) = panel.groups.iter_mut().find(|group| group.local) {
                    group.loading = false;
                    match result {
                        Ok(sites) => group.sites = sites,
                        Err(error) => { group.sites.clear(); group.error = Some(error); }
                    }
                }
                panel.update_loaded_page_metadata(cx);
                cx.notify();
            });
        }).detach();
        cx.notify();
    }

    fn update_loaded_page_metadata(&mut self, cx: &mut gpui::Context<Self>) {
        let _ = self.main_app.update(cx, |app, _| {
            for group in &mut self.groups {
                for site in &mut group.sites {
                    let origin = browser_url_origin_key(&site.url);
                    let tab = app
                        .browser_tabs
                        .tabs
                        .iter()
                        .chain(
                            app.parked_browser_tabs_by_project
                                .values()
                                .flat_map(|tabs| tabs.tabs.iter()),
                        )
                        .find(|tab| {
                            tab.remote_machine_id.as_deref()
                                == if group.local {
                                    None
                                } else {
                                    Some(group.id.as_str())
                                }
                                && browser_url_origin_key(&tab.url) == origin
                        });
                    if let Some(tab) = tab {
                        /*
                        CDXC:Browser 2026-09-18 WHY:
                        A tab title that equals the tab's URL-host fallback is
                        not a page title ("localhost" from an error page, or
                        from a title cached by older builds) and must not
                        override the title this panel just probed from the live
                        server.
                        */
                        if let Some(title) = tab
                            .runtime_page_title
                            .as_deref()
                            .and_then(sanitize_browser_tab_cached_title)
                            .filter(|title| *title != browser_tab_title_for_url(&tab.url))
                        {
                            site.title = Some(title);
                        }
                        if let Some(image) = &tab.runtime_favicon_image {
                            site.favicon = Some(image.clone());
                        }
                    }
                }
            }
        });
    }

    fn connect(&mut self, id: String, cx: &mut gpui::Context<Self>) {
        let command = serde_json::json!({ "remoteMachineId": id });
        let _ = self.main_app.update(cx, |app, cx| {
            app.handle_gpui_reconnect_remote_machine_message(
                command.as_object().expect("object"),
                cx,
            )
        });
        if let Some(group) = self.groups.iter_mut().find(|group| group.id == id) {
            group.loading = true;
        }
        let main_app = self.main_app.clone();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            for _ in 0..120 {
                background.timer(Duration::from_millis(500)).await;
                if this.update(cx, |_, _| ()).is_err() {
                    return;
                }
                if main_app
                    .update(cx, |app, _| {
                        app.remote_gxserver_connections.contains_key(&id)
                    })
                    .unwrap_or(false)
                {
                    let _ = this.update(cx, |panel, cx| panel.refresh(cx));
                    return;
                }
            }
            let _ = this.update(cx, |panel, cx| {
                if let Some(group) = panel.groups.iter_mut().find(|group| group.id == id) {
                    group.loading = false;
                    group.error = Some(
                        "Connection is not ready. Check this machine in Settings → Remote.".into(),
                    );
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// "…and close the dropdown", for the rows that open a site.
    fn close(&self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let _ = self.main_app.update(cx, |app, cx| {
            app.clear_gpui_titlebar_popup_from_window(GpuiTitlebarPopupKind::RemoteSites, cx)
        });
        window.remove_window();
    }

    fn render_group(&self, group: &MachineGroup, cx: &mut gpui::Context<Self>) -> AnyElement {
        let id = group.id.clone();
        let connection = if group.loading {
            if group.connected {
                "Checking…"
            } else {
                "Connecting…"
            }
        } else if group.connected {
            group.transport
        } else {
            "Disconnected"
        };
        let mut rows = v_flex().w_full().gap(px(7.0));
        if let Some(error) = &group.error {
            rows = rows.child(
                resource_row_frame().child(
                    resource_row_content()
                        .text_size(px(12.0))
                        .text_color(chrome_color(0xd5ae6b, 0x8a5c16))
                        .child(error.clone()),
                ),
            );
        }
        if !group.connected {
            rows = rows.child(
                resource_row_frame().child(
                    resource_row_content()
                        .child(
                            h_flex()
                                .flex_1()
                                .min_w_0()
                                .items_center()
                                .gap(px(8.0))
                                .child(div().flex_shrink_0().size(px(20.0)))
                                .child(
                                    resource_avatar_tile()
                                        .child(site_icon(TITLEBAR_ICON_DEVICE_DESKTOP, 15.0)),
                                )
                                .child(
                                    v_flex()
                                        .flex_1()
                                        .min_w_0()
                                        .gap(px(2.0))
                                        .child(resource_name_text().child(if group.loading {
                                            "Connecting to this computer…"
                                        } else {
                                            "Connect to see local sites"
                                        }))
                                        .child(resource_detail_text().child(group.transport)),
                                ),
                        )
                        .when(!group.loading, |row| {
                            row.child(site_button(format!("connect-{id}"), "Connect").on_click(
                                cx.listener(move |panel, _, _, cx| panel.connect(id.clone(), cx)),
                            ))
                        }),
                ),
            );
        } else if group.sites.is_empty() && group.error.is_none() {
            rows = rows.child(
                resource_row_frame().child(
                    resource_row_content()
                        .text_size(px(12.0))
                        .text_color(chrome_ink().opacity(0.58))
                        .child(if group.loading {
                            "Checking this computer's local sites…"
                        } else {
                            "No dev servers found. Start a server on this computer."
                        }),
                ),
            );
        }
        for site in &group.sites {
            rows = rows.child(self.render_site(group, site, cx));
        }
        v_flex()
            .w_full()
            .flex_shrink_0()
            .child(
                resource_section_heading()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(group.name.to_uppercase()),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap(px(10.0))
                            .text_color(chrome_ink().opacity(0.52))
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap(px(4.0))
                                    .child(status_dot(if group.connected {
                                        0x9bb79f
                                    } else {
                                        0x666666
                                    }))
                                    .child(connection),
                            )
                            .child(
                                div()
                                    .text_color(chrome_ink().opacity(0.38))
                                    .child(group.sites.len().to_string()),
                            ),
                    ),
            )
            .child(rows)
            .into_any_element()
    }

    fn render_site(
        &self,
        group: &MachineGroup,
        site: &RemoteBrowserSite,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let key = format!("{}-{}", group.id, site.port);
        let machine_id = (!group.local).then(|| group.id.clone());
        let open_site = site.clone();
        let color = site.status_color();
        let icon = if let Some(favicon) = &site.favicon {
            resource_avatar_tile().child(img(favicon.image.clone()).size(px(15.0)))
        } else {
            resource_avatar_tile().child(site_icon(BROWSER_ICON_WORLD, 15.0))
        };
        let action_key = key.clone();
        let copy_key = key.clone();
        let copy_url = site.url.clone();
        let copied = self
            .copied
            .as_ref()
            .is_some_and(|(id, time)| id == &key && time.elapsed() < Duration::from_secs(2));
        let evidence = if site.status.is_some() {
            site.detail
                .strip_prefix("HTTP ")
                .unwrap_or(&site.detail)
                .to_string()
        } else {
            "No HTTP".into()
        };
        let detail = site.detail.clone();
        let title = site.label();
        let full_title = title.clone();
        let address = site
            .url
            .strip_prefix("http://")
            .unwrap_or(&site.url)
            .trim_end_matches('/')
            .to_string();
        let subtitle = match &site.process {
            Some(process) => format!("{address} · {process}"),
            None => address,
        };
        let identity = h_flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .gap(px(8.0))
            .child(div().flex_shrink_0().size(px(20.0)))
            .child(icon)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.0))
                    .child(
                        resource_name_text()
                            .id(format!("title-{key}"))
                            .child(title)
                            .tooltip(move |window, cx| {
                                Tooltip::new(full_title.clone()).build(window, cx)
                            }),
                    )
                    .child(
                        resource_detail_text()
                            .id(format!("subtitle-{key}"))
                            .child(subtitle.clone())
                            .tooltip(move |window, cx| {
                                Tooltip::new(subtitle.clone()).build(window, cx)
                            }),
                    ),
            );
        let primary = site_action_slot().child(
            site_icon_button(
                format!("open-{key}"),
                if site.can_open {
                    "titlebar/focus-2.svg"
                } else {
                    TITLEBAR_ICON_INFO
                },
                if site.can_open {
                    "Open in the embedded browser"
                } else {
                    "Inspect web check"
                },
            )
            .on_click(cx.listener(move |panel, _, window, cx| {
                if open_site.can_open {
                    let _ = panel.main_app.update_in(cx, |app, main_window, cx| {
                        if let Some(machine_id) = &machine_id {
                            app.open_remote_browser_site(
                                machine_id.clone(),
                                open_site.clone(),
                                main_window,
                                cx,
                            );
                        } else {
                            app.open_local_dev_server(open_site.clone(), main_window, cx);
                        }
                    });
                    panel.close(window, cx);
                } else {
                    if !panel.expanded.remove(&action_key) {
                        panel.expanded.insert(action_key.clone());
                    }
                    cx.notify();
                }
            })),
        );
        let secondary = site_action_slot().child(
            site_icon_button(
                format!("copy-{key}"),
                if copied {
                    "titlebar/check.svg"
                } else {
                    "titlebar/copy.svg"
                },
                if copied { "Copied" } else { "Copy URL" },
            )
            .on_click(cx.listener(move |panel, _, _, cx| {
                gpui_copy_to_clipboard(ClipboardItem::new_string(copy_url.clone()), cx);
                panel.copied = Some((copy_key.clone(), std::time::Instant::now()));
                cx.notify();
            })),
        );
        let status = match site.status_label() {
            "Login required" => "Login",
            "Certificate issue" => "Certificate",
            "No web response" => "No HTTP",
            label => label,
        };
        let status_detail = site.status_label();
        resource_row_frame().flex_shrink_0()
            .child(resource_row_content().child(identity).child(primary).child(secondary)
                .child(h_flex().flex_shrink_0().w(px(200.0)).gap(px(8.0))
                    .child(resource_metric(86.0).id(format!("status-{key}"))
                        .child(status_dot(color)).child(status)
                        .when(status != status_detail, |this| this.tooltip(move |window, cx| Tooltip::new(status_detail).build(window, cx))))
                    .child(resource_metric(106.0).id(format!("evidence-{key}"))
                        .child(evidence)
                        .tooltip(move |window, cx| Tooltip::new(detail.clone()).build(window, cx)))))
            .when(self.expanded.contains(&key), |row| row.child(div().pb(px(8.0)).pr(px(8.0)).pl(px(64.0))
                .text_size(px(12.0)).text_color(chrome_ink().opacity(0.58))
                .child(format!("{}. This TCP port is listening, but the HTTP/HTTPS check did not return a web response. It may be a non-web service or still starting.", site.detail))))
            .into_any_element()
    }
}

impl Drop for RemoteSitesPanel {
    fn drop(&mut self) {
        self.canceled.store(true, Ordering::Relaxed);
    }
}

fn site_icon(path: &'static str, size: f32) -> gpui::Svg {
    svg()
        .path(path)
        .size(px(size))
        .text_color(chrome_ink().opacity(0.82))
}

fn status_dot(color: u32) -> gpui::Div {
    div()
        .flex_shrink_0()
        .size(px(5.0))
        .rounded_full()
        .bg(chrome_color(
            color,
            match color {
                0x7acb9d | 0x9bb79f => 0x287a48,
                0xe28b88 => 0xb43d39,
                0xd5ae6b => 0x8a5c16,
                _ => 0x666666,
            },
        ))
}

fn site_action_slot() -> gpui::Div {
    div()
        .flex_shrink_0()
        .flex()
        .w(px(24.0))
        .items_center()
        .justify_center()
}

fn site_icon_button(
    id: String,
    icon: &'static str,
    tooltip: &'static str,
) -> gpui::Stateful<gpui::Div> {
    resource_square_button(id)
        .child(site_icon(icon, 12.0).text_color(chrome_ink().opacity(0.90)))
        .tooltip(move |window, cx| Tooltip::new(tooltip).build(window, cx))
}

fn site_button(id: String, label: &'static str) -> gpui::Stateful<gpui::Div> {
    h_flex()
        .id(id)
        .flex_shrink_0()
        .h(px(22.0))
        .items_center()
        .justify_center()
        .rounded(px(RESOURCE_CONTROL_RADIUS))
        .border_1()
        .border_color(chrome_ink().opacity(0.13))
        .bg(chrome_ink().opacity(0.08))
        .px(px(8.0))
        .text_size(px(11.0))
        .text_color(chrome_ink().opacity(0.86))
        .cursor_pointer()
        .hover(|this| this.bg(chrome_ink().opacity(0.14)))
        .child(label)
}

fn toolbar_segment(id: &'static str) -> gpui::Stateful<gpui::Div> {
    h_flex()
        .id(id)
        .h_full()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .gap(px(8.0))
        .px(px(15.0))
        .border_l_1()
        .border_color(chrome_ink().opacity(0.12))
        .text_size(px(TITLEBAR_POPUP_READING_HEADER_BUTTON_TEXT_SIZE))
        .font_weight(FontWeight::NORMAL)
        .text_color(chrome_ink().opacity(0.78))
}

impl Render for RemoteSitesPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let total = self
            .groups
            .iter()
            .map(|group| group.sites.len())
            .sum::<usize>();
        let connected = self
            .groups
            .iter()
            .filter(|group| group.connected && !group.local)
            .count();
        let loading = self.groups.iter().any(|group| group.loading);
        let checked = self
            .groups
            .iter()
            .flat_map(|group| &group.sites)
            .map(|site| site.checked_at)
            .max();
        let freshness = if loading {
            "Checking…".to_string()
        } else if let Some(checked) = checked {
            let age = checked.elapsed().unwrap_or_default().as_secs();
            if age < 5 {
                "Checked just now".into()
            } else if age < 60 {
                format!("Checked {age}s ago")
            } else {
                format!("Checked {}m ago", age / 60)
            }
        } else {
            "No sites checked".into()
        };
        resource_panel_frame().child(v_flex().size_full().overflow_hidden()
            .child(resource_header()
                .child(resource_heading()
                    .child(site_icon(BROWSER_ICON_WORLD, 18.0).text_color(chrome_ink().opacity(0.96)))
                    .child(div().truncate().child("Dev servers")))
                .child(toolbar_segment("remote-sites-info").px_0().w(px(TITLEBAR_POPUP_READING_HEADER_HEIGHT))
                    .cursor_pointer().hover(|this| this.bg(chrome_ink().opacity(0.14)))
                    .when(self.info_open, |this| this.bg(chrome_ink().opacity(0.14)))
                    .child(site_icon(TITLEBAR_ICON_INFO, TITLEBAR_POPUP_READING_HEADER_BUTTON_ICON_SIZE))
                    .tooltip(|window, cx| Tooltip::new("About dev servers").build(window, cx))
                    .on_click(cx.listener(|panel, _, _, cx| { panel.info_open = !panel.info_open; cx.notify(); })))
                .child(toolbar_segment("remote-sites-refresh")
                    .when(!loading, |this| this.cursor_pointer().hover(|this| this.bg(chrome_ink().opacity(0.14))))
                    .when(loading, |this| this.text_color(chrome_ink().opacity(0.30)).opacity(0.55))
                    .child(site_icon(BROWSER_ICON_RELOAD, TITLEBAR_POPUP_READING_HEADER_BUTTON_ICON_SIZE)
                        .text_color(chrome_ink().opacity(if loading { 0.30 } else { 0.78 })))
                    .child(if loading { "Checking…" } else { "Recheck all" })
                    .on_click(cx.listener(|panel, _, _, cx| { if !panel.groups.iter().any(|group| group.loading) { panel.refresh(cx); } })))
                .child(toolbar_segment("remote-sites-connected").px(px(12.0)).gap(px(5.0)).text_color(chrome_ink().opacity(0.72))
                    .child(site_icon(TITLEBAR_ICON_DEVICE_DESKTOP, 13.0).text_color(chrome_ink().opacity(0.62)))
                    .child(format!("{connected} remote"))))
            .when(self.info_open, |panel| panel.child(div().flex_shrink_0().p(px(10.0)).border_b_1()
                .border_color(chrome_ink().opacity(0.14)).bg(chrome_color(0x3a3a3a, 0xf0f0f0)).text_size(px(12.0)).line_height(px(16.2)).text_color(chrome_ink().opacity(0.62))
                .child("Dev servers on this computer appear first and update automatically. Remote computers follow below. Open browses through the computer running that server; Copy URL copies its localhost address. Recheck all refreshes discovery and web checks.")))
            .child(div().relative().w_full().min_h_0().flex_1()
                .child(v_flex().id("remote-sites-scroll").size_full().overflow_y_scroll().track_scroll(&self.scroll)
                    .p(px(10.0)).pt(px(8.0)).gap(px(8.0))
                    .children(self.groups.iter().map(|group| self.render_group(group, cx))))
                .child(Scrollbar::vertical(&self.scroll).thickness(px(TITLEBAR_DROPDOWN_SCROLLBAR_WIDTH))))
            .child(h_flex().flex_shrink_0().px(px(12.0)).py(px(7.0)).gap(px(8.0)).items_center()
                .border_t_1().border_color(chrome_ink().opacity(0.12)).text_size(px(11.0)).text_color(chrome_ink().opacity(0.52))
                .child(div().flex_1().min_w_0().truncate().child("Local first · Remote sites use their computer"))
                .child(format!("{total} locations · {freshness}"))))
    }
}
