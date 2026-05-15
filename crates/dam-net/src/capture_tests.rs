use super::*;

#[test]
fn backend_kind_maps_to_capture_modes() {
    assert_eq!(
        CaptureBackendKind::ExplicitProxy.capture_mode(),
        CaptureMode::ExplicitProxy
    );
    assert_eq!(
        CaptureBackendKind::SystemProxy.capture_mode(),
        CaptureMode::SystemProxy
    );
    assert_eq!(
        CaptureBackendKind::MacosNetworkExtension.capture_mode(),
        CaptureMode::Tun
    );
    assert_eq!(
        CaptureBackendKind::WindowsFilteringPlatform.capture_mode(),
        CaptureMode::Tun
    );
    assert_eq!(
        CaptureBackendKind::LinuxTransparentProxy.capture_mode(),
        CaptureMode::Tun
    );
}

#[test]
fn active_backend_status_uses_common_ready_shape() {
    let status = CaptureBackendStatus::installed_active(
        CaptureBackendKind::MacosNetworkExtension,
        CapturePlatform::Macos,
        "active",
    );

    assert_eq!(status.mode, CaptureMode::Tun);
    assert_eq!(status.readiness, CaptureBackendReadiness::Ready);
    assert!(status.installed);
    assert!(status.active);
    assert!(status.rollback_available);
}
