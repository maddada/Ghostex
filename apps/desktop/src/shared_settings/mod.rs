//! The shared sidebar settings file as the desktop reads and writes it: defaults, the typed settings
//! values, the snapshot readers, the writer of Ghostex's own terminal Ghostty config and its one-time
//! migration out of the Ghostty app's config, the settings service and the field
//! normalizers.

/*
CDXC:Settings 2026-06-24-10:50:
GPUI must read and persist the shared sidebar settings JSON through the central XDG/GHOSTEX_HOME path resolver. Keep this module as the single GPUI path/read/write contract so Settings UI parity handles `updateSettings` without introducing a second settings store.

CDXC:Settings 2026-06-24-10:50:
Rust should parse only the GPUI runtime fields it consumes today: debuggingMode, the experimental switches (keepAwakeEnabled, allAutomationsEnabled), sidebarDefaultWidthPx, projectSwitchKeepAliveMinutes, project-editor auto-sleep fields, legacy external-IDE command fields, and the supported embedded Ghostty surface font-size field. The raw JSON object is preserved for whole-object writes, but this service intentionally does not duplicate the full TypeScript `ghostexSettings` schema.

CDXC:Settings 2026-06-24-10:50:
GPUI `updateSettings` handling needs a production write path: accept only JSON object payloads, create the shared state directory, write through an adjacent temp file then rename, skip byte-identical writes, and maintain a monotonic in-memory revision/hash/snapshot signal without logging paths, project names, URLs, commands, environment values, tokens, stdout/stderr, or user-owned content.

CDXC:Settings 2026-06-24-11:14:
The real React app-modal host now saves through GPUI, so the service exposes immutable snapshot object reads and a central object write entrypoint. Keep validation at this boundary: only object-shaped Settings payloads may persist, and callers must use the returned snapshot for post-save hydration instead of re-reading the settings file ad hoc.

CDXC:Terminal 2026-06-27-10:10:
The GPUI surface FFI contract only accepts `terminalFontSize` through `ghostty_surface_config_s`; normalize it into `font_size` and keep every other terminal Settings key out of the surface request.

CDXC:Terminal 2026-06-27-10:10:
Font family, theme, cursor, scrollback, clipboard, and mouse settings are Ghostty config-file-backed for future or recreated surfaces. `terminalPastePreviewableImages` is runtime-only and must not be included in config-file change detection or config writes.

CDXC:Terminal 2026-06-27-10:22:
GPUI image paste preview is runtime-only app behavior and defaults on for parity with `ghostex-settings.ts`. Snapshot access must accept only strict JSON booleans for `terminalPastePreviewableImages`, with missing or malformed values resolving to true.

CDXC:AgentProviders 2026-06-24-11:39:
GPUI Settings matches macOS for gxserver-owned agent launch policy: `agentAcceptAllEnabled` and `defaultPromptAgentId` remain in shared Settings only as a synchronous render cache. Parse them with the same default/normalization semantics as the TypeScript settings schema so GPUI can compare saves and reconcile gxserver canonical responses without duplicating the full settings model.

*/

mod appearance;
pub use appearance::effective_content_color_scheme;
mod ghostty_themes;

pub(crate) mod defaults;
pub(crate) mod experimental_switches;
pub(crate) mod ghostty_config_file;
pub(crate) mod ghostty_config_migration;
pub(crate) mod ghostty_config_values;
pub(crate) mod normalize;
pub(crate) mod service;
pub(crate) mod snapshot;
pub(crate) mod terminal_layout;
pub(crate) mod types;
pub(crate) mod work_area_frame_default;

pub use defaults::*;
pub use ghostty_config_file::*;
pub use ghostty_config_values::*;
pub use normalize::*;
pub use service::*;
pub use snapshot::*;
pub use terminal_layout::*;
pub use types::*;
