use super::*;

fn installed_trust_state() -> dam_trust::TrustState {
    dam_trust::TrustState {
        mode: dam_trust::TrustMode::LocalCa,
        local_ca: Some(dam_trust::LocalCaRecord {
            id: "dam-local-ca-test".to_string(),
            label: "DAM Local CA".to_string(),
            fingerprint_sha256: "a".repeat(64),
            fingerprint_sha1: Some("b".repeat(40)),
            created_at_unix: 1,
            installed_at_unix: Some(2),
        }),
        ..dam_trust::TrustState::default()
    }
}

#[test]
fn explicit_proxy_can_activate_interception_for_configured_clients() {
    let readiness = readiness_for_default_routes(
        dam_net::CaptureMode::ExplicitProxy,
        false,
        false,
        &installed_trust_state(),
        true,
        TlsInterceptionAdapter::new(true),
    );

    assert!(
        readiness
            .iter()
            .all(|route| route.readiness == TlsInterceptionReadiness::Ready)
    );
    assert!(
        TlsInterceptionAdapter::new(true)
            .activate(&readiness[0])
            .is_ok()
    );
}

#[test]
fn routing_is_required_before_trust_or_adapter_activation() {
    let readiness = readiness_for_default_routes(
        dam_net::CaptureMode::SystemProxy,
        false,
        false,
        &installed_trust_state(),
        true,
        TlsInterceptionAdapter::new(true),
    );

    assert!(
        readiness
            .iter()
            .all(|route| route.readiness == TlsInterceptionReadiness::NeedsRouting)
    );
}

#[test]
fn consent_and_trust_gate_adapter_activation_after_routing() {
    let no_consent = readiness_for_default_routes(
        dam_net::CaptureMode::SystemProxy,
        true,
        false,
        &installed_trust_state(),
        false,
        TlsInterceptionAdapter::new(true),
    );
    let no_trust = readiness_for_default_routes(
        dam_net::CaptureMode::SystemProxy,
        true,
        false,
        &dam_trust::TrustState {
            mode: dam_trust::TrustMode::LocalCa,
            ..dam_trust::TrustState::default()
        },
        true,
        TlsInterceptionAdapter::new(true),
    );

    assert_eq!(
        no_consent[0].readiness,
        TlsInterceptionReadiness::NeedsUserConsent
    );
    assert_eq!(no_trust[0].readiness, TlsInterceptionReadiness::NeedsTrust);
}

#[test]
fn adapter_only_activates_when_every_gate_is_ready() {
    let adapter = TlsInterceptionAdapter::new(true);
    let readiness = readiness_for_default_routes(
        dam_net::CaptureMode::SystemProxy,
        true,
        false,
        &installed_trust_state(),
        true,
        adapter,
    );

    assert!(
        readiness
            .iter()
            .all(|route| route.readiness == TlsInterceptionReadiness::Ready)
    );
    let activation = adapter.activate(&readiness[0]).unwrap();
    assert_eq!(activation.state, TlsInterceptionActivationState::Active);
}

#[test]
fn unavailable_adapter_stays_inactive_even_after_prerequisites() {
    let readiness = readiness_for_default_routes(
        dam_net::CaptureMode::Tun,
        false,
        true,
        &installed_trust_state(),
        true,
        TlsInterceptionAdapter::unavailable(),
    );

    assert_eq!(
        readiness[0].readiness,
        TlsInterceptionReadiness::NeedsAdapter
    );
}

#[test]
fn unavailable_adapter_handle_cannot_activate_stale_ready_readiness() {
    let ready_adapter = TlsInterceptionAdapter::new(true);
    let readiness = readiness_for_default_routes(
        dam_net::CaptureMode::SystemProxy,
        true,
        false,
        &installed_trust_state(),
        true,
        ready_adapter,
    );

    assert_eq!(readiness[0].readiness, TlsInterceptionReadiness::Ready);
    assert!(
        TlsInterceptionAdapter::unavailable()
            .activate(&readiness[0])
            .is_err()
    );
}
