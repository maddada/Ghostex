use super::*;

pub(super) fn install_sidebar_project_context_v8_bridge(
    context: Option<&mut cef::V8Context>,
    runtime_settings: SidebarRuntimeSettingsSnapshot,
    gxserver_bootstrap: Option<SidebarGxserverBootstrap>,
) {
    /*
    CDXC:CefRuntime 2026-06-24-11:17:
    The CEF helper exposes fixed renderer-side GPUI bridge functions, runtimeSettings, and the real sidebar gxserver bootstrap only after the sidebar browser sends the private install message to its own main frame. gxserverBootstrap may carry only the loopback base URL, bearer token, protocol version, stable client id, and explicit gxserver ids supplied by app state; this helper is not a generic event bus and does not inspect projects, paths, URLs, titles, terminal content, cookies, filesystem markers, logs, or persistence.

    CDXC:CefRuntime 2026-06-23-06:57:
    Initial install publishes runtime settings through an already-registered `window.ghostexGpui.onRuntimeSettingsChanged(settings)` callback because the sidebar runtime can mount before CEF's load-end install message. If install wins the race, the runtime reads the installed object directly. Later refreshes use a private post-install CEF message with the same callback contract. This does not add a settings bus, project detection path, logging path, or Browser-tab bridge.

    CDXC:Settings 2026-06-24-11:22:
    The helper must mirror the macOS renderer bridge by parsing the bounded saved Settings JSON into `runtimeSettings.settings` for SidebarApp normalization while keeping Manage availability tied only to strict debuggingMode/showBetaFeatures booleans.

    CDXC:ServerDaemon 2026-06-24-11:17:
    Post-load bootstrap refresh uses a separate private sidebar message that may replace only `window.ghostexGpui.gxserverBootstrap` and call `onGxserverBootstrapChanged(bootstrap)`. Keep the helper bridge in sync with the macOS renderer bridge so ordinary Browser/workarea/modal CEF pages never receive tokens.

    CDXC:Projects 2026-06-24-14:18:
    The helper must expose the sidebar-native project path action as a named bridge function, not a generic IPC method. It forwards only one bounded string payload to the browser process; app-side Rust resolves trusted project ids through gxserver before clipboard or Finder side effects.

    CDXC:Git 2026-06-24-15:43:
    Existing-PR browser open and changed-file IDE open reuse this fixed sidebar-native bridge instead of adding renderer-owned URL/path launch APIs. The helper still forwards only one bounded string; Rust must parse the allowlisted action contract and re-query gxserver before any browser or editor side effect.

    CDXC:FocusRouting 2026-06-26-06:08:
    Workspace terminal focus is a fixed sidebar-only bridge function carrying one bounded project/session id JSON payload. The helper must not add renderer-provided commands, cwd, paths, titles, terminal text, logs, or generic native IPC for local attach behavior.

    CDXC:StatusPet 2026-06-26-04:38:
    The helper mirrors the main CEF bridge for status indicator and pet overlay state as fixed sidebar-only functions. These functions are not activation callbacks or a generic native bus; they forward only bounded presentation JSON for app-side parsing.
    */
    let Some(context) = context else {
        return;
    };
    let Some(global) = context.global() else {
        return;
    };

    let namespace_key = CefString::from(SIDEBAR_PROJECT_CONTEXT_JS_NAMESPACE);
    let mut namespace = global
        .value_bykey(Some(&namespace_key))
        .filter(|value| value.is_object() != 0)
        .or_else(|| cef::v8_value_create_object(None, None));
    let Some(namespace) = namespace.as_mut() else {
        return;
    };

    for spec in SIDEBAR_BRIDGE_FUNCTION_SPECS {
        let mut handler = GhostexGpuiSidebarBridgeV8Handler::new();
        let function_name = CefString::from(spec.js_function_name);
        let mut function = cef::v8_value_create_function(Some(&function_name), Some(&mut handler));
        let Some(function) = function.as_mut() else {
            return;
        };

        namespace.set_value_bykey(
            Some(&function_name),
            Some(function),
            V8Propertyattribute::default(),
        );
    }
    let runtime_settings_object =
        install_sidebar_runtime_settings_v8_object(context, namespace, runtime_settings);
    let _ = install_sidebar_gxserver_bootstrap_v8_object(namespace, gxserver_bootstrap);
    global.set_value_bykey(
        Some(&namespace_key),
        Some(namespace),
        V8Propertyattribute::default(),
    );
    if let Some(runtime_settings_object) = runtime_settings_object {
        notify_sidebar_runtime_settings_changed(context, namespace, runtime_settings_object);
    }
}

pub(super) fn install_project_workarea_v8_bridge(
    context: Option<&mut cef::V8Context>,
    manage_docs_resource_base_url: Option<&str>,
) {
    let Some(context) = context else {
        return;
    };
    let Some(global) = context.global() else {
        return;
    };

    let namespace_key = CefString::from(SIDEBAR_PROJECT_CONTEXT_JS_NAMESPACE);
    let mut namespace = global
        .value_bykey(Some(&namespace_key))
        .filter(|value| value.is_object() != 0)
        .or_else(|| cef::v8_value_create_object(None, None));
    let Some(namespace) = namespace.as_mut() else {
        return;
    };

    /*
    CDXC:CefRuntime 2026-06-29-14:45:
    Project workarea renderers in the helper install only the manifest-listed Kanban/Manage fixed functions. This keeps helper support in sync with macOS CEF without exposing project-workarea calls to sidebar, modal, titlebar, or Browser surfaces.
    */
    for spec in PROJECT_WORKAREA_BRIDGE_FUNCTION_SPECS {
        let mut handler = GhostexGpuiProjectWorkareaBridgeV8Handler::new();
        let function_name = CefString::from(spec.js_function_name);
        let mut function = cef::v8_value_create_function(Some(&function_name), Some(&mut handler));
        let Some(function) = function.as_mut() else {
            return;
        };

        namespace.set_value_bykey(
            Some(&function_name),
            Some(function),
            V8Propertyattribute::default(),
        );
    }

    if let Some(base_url) = manage_docs_resource_base_url {
        let _ = set_v8_string_property(
            namespace,
            PROJECT_WORKAREA_MANAGE_DOCS_RESOURCE_BASE_URL_JS_FIELD,
            base_url,
        );
    }

    global.set_value_bykey(
        Some(&namespace_key),
        Some(namespace),
        V8Propertyattribute::default(),
    );
}

pub(super) fn install_app_modal_host_v8_bridge(
    context: Option<&mut cef::V8Context>,
    surface: AppModalHostBridgeSurface,
) {
    let Some(context) = context else {
        return;
    };
    let Some(global) = context.global() else {
        return;
    };

    if surface.exposes_native_window_identity() {
        let _ = set_v8_string_property(
            &global,
            APP_MODAL_HOST_SURFACE_JS_FIELD,
            APP_MODAL_HOST_SURFACE_VALUE,
        );
        let _ =
            set_v8_string_property(&global, APP_MODAL_HOST_ID_JS_FIELD, APP_MODAL_HOST_ID_VALUE);
    }

    let Some(mut webkit) = v8_object_property_or_new(&global, WEBKIT_JS_OBJECT) else {
        return;
    };
    let Some(mut message_handlers) =
        v8_object_property_or_new(&webkit, WEBKIT_MESSAGE_HANDLERS_JS_OBJECT)
    else {
        return;
    };
    let Some(mut app_modal_host) = cef::v8_value_create_object(None, None) else {
        return;
    };

    let mut handler = GhostexGpuiAppModalHostBridgeV8Handler::new();
    let function_name = CefString::from(WEBKIT_POST_MESSAGE_JS_FUNCTION);
    let mut post_message =
        match cef::v8_value_create_function(Some(&function_name), Some(&mut handler)) {
            Some(function) => function,
            None => return,
        };
    app_modal_host.set_value_bykey(
        Some(&function_name),
        Some(&mut post_message),
        V8Propertyattribute::default(),
    );

    let app_modal_host_key = CefString::from(WEBKIT_APP_MODAL_HOST_MESSAGE_HANDLER_JS_OBJECT);
    message_handlers.set_value_bykey(
        Some(&app_modal_host_key),
        Some(&mut app_modal_host),
        V8Propertyattribute::default(),
    );

    /*
    CDXC:CefRuntime 2026-07-14:
    Helper-backed titlebar and sidebar renderers must install the same
    `ghostexNativeHost` bridge as the shell CEF path (apps/desktop/src/cef/shell.rs).
    The titlebar uses it for Resources actions, while the sidebar uses it for
    explicit native lifecycle actions such as starting gxserver. Without the
    sidebar branch, its optional bridge call silently becomes a no-op.
    */
    if matches!(
        surface,
        AppModalHostBridgeSurface::Sidebar | AppModalHostBridgeSurface::Titlebar
    ) {
        let Some(mut native_host) = cef::v8_value_create_object(None, None) else {
            return;
        };
        let mut handler = GhostexGpuiNativeHostBridgeV8Handler::new();
        let function_name = CefString::from(WEBKIT_POST_MESSAGE_JS_FUNCTION);
        let mut post_message =
            match cef::v8_value_create_function(Some(&function_name), Some(&mut handler)) {
                Some(function) => function,
                None => return,
            };
        native_host.set_value_bykey(
            Some(&function_name),
            Some(&mut post_message),
            V8Propertyattribute::default(),
        );

        let native_host_key = CefString::from(WEBKIT_NATIVE_HOST_MESSAGE_HANDLER_JS_OBJECT);
        message_handlers.set_value_bykey(
            Some(&native_host_key),
            Some(&mut native_host),
            V8Propertyattribute::default(),
        );
    }

    let message_handlers_key = CefString::from(WEBKIT_MESSAGE_HANDLERS_JS_OBJECT);
    webkit.set_value_bykey(
        Some(&message_handlers_key),
        Some(&mut message_handlers),
        V8Propertyattribute::default(),
    );

    let webkit_key = CefString::from(WEBKIT_JS_OBJECT);
    global.set_value_bykey(
        Some(&webkit_key),
        Some(&mut webkit),
        V8Propertyattribute::default(),
    );
}

pub(super) fn install_extension_v8_bridge(context: Option<&mut cef::V8Context>) {
    let Some(context) = context else {
        return;
    };
    let Some(global) = context.global() else {
        return;
    };
    let Some(mut webkit) = v8_object_property_or_new(&global, WEBKIT_JS_OBJECT) else {
        return;
    };
    let Some(mut message_handlers) =
        v8_object_property_or_new(&webkit, WEBKIT_MESSAGE_HANDLERS_JS_OBJECT)
    else {
        return;
    };
    let Some(mut extension_host) = cef::v8_value_create_object(None, None) else {
        return;
    };
    let mut handler = GhostexGpuiExtensionBridgeV8Handler::new();
    let function_name = CefString::from(WEBKIT_POST_MESSAGE_JS_FUNCTION);
    let Some(mut post_message) =
        cef::v8_value_create_function(Some(&function_name), Some(&mut handler))
    else {
        return;
    };
    extension_host.set_value_bykey(
        Some(&function_name),
        Some(&mut post_message),
        V8Propertyattribute::default(),
    );
    let host_key = CefString::from(WEBKIT_EXTENSION_HOST_MESSAGE_HANDLER_JS_OBJECT);
    message_handlers.set_value_bykey(
        Some(&host_key),
        Some(&mut extension_host),
        V8Propertyattribute::default(),
    );
    let handlers_key = CefString::from(WEBKIT_MESSAGE_HANDLERS_JS_OBJECT);
    webkit.set_value_bykey(
        Some(&handlers_key),
        Some(&mut message_handlers),
        V8Propertyattribute::default(),
    );
    let webkit_key = CefString::from(WEBKIT_JS_OBJECT);
    global.set_value_bykey(
        Some(&webkit_key),
        Some(&mut webkit),
        V8Propertyattribute::default(),
    );
}

pub(super) fn update_sidebar_runtime_settings_v8_bridge(
    context: Option<&mut cef::V8Context>,
    runtime_settings: SidebarRuntimeSettingsSnapshot,
) {
    let Some(context) = context else {
        return;
    };
    let Some(global) = context.global() else {
        return;
    };
    let namespace_key = CefString::from(SIDEBAR_PROJECT_CONTEXT_JS_NAMESPACE);
    let mut namespace = global
        .value_bykey(Some(&namespace_key))
        .filter(|value| value.is_object() != 0);
    let Some(namespace) = namespace.as_mut() else {
        return;
    };
    for spec in SIDEBAR_BRIDGE_FUNCTION_SPECS {
        let function_key = CefString::from(spec.js_function_name);
        if namespace
            .value_bykey(Some(&function_key))
            .filter(|value| value.is_function() != 0)
            .is_none()
        {
            return;
        }
    }

    let Some(runtime_settings_object) =
        install_sidebar_runtime_settings_v8_object(context, namespace, runtime_settings)
    else {
        return;
    };
    notify_sidebar_runtime_settings_changed(context, namespace, runtime_settings_object);
}

pub(super) fn update_sidebar_gxserver_bootstrap_v8_bridge(
    context: Option<&mut cef::V8Context>,
    gxserver_bootstrap: Option<SidebarGxserverBootstrap>,
) {
    let Some(context) = context else {
        return;
    };
    let Some(global) = context.global() else {
        return;
    };
    let namespace_key = CefString::from(SIDEBAR_PROJECT_CONTEXT_JS_NAMESPACE);
    let mut namespace = global
        .value_bykey(Some(&namespace_key))
        .filter(|value| value.is_object() != 0);
    let Some(namespace) = namespace.as_mut() else {
        return;
    };
    for spec in SIDEBAR_BRIDGE_FUNCTION_SPECS {
        let function_key = CefString::from(spec.js_function_name);
        if namespace
            .value_bykey(Some(&function_key))
            .filter(|value| value.is_function() != 0)
            .is_none()
        {
            return;
        }
    }

    let Some(bootstrap_object) =
        install_sidebar_gxserver_bootstrap_v8_object(namespace, gxserver_bootstrap)
    else {
        return;
    };
    notify_sidebar_gxserver_bootstrap_changed(context, namespace, bootstrap_object);
}

pub(super) fn install_session_chat_activation_v8_bridge(
    context: &mut cef::V8Context,
    activation_json: &str,
) {
    let Some(global) = context.global() else {
        return;
    };
    let namespace_key = CefString::from(SIDEBAR_PROJECT_CONTEXT_JS_NAMESPACE);
    let mut namespace = global
        .value_bykey(Some(&namespace_key))
        .filter(|value| value.is_object() != 0)
        .or_else(|| cef::v8_value_create_object(None, None));
    let Some(namespace) = namespace.as_mut() else {
        return;
    };
    let Some(mut activation) = parse_sidebar_json_v8_object(context, activation_json) else {
        return;
    };
    let Some(frame) = context.frame() else {
        return;
    };
    let frame_url = CefString::from(&frame.url()).to_string();
    let activation_url = activation
        .value_bykey(Some(&CefString::from("url")))
        .filter(|value| value.is_string() != 0)
        .map(|value| CefString::from(&value.string_value()).to_string());
    if frame.is_main() == 0
        || !is_gpui_first_party_cef_entry_url(&frame_url, "chat.html")
        || activation_url
            .as_deref()
            .and_then(|url| url.split(['?', '#']).next())
            != frame_url.split(['?', '#']).next()
    {
        return;
    }
    let Some(mut bootstrap) = activation.value_bykey(Some(&CefString::from("bootstrap"))) else {
        return;
    };
    namespace.set_value_bykey(
        Some(&CefString::from("gxserverBootstrap")),
        Some(&mut bootstrap),
        V8Propertyattribute::default(),
    );
    namespace.set_value_bykey(
        Some(&CefString::from("sessionChatActivation")),
        Some(&mut activation),
        V8Propertyattribute::default(),
    );
    global.set_value_bykey(
        Some(&namespace_key),
        Some(namespace),
        V8Propertyattribute::default(),
    );
    if let Some(callback) = namespace
        .value_bykey(Some(&CefString::from("onSessionChatActivate")))
        .filter(|value| value.is_function() != 0)
    {
        callback.execute_function_with_context(
            Some(context),
            Some(namespace),
            Some(&[Some(activation)]),
        );
    }
}

pub(super) fn install_session_chat_gxserver_bootstrap_v8_bridge(
    context: Option<&mut cef::V8Context>,
    gxserver_bootstrap: Option<SidebarGxserverBootstrap>,
) {
    let Some(context) = context else {
        return;
    };
    let Some(global) = context.global() else {
        return;
    };
    let namespace_key = CefString::from(SIDEBAR_PROJECT_CONTEXT_JS_NAMESPACE);
    let mut namespace = global
        .value_bykey(Some(&namespace_key))
        .filter(|value| value.is_object() != 0)
        .or_else(|| cef::v8_value_create_object(None, None));
    let Some(namespace) = namespace.as_mut() else {
        return;
    };
    let Some(bootstrap_object) =
        install_sidebar_gxserver_bootstrap_v8_object(namespace, gxserver_bootstrap)
    else {
        return;
    };
    global.set_value_bykey(
        Some(&namespace_key),
        Some(namespace),
        V8Propertyattribute::default(),
    );
    notify_sidebar_gxserver_bootstrap_changed(context, namespace, bootstrap_object);
}
