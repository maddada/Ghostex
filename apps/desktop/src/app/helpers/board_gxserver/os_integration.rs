// C1 wave-1 deferred split: apps/desktop/src/app/helpers/board_gxserver.rs
// (~4.3k lines) further divided into responsibility-scoped submodules (pure
// move, no logic changes). This file holds the OS integration (default app/file-association) status and
// defaults-setting helpers.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::path::PathBuf;

use crate::app::helpers::*;

pub(crate) const GPUI_OS_INTEGRATION_EDITOR_EXTENSIONS: &[&str] = &[
    "txt", "md", "markdown", "json", "jsonc", "yaml", "yml", "toml", "ini", "env", "xml", "csv",
    "html", "css", "scss", "js", "jsx", "ts", "tsx", "sh", "bash", "zsh", "fish", "py", "rb", "go",
    "rs", "swift", "java", "kt", "c", "h", "cpp", "hpp", "cs", "php", "lua", "sql",
];
pub(crate) const GPUI_OS_INTEGRATION_STATUS_EDITOR_EXTENSIONS: &[&str] =
    &["txt", "md", "json", "js", "ts", "sh"];
pub(crate) const GPUI_OS_INTEGRATION_SCRIPT_EXTENSIONS: &[&str] = &["command", "tool", "sh"];

pub(crate) fn gpui_set_os_integration_defaults_status_message(
    target: Option<&str>,
) -> serde_json::Value {
    let status_items = gpui_set_os_integration_defaults(target);
    let mut payload = gpui_os_integration_status_message();
    gpui_merge_os_integration_status_items(&mut payload, status_items);
    payload
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_set_os_integration_defaults(target: Option<&str>) -> Vec<serde_json::Value> {
    /*
    CDXC:GPUIOSIntegration 2026-06-24-15:02:
    GPUI Settings may mutate Launch Services defaults only from explicit OS Integration button clicks. Status refreshes and startup must stay read-only, while this path targets only the requested editor, ghostex:// terminal-link, script-runner, or all roles.

    CDXC:GPUIOSIntegration 2026-06-24-15:10:
    Default mutations must return privacy-safe status items for the reused Settings UI. Capture per-extension and per-scheme Launch Services failures as enum reasons only; never expose bundle paths, file paths, URLs, command text, environment values, stdout/stderr, daemon bodies, or raw OSStatus values.
    */
    let mut status_items = Vec::new();
    let Some(target) = target else {
        status_items.push(gpui_os_integration_status_item(
            "platform",
            "setDefault",
            "skipped",
            "invalidTarget",
            None,
            None,
        ));
        return status_items;
    };
    if !matches!(target, "editor" | "terminalLinks" | "scriptRunner" | "all") {
        status_items.push(gpui_os_integration_status_item(
            "platform",
            "setDefault",
            "skipped",
            "invalidTarget",
            None,
            None,
        ));
        return status_items;
    }
    let Some(bundle) = gpui_macos_os_integration_bundle_info() else {
        status_items.push(gpui_os_integration_status_item(
            "bundleRegistration",
            "setDefault",
            "failed",
            "bundleIdentifierMissing",
            None,
            None,
        ));
        return status_items;
    };

    match gpui_macos_register_os_integration_bundle(&bundle.bundle_root) {
        Some(status) if status == 0 => {}
        _ => status_items.push(gpui_os_integration_status_item(
            "bundleRegistration",
            "registerBundle",
            "failed",
            "bundleRegistrationFailed",
            None,
            None,
        )),
    }
    if target == "editor" || target == "all" {
        for file_extension in GPUI_OS_INTEGRATION_EDITOR_EXTENSIONS {
            let Some(content_type) = gpui_macos_content_type_for_extension(file_extension) else {
                status_items.push(gpui_os_integration_status_item(
                    "editor",
                    "setDefault",
                    "skipped",
                    "contentTypeUnavailable",
                    Some(file_extension),
                    None,
                ));
                continue;
            };
            let Some(bundle_identifier) = GpuiCfString::new(&bundle.bundle_identifier) else {
                status_items.push(gpui_os_integration_status_item(
                    "editor",
                    "setDefault",
                    "failed",
                    "bundleIdentifierMissing",
                    Some(file_extension),
                    None,
                ));
                continue;
            };
            let status = unsafe {
                LSSetDefaultRoleHandlerForContentType(
                    content_type.as_ref(),
                    K_LS_ROLES_EDITOR,
                    bundle_identifier.as_ref(),
                )
            };
            if status != 0 {
                status_items.push(gpui_os_integration_status_item(
                    "editor",
                    "setDefault",
                    "failed",
                    "launchServicesRejected",
                    Some(file_extension),
                    None,
                ));
            }
        }
    }
    if target == "terminalLinks" || target == "all" {
        if let (Some(scheme), Some(bundle_identifier)) = (
            GpuiCfString::new("ghostex"),
            GpuiCfString::new(&bundle.bundle_identifier),
        ) {
            let status = unsafe {
                LSSetDefaultHandlerForURLScheme(scheme.as_ref(), bundle_identifier.as_ref())
            };
            if status != 0 {
                status_items.push(gpui_os_integration_status_item(
                    "terminalLinks",
                    "setDefault",
                    "failed",
                    "launchServicesRejected",
                    None,
                    Some("ghostex"),
                ));
            }
        } else {
            status_items.push(gpui_os_integration_status_item(
                "terminalLinks",
                "setDefault",
                "failed",
                "bundleIdentifierMissing",
                None,
                Some("ghostex"),
            ));
        }
    }
    if target == "scriptRunner" || target == "all" {
        for file_extension in GPUI_OS_INTEGRATION_SCRIPT_EXTENSIONS {
            let Some(content_type) = gpui_macos_content_type_for_extension(file_extension) else {
                status_items.push(gpui_os_integration_status_item(
                    "scriptRunner",
                    "setDefault",
                    "skipped",
                    "contentTypeUnavailable",
                    Some(file_extension),
                    None,
                ));
                continue;
            };
            let Some(bundle_identifier) = GpuiCfString::new(&bundle.bundle_identifier) else {
                status_items.push(gpui_os_integration_status_item(
                    "scriptRunner",
                    "setDefault",
                    "failed",
                    "bundleIdentifierMissing",
                    Some(file_extension),
                    None,
                ));
                continue;
            };
            let status = unsafe {
                LSSetDefaultRoleHandlerForContentType(
                    content_type.as_ref(),
                    K_LS_ROLES_SHELL,
                    bundle_identifier.as_ref(),
                )
            };
            if status != 0 {
                status_items.push(gpui_os_integration_status_item(
                    "scriptRunner",
                    "setDefault",
                    "failed",
                    "launchServicesRejected",
                    Some(file_extension),
                    None,
                ));
            }
        }
    }
    status_items
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_set_os_integration_defaults(target: Option<&str>) -> Vec<serde_json::Value> {
    if matches!(
        target,
        Some("editor") | Some("terminalLinks") | Some("scriptRunner") | Some("all")
    ) {
        return Vec::new();
    }
    vec![gpui_os_integration_status_item(
        "platform",
        "setDefault",
        "skipped",
        "invalidTarget",
        None,
        None,
    )]
}

pub(crate) fn gpui_os_integration_status_message() -> serde_json::Value {
    gpui_os_integration_status_payload()
}

pub(crate) fn gpui_merge_os_integration_status_items(
    payload: &mut serde_json::Value,
    status_items: Vec<serde_json::Value>,
) {
    if status_items.is_empty() {
        return;
    }
    let Some(object) = payload.as_object_mut() else {
        return;
    };
    match object.get_mut("statusItems") {
        Some(serde_json::Value::Array(existing_items)) => {
            existing_items.extend(status_items);
        }
        _ => {
            object.insert(
                "statusItems".to_string(),
                serde_json::Value::Array(status_items),
            );
        }
    }
}

pub(crate) fn gpui_os_integration_status_item(
    target: &str,
    operation: &str,
    status: &str,
    reason: &str,
    file_extension: Option<&str>,
    scheme: Option<&str>,
) -> serde_json::Value {
    let mut item = serde_json::Map::new();
    item.insert(
        "operation".to_string(),
        serde_json::Value::String(operation.to_string()),
    );
    item.insert(
        "reason".to_string(),
        serde_json::Value::String(reason.to_string()),
    );
    item.insert(
        "status".to_string(),
        serde_json::Value::String(status.to_string()),
    );
    item.insert(
        "target".to_string(),
        serde_json::Value::String(target.to_string()),
    );
    if let Some(file_extension) = file_extension {
        item.insert(
            "extension".to_string(),
            serde_json::Value::String(file_extension.to_string()),
        );
    }
    if let Some(scheme) = scheme {
        item.insert(
            "scheme".to_string(),
            serde_json::Value::String(scheme.to_string()),
        );
    }
    serde_json::Value::Object(item)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_os_integration_status_payload() -> serde_json::Value {
    /*
    CDXC:GPUIOSIntegration 2026-06-24-15:02:
    Non-macOS builds cannot inspect or mutate macOS Launch Services. Keep the shared status payload honest with unavailable registrations and no default handlers instead of inventing platform parity.
    */
    serde_json::json!({
        "bundleIdentifier": "com.madda.ghostex.gpui-unavailable",
        "editorDefaults": {},
        "generatedAt": gpui_status_generated_at(),
        "registeredEditableFiles": false,
        "registeredGhostexURLScheme": false,
        "registeredScriptRunner": false,
        "scriptDefaults": {},
        "statusItems": [
            gpui_os_integration_status_item(
                "platform",
                "readStatus",
                "unsupported",
                "unsupportedPlatform",
                None,
                None,
            )
        ],
        "terminalLinkDefaultBundleId": "GPUI Launch Services bridge unavailable",
        "type": "osIntegrationStatus",
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_os_integration_status_payload() -> serde_json::Value {
    /*
    CDXC:GPUIOSIntegration 2026-06-24-15:02:
    Settings OS integration status should mirror the Swift host payload: app bundle id, Launch Services defaults for representative editor/script extensions, ghostex:// default handler, and Info.plist registration booleans. This function is read-only and must not set defaults or register the app on status requests.
    */
    let bundle = gpui_macos_os_integration_bundle_info();
    let bundle_identifier = bundle
        .as_ref()
        .map(|info| info.bundle_identifier.as_str())
        .unwrap_or("com.madda.ghostex.gpui-unavailable");
    let info_plist = bundle
        .as_ref()
        .map(|info| info.info_plist.as_str())
        .unwrap_or("");
    let mut payload = serde_json::json!({
        "bundleIdentifier": bundle_identifier,
        "editorDefaults": gpui_macos_default_role_handlers(
            GPUI_OS_INTEGRATION_STATUS_EDITOR_EXTENSIONS,
            K_LS_ROLES_EDITOR,
        ),
        "generatedAt": gpui_status_generated_at(),
        "registeredEditableFiles": gpui_os_integration_has_editable_registration(info_plist),
        "registeredGhostexURLScheme": gpui_os_integration_has_ghostex_url_registration(info_plist),
        "registeredScriptRunner": gpui_os_integration_has_script_registration(info_plist),
        "scriptDefaults": gpui_macos_default_role_handlers(
            GPUI_OS_INTEGRATION_SCRIPT_EXTENSIONS,
            K_LS_ROLES_SHELL,
        ),
        "type": "osIntegrationStatus",
    });
    if bundle.is_none() {
        gpui_merge_os_integration_status_items(
            &mut payload,
            vec![gpui_os_integration_status_item(
                "bundleRegistration",
                "readStatus",
                "failed",
                "bundleIdentifierMissing",
                None,
                None,
            )],
        );
    }
    if let Some(handler) = gpui_macos_default_url_scheme_handler("ghostex") {
        if let Some(object) = payload.as_object_mut() {
            object.insert(
                "terminalLinkDefaultBundleId".to_string(),
                serde_json::Value::String(handler),
            );
        }
    }
    payload
}

#[cfg(target_os = "macos")]
pub(crate) struct GpuiOSIntegrationBundleInfo {
    pub(crate) bundle_identifier: String,
    pub(crate) bundle_root: PathBuf,
    pub(crate) info_plist: String,
}
