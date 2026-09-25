use super::*;

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
    Helper-backed titlebar renderers must install the same `ghostexNativeHost`
    bridge as the shell CEF path (apps/desktop/src/cef/shell.rs); the titlebar
    uses it for Resources actions. (The sidebar page that also used it was
    deleted on 2026-09-21.)
    */
    if surface == AppModalHostBridgeSurface::Titlebar {
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
