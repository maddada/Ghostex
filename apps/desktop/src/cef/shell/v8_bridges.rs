use super::*;

pub(crate) fn install_sidebar_project_context_v8_bridge(
    context: Option<&mut cef::V8Context>,
    runtime_settings: SidebarRuntimeSettingsSnapshot,
    gxserver_bootstrap: Option<SidebarGxserverBootstrap>,
) {
    /*
    CDXC:CefRuntime 2026-06-23-18:29:
    The renderer-side sidebar bridge exposes only fixed typed string-payload functions for active-project context, Source readiness, Browser readiness, project-workarea readiness, Manage operation requests, sidebar-native side-effect requests, gxserver focus-state hints, and workspace terminal focus and rename requests, plus `window.ghostexGpui.runtimeSettings` with strict debuggingMode/showBetaFeatures booleans and the saved shared Settings object. It does not expose generic message names, event buses, filesystem/project detection, trusted file paths, URL/title inspection, arbitrary logging, persistence, or fallback project inference.

    CDXC:CefRuntime 2026-06-23-06:57:
    Initial install publishes runtime settings through an already-registered `window.ghostexGpui.onRuntimeSettingsChanged(settings)` callback because the sidebar runtime can mount before CEF's load-end install message. If install wins the race, the runtime reads the installed object directly. Later refreshes use a second private browser-to-renderer CEF message with the same callback contract. This keeps ordinary Browser tabs out of the sidebar bridge and avoids a generic event/settings bus.

    CDXC:Settings 2026-06-24-11:22:
    The runtimeSettings object also carries the saved shared Settings object for the sidebar renderer to normalize with the shared TypeScript schema. This remains a narrow sidebar-owned handoff: the CEF boundary accepts only the serialized object already read by GPUI, parses it into one V8 object property, and does not expose a generic settings API, persistence hook, logging path, URL/title state, command text, tokens, or fallback project inference.

    CDXC:ServerDaemon 2026-06-24-11:17:
    The same sidebar-only private install message may set `window.ghostexGpui.gxserverBootstrap` from real local gxserver facts: loopback base URL, bearer token, protocol version, stable client id, and optional gxserver ids only when app state already owns them. Do not derive ids from paths, titles, fixtures, shell placeholders, Browser tabs, terminal state, logs, persistence, or fallback project detection.

    CDXC:FocusRouting 2026-06-24-21:07:
    The focus-state bridge is a fixed sidebar-only string payload used to return React-owned gxserver presentation session ids to Rust for bootstrap replay. It must remain separate from native path actions and must not carry paths, titles, command text, terminal contents, tokens, daemon response bodies, or renderer-derived labels.

    CDXC:FocusRouting 2026-06-26-06:08:
    The workspace terminal focus bridge is fixed-function and sidebar-only. It may carry only the gxserver project/session ids React already focused so Rust can select or materialize the matching Agents tab from gxserver attach metadata; it must not accept labels, commands, paths, terminal contents, daemon responses, or generic terminal IPC.

    CDXC:SessionTitles 2026-06-27-02:27:
    Workspace terminal rename parity adds one fixed sidebar-only bridge function for the already-trimmed rename title plus gxserver project/session ids. CEF still exposes no generic terminal-text sender, command bus, cwd/path authority, logging path, renderer-selected target surface, or fallback terminal IPC.

    CDXC:CommandPane 2026-06-24-23:17:
    The sidebar command-action bridge is a fixed-function handoff for the shared SidebarApp `runSidebarCommand` message. It may carry the selected gxserver HUD action fields to app Rust, but it must not expose generic IPC, filesystem/project discovery, logs, persistence, terminal content, stdout/stderr, or renderer-side execution authority.

    CDXC:CommandPane 2026-06-26-00:05:
    Terminal Actions may include the terminal-only `closeTerminalOnExit` boolean in the fixed command-action JSON so GPUI can match macOS close-on-exit behavior. Browser Actions must not use that flag, and CEF still forwards only the bounded sidebar payload string for Rust-side strict parsing.

    CDXC:AppShots 2026-06-25-23:28:
    The App Shot prompt bridge is fixed-function and sidebar-only. It may carry only the validated gxserver presentation session id and the already formatted app-owned prompt string; screenshot paths are not accepted as separate authority and the bridge is not a generic terminal text IPC.

    CDXC:AppShots 2026-06-26-04:27:
    Remote App Shot insertion still uses this fixed bridge shape. Renderer code may identify only the existing machine-scoped remote session row; Rust owns mounted-surface verification and must not receive SSH details, paths, URLs, tokens, commands, output, or terminal content.

    CDXC:StatusPet 2026-06-26-04:38:
    Status indicator and pet overlay updates are fixed sidebar-only functions on this namespace. They may carry bounded enum/count/boolean/size/pet-id/project-id/session-id/order/title fields for GPUI-owned presentation and click routing, but no generic native-event bus, menu-bar status-item emulation, paths, URLs, commands, stdout/stderr, tokens, or terminal content.
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

pub(crate) fn update_sidebar_runtime_settings_v8_bridge(
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

pub(crate) fn update_sidebar_gxserver_bootstrap_v8_bridge(
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

    /*
    CDXC:ServerDaemon 2026-06-24-11:17:
    Post-load gxserver bootstrap refresh is a narrow sidebar bridge update, not a generic host event bus or JavaScript injection channel. It can replace only `window.ghostexGpui.gxserverBootstrap` and call the fixed optional `onGxserverBootstrapChanged(bootstrap)` callback, so token availability changes reach the React runtime while Browser/workarea/modal CEF clients remain outside the token path.
    */
    let Some(bootstrap_object) =
        install_sidebar_gxserver_bootstrap_v8_object(namespace, gxserver_bootstrap)
    else {
        return;
    };
    notify_sidebar_gxserver_bootstrap_changed(context, namespace, bootstrap_object);
}

pub(crate) fn install_session_chat_activation_v8_bridge(
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

pub(crate) fn install_session_chat_gxserver_bootstrap_v8_bridge(
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

pub(crate) fn install_project_workarea_v8_bridge(
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
        set_v8_string_property(
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

pub(crate) fn install_app_modal_host_v8_bridge(
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

pub(crate) fn install_extension_v8_bridge(context: Option<&mut cef::V8Context>) {
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

pub(crate) fn v8_object_property_or_new(parent: &V8Value, key: &str) -> Option<V8Value> {
    let key = CefString::from(key);
    parent
        .value_bykey(Some(&key))
        .filter(|value| value.is_object() != 0)
        .or_else(|| cef::v8_value_create_object(None, None))
}

pub(crate) fn set_v8_string_property(parent: &V8Value, key: &str, value: &str) -> bool {
    let key = CefString::from(key);
    let value = CefString::from(value);
    let Some(mut value) = cef::v8_value_create_string(Some(&value)) else {
        return false;
    };
    parent.set_value_bykey(Some(&key), Some(&mut value), V8Propertyattribute::default()) != 0
}

pub(crate) fn app_modal_host_payload_from_v8_value(value: &V8Value) -> Option<String> {
    if value.is_string() != 0 {
        return Some(CefString::from(&value.string_value()).to_string());
    }

    let Some(context) = cef::v8_context_get_current_context() else {
        return None;
    };
    let Some(global) = context.global() else {
        return None;
    };
    let json_key = CefString::from("JSON");
    let mut json = global
        .value_bykey(Some(&json_key))
        .filter(|value| value.is_object() != 0)?;
    let stringify_key = CefString::from("stringify");
    let stringify = json
        .value_bykey(Some(&stringify_key))
        .filter(|value| value.is_function() != 0)?;
    let argument = value.clone();
    let result = stringify.execute_function(Some(&mut json), Some(&[Some(argument)]))?;
    if result.is_string() == 0 {
        return None;
    }
    Some(CefString::from(&result.string_value()).to_string())
}
