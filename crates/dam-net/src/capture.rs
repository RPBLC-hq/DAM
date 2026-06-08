use serde::{Deserialize, Serialize};

use crate::{CaptureMode, CaptureSupport};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapturePlatform {
    Macos,
    Windows,
    Linux,
    Unknown,
}

impl CapturePlatform {
    pub fn current() -> Self {
        #[cfg(target_os = "macos")]
        {
            Self::Macos
        }
        #[cfg(target_os = "windows")]
        {
            Self::Windows
        }
        #[cfg(target_os = "linux")]
        {
            Self::Linux
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            Self::Unknown
        }
    }

    pub fn tag(self) -> &'static str {
        match self {
            Self::Macos => "macos",
            Self::Windows => "windows",
            Self::Linux => "linux",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureBackendKind {
    ExplicitProxy,
    SystemProxy,
    MacosNetworkExtension,
    PacketTunnel,
    WindowsFilteringPlatform,
    LinuxTransparentProxy,
    Custom,
}

impl CaptureBackendKind {
    pub fn tag(self) -> &'static str {
        match self {
            Self::ExplicitProxy => "explicit_proxy",
            Self::SystemProxy => "system_proxy",
            Self::MacosNetworkExtension => "macos_network_extension",
            Self::PacketTunnel => "packet_tunnel",
            Self::WindowsFilteringPlatform => "windows_filtering_platform",
            Self::LinuxTransparentProxy => "linux_transparent_proxy",
            Self::Custom => "custom",
        }
    }

    pub fn capture_mode(self) -> CaptureMode {
        match self {
            Self::ExplicitProxy => CaptureMode::ExplicitProxy,
            Self::SystemProxy => CaptureMode::SystemProxy,
            Self::MacosNetworkExtension
            | Self::PacketTunnel
            | Self::WindowsFilteringPlatform
            | Self::LinuxTransparentProxy
            | Self::Custom => CaptureMode::Tun,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureBackendOperation {
    PreviewInstall,
    Install,
    Remove,
    Status,
    StartCapture,
    StopCapture,
}

impl CaptureBackendOperation {
    pub fn tag(self) -> &'static str {
        match self {
            Self::PreviewInstall => "preview_install",
            Self::Install => "install",
            Self::Remove => "remove",
            Self::Status => "status",
            Self::StartCapture => "start_capture",
            Self::StopCapture => "stop_capture",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureBackendReadiness {
    Inactive,
    NeedsInstall,
    NeedsApproval,
    Ready,
    Error,
}

impl CaptureBackendReadiness {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Inactive => "inactive",
            Self::NeedsInstall => "needs_install",
            Self::NeedsApproval => "needs_approval",
            Self::Ready => "ready",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureBackendStatus {
    pub kind: CaptureBackendKind,
    pub platform: CapturePlatform,
    pub mode: CaptureMode,
    pub support: CaptureSupport,
    pub installed: bool,
    pub active: bool,
    pub requires_admin: bool,
    pub changes_system_routes: bool,
    pub rollback_available: bool,
    pub readiness: CaptureBackendReadiness,
    pub message: String,
}

impl CaptureBackendStatus {
    pub fn inactive(
        kind: CaptureBackendKind,
        platform: CapturePlatform,
        support: CaptureSupport,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            platform,
            mode: kind.capture_mode(),
            support,
            installed: false,
            active: false,
            requires_admin: false,
            changes_system_routes: false,
            rollback_available: false,
            readiness: CaptureBackendReadiness::Inactive,
            message: message.into(),
        }
    }

    pub fn installed_active(
        kind: CaptureBackendKind,
        platform: CapturePlatform,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            platform,
            mode: kind.capture_mode(),
            support: CaptureSupport::Implemented,
            installed: true,
            active: true,
            requires_admin: true,
            changes_system_routes: true,
            rollback_available: true,
            readiness: CaptureBackendReadiness::Ready,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureBackendPlan {
    pub operation: CaptureBackendOperation,
    pub status: CaptureBackendStatus,
    pub commands: Vec<Vec<String>>,
}

#[cfg(test)]
#[path = "capture_tests.rs"]
mod tests;
