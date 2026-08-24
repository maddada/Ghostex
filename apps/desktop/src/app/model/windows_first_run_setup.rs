// C1 wave-3 re-cluster: the Windows-specific first-run setup state and action types, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

#![allow(unused_imports)]

use crate::*;

#[cfg(target_os = "windows")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GpuiWindowsFirstRunSetupState {
    Checking,
    MissingWsl,
    MissingDistribution,
    ChooseDistribution(Vec<String>),
    ConfiguredDistributionUnavailable(String),
    SettingUp(windows_terminal_backend::WindowsWslSetupPhase),
    Failed(String),
    Ready,
}

#[cfg(target_os = "windows")]
#[derive(Clone, Debug)]
pub(crate) enum GpuiWindowsFirstRunSetupAction {
    Retry,
    OpenWslGuide,
    ChooseDistribution(String),
    ClearDistribution,
}
