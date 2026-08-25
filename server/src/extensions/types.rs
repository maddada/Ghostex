use std::{collections::BTreeMap, path::Path};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionManifest {
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub name: String,
    pub title: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub icon: String,
    pub categories: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preferences: Vec<ExtensionPreference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permissions: Vec<ExtensionPermission>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub placements: Vec<ExtensionPlacement>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_placement: Option<ExtensionPlacement>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<ExtensionServer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<ExtensionKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal: Option<ExtensionTerminal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modal: Option<ExtensionSize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub popup: Option<ExtensionSize>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExtensionPlacement {
    View,
    ChatBar,
    Popup,
    Modal,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtensionPermission {
    Exec,
    Cli,
    Ssh,
    Network,
    Clipboard,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExtensionKind {
    TerminalPane,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionServer {
    #[serde(rename = "static", skip_serializing_if = "Option::is_none")]
    pub static_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readiness: Option<ExtensionReadiness>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub install: BTreeMap<String, ExtensionPlatformInstall>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionReadiness {
    pub http_get: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionPlatformInstall {
    pub url: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionTerminal {
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionPreference {
    pub name: String,
    pub title: String,
    pub description: String,
    #[serde(rename = "type")]
    pub preference_type: ExtensionPreferenceType,
    #[serde(default)]
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data: Vec<ExtensionPreferenceOption>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtensionPreferenceType {
    Textfield,
    Password,
    Checkbox,
    Dropdown,
    File,
    Directory,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExtensionPreferenceOption {
    pub title: String,
    pub value: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct ExtensionSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionStoreEntry {
    pub enabled: bool,
    pub pinned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement: Option<ExtensionPlacement>,
    pub terminal_placement: ExtensionTerminalPlacement,
    #[serde(default)]
    pub preferences: BTreeMap<String, Value>,
    pub version: String,
    #[serde(default)]
    pub granted_permissions: Vec<ExtensionPermission>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ExtensionTerminalPlacement {
    SplitRight,
    Tab,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionStatePatch {
    pub enabled: Option<bool>,
    pub pinned: Option<bool>,
    pub placement: Option<ExtensionPlacement>,
    pub terminal_placement: Option<ExtensionTerminalPlacement>,
    pub preferences: Option<BTreeMap<String, Value>>,
    pub granted_permissions: Option<Vec<ExtensionPermission>>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledExtension {
    pub id: String,
    pub manifest: ExtensionManifest,
    pub state: ExtensionStoreEntry,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionCatalog {
    pub schema_version: u64,
    pub published_at: String,
    pub extensions: Vec<ExtensionCatalogEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExtensionCatalogEntry {
    #[serde(flatten)]
    pub manifest: ExtensionManifest,
    pub readme: String,
    pub changelog: String,
    #[serde(default)]
    pub screenshots: Vec<String>,
    pub zip: String,
    pub sha256: String,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtensionCatalogSource {
    Remote,
    Cache,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionCatalogSnapshot {
    pub catalog: ExtensionCatalog,
    pub source: ExtensionCatalogSource,
    pub url: String,
}

#[derive(Debug)]
pub struct ExtensionError {
    pub code: &'static str,
    pub message: String,
}

impl ExtensionError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            code: "badRequest",
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            code: "notFound",
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            code: "internalError",
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ExtensionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ExtensionError {}

pub type ExtensionResult<T> = Result<T, ExtensionError>;

impl ExtensionManifest {
    pub fn validate(
        &self,
        payload_dir: Option<&Path>,
        expected_id: Option<&str>,
    ) -> ExtensionResult<()> {
        if !valid_extension_id(&self.name) {
            return Err(ExtensionError::bad_request(format!(
                "Extension name {:?} must be a kebab-case id.",
                self.name
            )));
        }
        if expected_id.is_some_and(|expected| expected != self.name) {
            return Err(ExtensionError::bad_request(format!(
                "Manifest name {:?} does not match extension id {:?}.",
                self.name,
                expected_id.unwrap_or_default()
            )));
        }
        for (field, value) in [
            ("title", self.title.as_str()),
            ("description", self.description.as_str()),
            ("author", self.author.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(ExtensionError::bad_request(format!(
                    "Extension {} must not be empty.",
                    field
                )));
            }
        }
        if !valid_semver(&self.version) {
            return Err(ExtensionError::bad_request(format!(
                "Extension version {:?} is not valid semantic versioning.",
                self.version
            )));
        }
        if self.categories.is_empty() || self.categories.iter().any(|value| value.trim().is_empty())
        {
            return Err(ExtensionError::bad_request(
                "Extension categories must contain at least one non-empty value.",
            ));
        }
        validate_relative_path(&self.icon, "icon")?;
        if !self.icon.to_ascii_lowercase().ends_with(".svg") {
            return Err(ExtensionError::bad_request(
                "Extension icon must reference an SVG file.",
            ));
        }
        self.validate_shape()?;
        self.validate_preferences()?;
        if let Some(payload_dir) = payload_dir {
            let icon_path = payload_dir.join(&self.icon);
            if !icon_path.is_file() {
                return Err(ExtensionError::bad_request(format!(
                    "Extension icon does not exist: {}",
                    icon_path.display()
                )));
            }
            if let Some(static_dir) = self
                .server
                .as_ref()
                .and_then(|server| server.static_dir.as_ref())
            {
                let static_path = payload_dir.join(static_dir);
                if !static_path.is_dir() {
                    return Err(ExtensionError::bad_request(format!(
                        "Extension static directory does not exist: {}",
                        static_path.display()
                    )));
                }
            }
        }
        Ok(())
    }

    fn validate_shape(&self) -> ExtensionResult<()> {
        match self.kind {
            Some(ExtensionKind::TerminalPane) => {
                if self.terminal.is_none()
                    || self.server.is_some()
                    || !self.placements.is_empty()
                    || self.default_placement.is_some()
                {
                    return Err(ExtensionError::bad_request(
                        "A terminal-pane extension requires terminal and cannot declare web placements or server.",
                    ));
                }
                let terminal = self.terminal.as_ref().expect("checked terminal");
                if terminal.command.trim().is_empty() {
                    return Err(ExtensionError::bad_request(
                        "A terminal-pane extension requires a non-empty command.",
                    ));
                }
            }
            None => {
                if self.terminal.is_some() || self.placements.is_empty() {
                    return Err(ExtensionError::bad_request(
                        "A web extension requires at least one placement and cannot declare terminal.",
                    ));
                }
                let Some(default_placement) = self.default_placement else {
                    return Err(ExtensionError::bad_request(
                        "A web extension requires defaultPlacement.",
                    ));
                };
                if !self.placements.contains(&default_placement) {
                    return Err(ExtensionError::bad_request(
                        "defaultPlacement must occur in placements.",
                    ));
                }
                let Some(server) = &self.server else {
                    return Err(ExtensionError::bad_request(
                        "A web extension requires server.",
                    ));
                };
                match (&server.static_dir, &server.command) {
                    (Some(static_dir), None) => {
                        validate_relative_path(static_dir, "server.static")?;
                        if server.readiness.is_some()
                            || server.cwd.is_some()
                            || !server.install.is_empty()
                        {
                            return Err(ExtensionError::bad_request(
                                "A static server cannot declare command-server fields.",
                            ));
                        }
                    }
                    (None, Some(command)) if !command.trim().is_empty() => {
                        let readiness = server.readiness.as_ref().ok_or_else(|| {
                            ExtensionError::bad_request(
                                "A command server requires readiness.httpGet.",
                            )
                        })?;
                        if !readiness.http_get.starts_with('/') {
                            return Err(ExtensionError::bad_request(
                                "readiness.httpGet must start with '/'.",
                            ));
                        }
                        for install in server.install.values() {
                            validate_sha256(&install.sha256)?;
                            url::Url::parse(&install.url).map_err(|_| {
                                ExtensionError::bad_request(format!(
                                    "Invalid command-server install URL: {}",
                                    install.url
                                ))
                            })?;
                        }
                    }
                    _ => {
                        return Err(ExtensionError::bad_request(
                            "server must declare exactly one of static or command.",
                        ));
                    }
                }
            }
        }
        let unique_placements = self
            .placements
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        if unique_placements.len() != self.placements.len() {
            return Err(ExtensionError::bad_request(
                "Extension placements must be unique.",
            ));
        }
        let unique_permissions = self
            .permissions
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        if unique_permissions.len() != self.permissions.len() {
            return Err(ExtensionError::bad_request(
                "Extension permissions must be unique.",
            ));
        }
        if let Some(modal) = self.modal {
            if modal.width == 0 || modal.width > 1400 || modal.height == 0 || modal.height > 900 {
                return Err(ExtensionError::bad_request(
                    "Modal size must be within 1400x900.",
                ));
            }
        }
        if let Some(popup) = self.popup {
            if popup.width == 0 || popup.width > 420 || popup.height == 0 || popup.height > 640 {
                return Err(ExtensionError::bad_request(
                    "Popup size must be within 420x640.",
                ));
            }
        }
        Ok(())
    }

    fn validate_preferences(&self) -> ExtensionResult<()> {
        let mut names = std::collections::BTreeSet::new();
        for preference in &self.preferences {
            if !valid_preference_name(&preference.name) || !names.insert(&preference.name) {
                return Err(ExtensionError::bad_request(format!(
                    "Invalid or duplicate preference name: {}",
                    preference.name
                )));
            }
            if preference.title.trim().is_empty() || preference.description.trim().is_empty() {
                return Err(ExtensionError::bad_request(format!(
                    "Preference {} requires title and description.",
                    preference.name
                )));
            }
            if preference.default.as_ref().is_some_and(|value| {
                !value.is_string() && !value.is_boolean() && !value.is_number()
            }) {
                return Err(ExtensionError::bad_request(format!(
                    "Preference {} has an unsupported default value.",
                    preference.name
                )));
            }
            if preference.preference_type == ExtensionPreferenceType::Dropdown
                && preference.data.is_empty()
            {
                return Err(ExtensionError::bad_request(format!(
                    "Dropdown preference {} requires data options.",
                    preference.name
                )));
            }
        }
        Ok(())
    }
}

pub fn validate_extension_id(id: &str) -> ExtensionResult<()> {
    if valid_extension_id(id) {
        Ok(())
    } else {
        Err(ExtensionError::bad_request(format!(
            "Invalid extension id: {id:?}."
        )))
    }
}

pub fn validate_sha256(value: &str) -> ExtensionResult<()> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(ExtensionError::bad_request(
            "Expected SHA-256 must contain exactly 64 hexadecimal characters.",
        ))
    }
}

fn valid_extension_id(value: &str) -> bool {
    !value.is_empty()
        && value.split('-').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

fn valid_preference_name(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic())
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn valid_semver(value: &str) -> bool {
    let (without_build, build) = value
        .split_once('+')
        .map(|(version, build)| (version, Some(build)))
        .unwrap_or((value, None));
    if build.is_some_and(|build| !valid_semver_identifiers(build, false)) {
        return false;
    }
    let (core, prerelease) = without_build
        .split_once('-')
        .map(|(core, prerelease)| (core, Some(prerelease)))
        .unwrap_or((without_build, None));
    if prerelease.is_some_and(|prerelease| !valid_semver_identifiers(prerelease, true)) {
        return false;
    }
    let parts = core.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.bytes().all(|byte| byte.is_ascii_digit())
                && (part == &"0" || !part.starts_with('0'))
        })
}

fn valid_semver_identifiers(value: &str, reject_numeric_leading_zero: bool) -> bool {
    !value.is_empty()
        && value.split('.').all(|identifier| {
            !identifier.is_empty()
                && identifier
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                && (!reject_numeric_leading_zero
                    || !identifier.bytes().all(|byte| byte.is_ascii_digit())
                    || identifier == "0"
                    || !identifier.starts_with('0'))
        })
}

fn validate_relative_path(value: &str, field: &str) -> ExtensionResult<()> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(ExtensionError::bad_request(format!(
            "Extension {field} must stay inside the extension folder."
        )));
    }
    Ok(())
}
