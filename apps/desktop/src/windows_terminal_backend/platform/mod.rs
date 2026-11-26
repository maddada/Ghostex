//! The Windows backend's WSL half: backend state, the ghostex CLI and gxserver startup, terminal and
//! resource invocations, WSL paths, distributions and processes, and the packaged gxserver and source
//! runtime installs.

// The moved bodies name the parent backend's items as `super::<item>`; importing them here keeps those
// paths resolving unchanged from the per-concern files below.
use super::{
    ResolvedWindowsTerminalBackend, WindowsTerminalBackendPreference, WindowsWslGhostexCliStatus,
    WindowsWslReadiness, WindowsWslSetupPhase, current_preference, native, native_package,
};

mod backend;
mod packaged_install;
mod settings_mirror;
mod wsl;

pub(super) use backend::*;
use packaged_install::*;
pub(super) use settings_mirror::*;
pub(super) use wsl::*;
