use std::{
    env, fs,
    path::Path,
};

use crate::app::helpers::*;

#[cfg(target_os = "macos")]
pub(crate) fn gpui_macos_os_integration_bundle_info() -> Option<GpuiOSIntegrationBundleInfo> {
    let executable = env::current_exe().ok()?;
    let bundle_root = find_app_bundle_root(&executable)?;
    let info_plist = fs::read_to_string(bundle_root.join("Contents/Info.plist")).ok()?;
    let bundle_identifier = gpui_plist_string_value(&info_plist, "CFBundleIdentifier")?;
    Some(GpuiOSIntegrationBundleInfo {
        bundle_identifier,
        bundle_root,
        info_plist,
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_plist_string_value(plist: &str, key: &str) -> Option<String> {
    let key_marker = format!("<key>{key}</key>");
    let after_key = plist.split_once(&key_marker)?.1;
    let after_string = after_key.split_once("<string>")?.1;
    let value = after_string.split_once("</string>")?.0.trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_os_integration_has_editable_registration(info_plist: &str) -> bool {
    info_plist.contains("<key>CFBundleDocumentTypes</key>")
        && info_plist.contains("<key>CFBundleTypeRole</key>")
        && info_plist.contains("<string>Editor</string>")
        && (info_plist.contains("<string>*</string>")
            || info_plist.contains("<string>public.text</string>")
            || info_plist.contains("<string>public.source-code</string>"))
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_os_integration_has_script_registration(info_plist: &str) -> bool {
    info_plist.contains("<key>CFBundleDocumentTypes</key>")
        && info_plist.contains("<key>CFBundleTypeRole</key>")
        && info_plist.contains("<string>Shell</string>")
        && GPUI_OS_INTEGRATION_SCRIPT_EXTENSIONS
            .iter()
            .all(|file_extension| {
                info_plist.contains(&format!("<string>{file_extension}</string>"))
            })
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_os_integration_has_ghostex_url_registration(info_plist: &str) -> bool {
    info_plist.contains("<key>CFBundleURLTypes</key>")
        && info_plist.contains("<key>CFBundleURLSchemes</key>")
        && info_plist.contains("<string>ghostex</string>")
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_macos_default_role_handlers(extensions: &[&str], role: u32) -> serde_json::Value {
    let handlers = extensions
        .iter()
        .filter_map(|file_extension| {
            let content_type = gpui_macos_content_type_for_extension(file_extension)?;
            let handler =
                unsafe { LSCopyDefaultRoleHandlerForContentType(content_type.as_ref(), role) };
            let handler = gpui_cf_string_to_string_and_release(handler)?;
            Some((
                (*file_extension).to_string(),
                serde_json::Value::String(handler),
            ))
        })
        .collect::<serde_json::Map<String, serde_json::Value>>();
    serde_json::Value::Object(handlers)
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_macos_default_url_scheme_handler(scheme: &str) -> Option<String> {
    let scheme = GpuiCfString::new(scheme)?;
    let handler = unsafe { LSCopyDefaultHandlerForURLScheme(scheme.as_ref()) };
    gpui_cf_string_to_string_and_release(handler)
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_macos_register_os_integration_bundle(bundle_root: &Path) -> Option<i32> {
    let path = GpuiCfString::new(&bundle_root.to_string_lossy())?;
    let url = unsafe {
        CFURLCreateWithFileSystemPath(
            std::ptr::null(),
            path.as_ref(),
            K_CF_URL_POSIX_PATH_STYLE,
            1,
        )
    };
    if url.is_null() {
        return None;
    }
    let status = unsafe { LSRegisterURL(url, 1) };
    unsafe {
        CFRelease(url);
    }
    Some(status)
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_macos_content_type_for_extension(file_extension: &str) -> Option<GpuiCfString> {
    let tag = GpuiCfString::new(file_extension)?;
    let content_type = unsafe {
        UTTypeCreatePreferredIdentifierForTag(
            kUTTagClassFilenameExtension,
            tag.as_ref(),
            std::ptr::null(),
        )
    };
    GpuiCfString::from_owned(content_type)
}

#[cfg(target_os = "macos")]
pub(crate) struct GpuiCfString(CFStringRef);

#[cfg(target_os = "macos")]
impl GpuiCfString {
    pub(crate) fn new(value: &str) -> Option<Self> {
        let c_value = std::ffi::CString::new(value).ok()?;
        let string = unsafe {
            CFStringCreateWithCString(
                std::ptr::null(),
                c_value.as_ptr(),
                K_CF_STRING_ENCODING_UTF8,
            )
        };
        Self::from_owned(string)
    }

    pub(crate) fn from_owned(value: CFStringRef) -> Option<Self> {
        (!value.is_null()).then_some(Self(value))
    }

    pub(crate) fn as_ref(&self) -> CFStringRef {
        self.0
    }
}

#[cfg(target_os = "macos")]
impl Drop for GpuiCfString {
    fn drop(&mut self) {
        unsafe {
            CFRelease(self.0);
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_cf_string_to_string_and_release(value: CFStringRef) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let converted = gpui_cf_string_to_string(value);
    unsafe {
        CFRelease(value);
    }
    converted
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_cf_string_to_string(value: CFStringRef) -> Option<String> {
    let direct = unsafe { CFStringGetCStringPtr(value, K_CF_STRING_ENCODING_UTF8) };
    if !direct.is_null() {
        return unsafe { std::ffi::CStr::from_ptr(direct) }
            .to_str()
            .ok()
            .map(str::to_string);
    }

    let length = unsafe { CFStringGetLength(value) };
    let max_size = unsafe { CFStringGetMaximumSizeForEncoding(length, K_CF_STRING_ENCODING_UTF8) };
    if max_size < 0 {
        return None;
    }
    let mut buffer = vec![0i8; (max_size as usize).saturating_add(1)];
    let ok = unsafe {
        CFStringGetCString(
            value,
            buffer.as_mut_ptr(),
            buffer.len() as isize,
            K_CF_STRING_ENCODING_UTF8,
        )
    } != 0;
    if !ok {
        return None;
    }
    unsafe { std::ffi::CStr::from_ptr(buffer.as_ptr()) }
        .to_str()
        .ok()
        .map(str::to_string)
}

#[cfg(target_os = "macos")]
type CFStringRef = *const std::ffi::c_void;
#[cfg(target_os = "macos")]
type CFURLRef = *const std::ffi::c_void;

#[cfg(target_os = "macos")]
pub(crate) const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
#[cfg(target_os = "macos")]
pub(crate) const K_CF_URL_POSIX_PATH_STYLE: isize = 0;
#[cfg(target_os = "macos")]
pub(crate) const K_LS_ROLES_EDITOR: u32 = 0x0000_0004;
#[cfg(target_os = "macos")]
pub(crate) const K_LS_ROLES_SHELL: u32 = 0x0000_0008;

#[cfg(target_os = "macos")]
unsafe extern "C" {
    static kUTTagClassFilenameExtension: CFStringRef;

    pub(crate) fn CFRelease(cf: *const std::ffi::c_void);
    pub(crate) fn CFStringCreateWithCString(
        allocator: *const std::ffi::c_void,
        c_str: *const std::ffi::c_char,
        encoding: u32,
    ) -> CFStringRef;
    pub(crate) fn CFStringGetCStringPtr(the_string: CFStringRef, encoding: u32) -> *const std::ffi::c_char;
    pub(crate) fn CFStringGetCString(
        the_string: CFStringRef,
        buffer: *mut std::ffi::c_char,
        buffer_size: isize,
        encoding: u32,
    ) -> u8;
    pub(crate) fn CFStringGetLength(the_string: CFStringRef) -> isize;
    pub(crate) fn CFStringGetMaximumSizeForEncoding(length: isize, encoding: u32) -> isize;
    pub(crate) fn CFURLCreateWithFileSystemPath(
        allocator: *const std::ffi::c_void,
        file_path: CFStringRef,
        path_style: isize,
        is_directory: u8,
    ) -> CFURLRef;
    pub(crate) fn UTTypeCreatePreferredIdentifierForTag(
        tag_class: CFStringRef,
        tag: CFStringRef,
        conforming_to_uti: CFStringRef,
    ) -> CFStringRef;
    pub(crate) fn LSCopyDefaultRoleHandlerForContentType(content_type: CFStringRef, role: u32) -> CFStringRef;
    pub(crate) fn LSSetDefaultRoleHandlerForContentType(
        content_type: CFStringRef,
        role: u32,
        handler_bundle_id: CFStringRef,
    ) -> i32;
    pub(crate) fn LSCopyDefaultHandlerForURLScheme(url_scheme: CFStringRef) -> CFStringRef;
    pub(crate) fn LSSetDefaultHandlerForURLScheme(
        url_scheme: CFStringRef,
        handler_bundle_id: CFStringRef,
    ) -> i32;
    pub(crate) fn LSRegisterURL(url: CFURLRef, update: u8) -> i32;
}

