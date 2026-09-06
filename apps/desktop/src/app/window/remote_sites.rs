//! CDXC:Browser 2026-09-05 DECISION:
//! User: implement the globe menu in GPUI using the Resources/Tips native popup pattern, and match docs/2026-09-05/dev-server-status/01-server-list.html.
use crate::app::helpers::*;
use crate::*;
use futures::{FutureExt as _, StreamExt as _};
use gpui::img;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone)]
struct MachineGroup {
    id: String,
    name: String,
    connected: bool,
    loading: bool,
    error: Option<String>,
    sites: Vec<RemoteBrowserSite>,
}

pub(crate) struct RemoteSitesPanel {
    main_app: gpui::WeakEntity<GhostexGpuiApp>,
    groups: Vec<MachineGroup>,
    scroll: ScrollHandle,
    epoch: u64,
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
            groups: Vec::new(),
            scroll: ScrollHandle::new(),
            epoch: 0,
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
                    name,
                    connected: false,
                    loading: false,
                    error: None,
                    sites: Vec::new(),
                })
            })
            .collect();
        self.groups.sort_by_key(|group| group.name.to_lowercase());
        for group in &mut self.groups {
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
                            tab.remote_machine_id.as_deref() == Some(group.id.as_str())
                                && browser_url_origin_key(&tab.url) == origin
                        });
                    if let Some(tab) = tab {
                        if let Some(title) = tab
                            .runtime_page_title
                            .as_deref()
                            .and_then(sanitize_browser_tab_cached_title)
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

    fn close(&self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let _ = self.main_app.update(cx, |app, cx| {
            app.clear_gpui_titlebar_popup_from_window(GpuiTitlebarPopupKind::RemoteSites, cx)
        });
        window.remove_window();
    }

    fn render_group(&self, group: &MachineGroup, cx: &mut gpui::Context<Self>) -> AnyElement {
        let id = group.id.clone();
        let mut section = v_flex().gap(px(8.0)).mb(px(14.0)).child(
            h_flex()
                .items_center()
                .gap(px(8.0))
                .px(px(10.0))
                .py(px(9.0))
                .child(
                    svg()
                        .path(TITLEBAR_ICON_DEVICE_DESKTOP)
                        .size(px(15.0))
                        .text_color(rgb(0xaaaaaa)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(group.name.clone()),
                )
                .child(div().text_size(px(11.0)).text_color(rgb(0x999999)).child(
                    if group.loading {
                        if group.connected {
                            "Checking…".into()
                        } else {
                            "Connecting…".into()
                        }
                    } else if group.connected {
                        format!("{} locations", group.sites.len())
                    } else {
                        "Disconnected".into()
                    },
                )),
        );
        if let Some(error) = &group.error {
            section = section.child(
                div()
                    .px(px(10.0))
                    .text_size(px(12.0))
                    .text_color(rgb(0xd5ae6b))
                    .child(error.clone()),
            );
        }
        if !group.connected {
            section = section.child(
                h_flex()
                    .px(px(10.0))
                    .pb(px(10.0))
                    .gap(px(12.0))
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(12.0))
                            .text_color(rgb(0x929292))
                            .child("Connect to discover this computer's local sites."),
                    )
                    .when(!group.loading, |row| {
                        row.child(site_button(format!("connect-{id}"), "Connect").on_click(
                            cx.listener(move |panel, _, _, cx| panel.connect(id.clone(), cx)),
                        ))
                    }),
            );
        } else if group.sites.is_empty() && !group.loading && group.error.is_none() {
            section = section.child(div().px(px(10.0)).pb(px(12.0)).text_size(px(12.0)).text_color(rgb(0x929292)).child("No listening ports found. Start a dev server on this computer, then recheck."));
        }
        for site in &group.sites {
            section = section.child(self.render_site(group, site, cx));
        }
        section.into_any_element()
    }

    fn render_site(
        &self,
        group: &MachineGroup,
        site: &RemoteBrowserSite,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let key = format!("{}-{}", group.id, site.port);
        let machine_id = group.id.clone();
        let open_site = site.clone();
        let color = site.status_color();
        let icon = div()
            .flex()
            .items_center()
            .justify_center()
            .flex_shrink_0()
            .size(px(32.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(rgb(0x3c3c3c))
            .bg(rgb(0x242424));
        let icon = if let Some(favicon) = &site.favicon {
            icon.child(img(favicon.image.clone()).size(px(20.0)))
        } else {
            icon.child(
                svg()
                    .path(BROWSER_ICON_WORLD)
                    .size(px(17.0))
                    .text_color(rgb(0xb9b9b9)),
            )
        };
        let age = site.checked_at.elapsed().unwrap_or_default().as_secs();
        let evidence = format!("{} · {}s ago", site.detail, age);
        let action = if site.can_open { "Open ↗" } else { "Inspect" };
        let action_key = key.clone();
        v_flex().px(px(10.0)).py(px(14.0)).border_t_1().border_color(rgb(0x2b2b2b))
            .when(site.status.is_some_and(|code| code >= 500), |row| row.bg(rgb(0x221b1b)).rounded(px(8.0)).border_color(rgb(0x4a3232)))
            .child(h_flex().items_start().gap(px(11.0)).child(icon)
                .child(v_flex().flex_1().min_w_0().gap(px(6.0))
                    .child(div().truncate().font_weight(FontWeight::SEMIBOLD).child(site.label()))
                    .child(div().truncate().text_size(px(11.0)).text_color(rgb(0xb7b7b7)).child(site.url.clone()))
                    .child(div().truncate().text_size(px(11.0)).text_color(rgb(0x858585)).child(site.process.clone().unwrap_or_else(|| "Process unavailable".into()))))
                .child(v_flex().items_end().gap(px(7.0)).w(px(185.0)).flex_shrink_0()
                    .child(h_flex().items_center().gap(px(6.0)).text_size(px(11.0)).text_color(rgb(color))
                        .child(div().size(px(6.0)).rounded_full().bg(rgb(color))).child(site.status_label()))
                    .child(div().text_size(px(10.0)).text_color(rgb(0x929292)).child(evidence))
                    .child(site_button(format!("open-{key}"), action).on_click(cx.listener(move |panel, _, window, cx| {
                        if open_site.can_open {
                            let _ = panel.main_app.update_in(cx, |app, main_window, cx| app.open_remote_browser_site(machine_id.clone(), open_site.clone(), main_window, cx));
                            panel.close(window, cx);
                        } else {
                            if !panel.expanded.remove(&action_key) { panel.expanded.insert(action_key.clone()); }
                            cx.notify();
                        }
                    })))))
            .when(self.expanded.contains(&key), |row| row.child(div().mt(px(12.0)).p(px(10.0)).rounded(px(8.0)).bg(rgb(0x1d1d1d)).text_size(px(12.0)).text_color(rgb(0xaaaaaa)).child("This TCP port is listening, but the HTTP/HTTPS check did not return a web response. It may be a non-web service, still starting, or require a trusted certificate.")))
            .into_any_element()
    }
}

impl Drop for RemoteSitesPanel {
    fn drop(&mut self) {
        self.canceled.store(true, Ordering::Relaxed);
    }
}

fn site_button(id: String, label: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .px(px(10.0))
        .py(px(5.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(rgb(0x3b3b3b))
        .bg(rgb(0x252525))
        .text_size(px(11.0))
        .text_color(rgb(0xeeeeee))
        .cursor_pointer()
        .hover(|style| style.bg(rgb(0x303030)))
        .child(label)
}

impl Render for RemoteSitesPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let total = self
            .groups
            .iter()
            .map(|group| group.sites.len())
            .sum::<usize>();
        let responding = self
            .groups
            .iter()
            .flat_map(|group| &group.sites)
            .filter(|site| site.status.is_some_and(|code| (200..300).contains(&code)))
            .count();
        let loading = self.groups.iter().any(|group| group.loading);
        v_flex().size_full().rounded(px(12.0)).border_1().border_color(rgb(0x363636)).bg(rgb(0x161616)).text_color(rgb(0xeeeeee)).text_size(px(13.0)).overflow_hidden()
            .child(h_flex().px(px(20.0)).py(px(17.0)).gap(px(10.0)).items_center().border_b_1().border_color(rgb(0x2c2c2c))
                .child(svg().path(BROWSER_ICON_WORLD).size(px(17.0)).text_color(rgb(0xb7b7b7)))
                .child(div().flex_1().font_weight(FontWeight::SEMIBOLD).text_size(px(15.0)).child("Remote dev servers"))
                .child(site_button("remote-sites-close".into(), "Close").on_click(cx.listener(|panel, _, window, cx| panel.close(window, cx)))))
            .child(h_flex().px(px(20.0)).py(px(15.0)).items_center().gap(px(8.0))
                .child(div().font_weight(FontWeight::SEMIBOLD).child("Localhost locations"))
                .child(div().rounded(px(5.0)).px(px(6.0)).py(px(1.0)).bg(rgb(0x303030)).text_size(px(11.0)).child(total.to_string()))
                .child(div().flex_1())
                .child(site_button("remote-sites-refresh".into(), if loading { "Checking…" } else { "↻ Recheck all" }).on_click(cx.listener(move |panel, _, _, cx| { if !loading { panel.refresh(cx); } }))))
            .child(div().px(px(20.0)).pb(px(14.0)).text_size(px(11.0)).text_color(rgb(0xb4b4b4)).child(format!("{responding} responding   ·   {} need attention   ·   {} computers", total - responding, self.groups.len())))
            .child(v_flex().id("remote-sites-scroll").flex_1().min_h_0().overflow_y_scroll().track_scroll(&self.scroll).px(px(12.0))
                .when(self.groups.is_empty(), |body| body.child(v_flex().p(px(20.0)).gap(px(8.0)).child("Your remote sites will appear here").child(div().text_size(px(12.0)).text_color(rgb(0x929292)).child("Add a computer in Settings → Remote using Easy Connect or SSH."))))
                .children(self.groups.iter().map(|group| self.render_group(group, cx))))
            .child(div().px(px(20.0)).py(px(12.0)).border_t_1().border_color(rgb(0x2c2c2c)).text_size(px(10.0)).text_color(rgb(0x888888)).child("Checks confirm a response from the shown URL. Opened tabs browse through that computer."))
    }
}
