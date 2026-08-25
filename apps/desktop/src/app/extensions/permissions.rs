use super::{GpuiExtensionPermission, GpuiInstalledExtension};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GpuiExtensionPermissionError {
    pub(crate) permission: GpuiExtensionPermission,
}

pub(crate) fn require_extension_permission(
    extension: &GpuiInstalledExtension,
    permission: GpuiExtensionPermission,
) -> Result<(), GpuiExtensionPermissionError> {
    if extension.declared_permissions.contains(&permission)
        && extension.granted_permissions.contains(&permission)
    {
        Ok(())
    } else {
        Err(GpuiExtensionPermissionError { permission })
    }
}

pub(crate) fn extension_permission_error_response(
    request_id: &str,
    extension_id: &str,
    method: &str,
    error: GpuiExtensionPermissionError,
) -> serde_json::Value {
    eprintln!(
        "Ghostex extension {extension_id:?} denied bridge method {method:?}: missing {} permission",
        error.permission.as_str()
    );
    serde_json::json!({
        "requestId": request_id,
        "ok": false,
        "error": {
            "code": "permissionDenied",
            "message": format!("This extension does not have the {} permission.", error.permission.as_str()),
            "permission": error.permission.as_str(),
        }
    })
}
