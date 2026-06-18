use super::*;

fn https_openai_decision() -> dam_net::TransparentRouteDecision {
    dam_net::decide_transparent_route(&dam_net::TrafficObservation::new(
        "api.openai.com",
        dam_net::TrafficProtocol::Https,
    ))
}

#[test]
fn parses_trust_modes() {
    assert_eq!("off".parse::<TrustMode>().unwrap(), TrustMode::Disabled);
    assert_eq!("local-ca".parse::<TrustMode>().unwrap(), TrustMode::LocalCa);
}

#[test]
fn trust_action_plans_mark_supported_local_ca_platforms_as_implemented() {
    let inspect =
        TrustActionPlan::for_action(TrustAction::Inspect, PlatformTrustStore::MacosKeychain);
    let install = TrustActionPlan::for_action(
        TrustAction::InstallLocalCa,
        PlatformTrustStore::MacosKeychain,
    );
    let linux = TrustActionPlan::for_action(
        TrustAction::InstallLocalCa,
        PlatformTrustStore::LinuxNssOrSystemStore,
    );

    assert_eq!(inspect.support, TrustSupport::Implemented);
    #[cfg(target_os = "macos")]
    assert_eq!(install.support, TrustSupport::Implemented);
    #[cfg(not(target_os = "macos"))]
    assert_eq!(install.support, TrustSupport::Planned);
    #[cfg(target_os = "linux")]
    assert_eq!(linux.support, TrustSupport::Implemented);
    #[cfg(not(target_os = "linux"))]
    assert_eq!(linux.support, TrustSupport::Planned);
    assert!(install.requires_user_consent);
    assert!(install.rollback_required);
}

#[test]
fn default_trust_state_allows_default_traffic_hosts_but_is_disabled() {
    let state = TrustState::default();

    assert_eq!(state.mode, TrustMode::Disabled);
    assert!(state.host_allowed("https://api.openai.com/v1/responses"));
    assert!(!state.host_allowed("example.com"));
}

#[test]
fn https_profile_traffic_needs_trust_when_interception_is_disabled() {
    let report = readiness_for_route(&https_openai_decision(), &TrustState::default(), false);

    assert_eq!(report.readiness, TlsInterceptionReadiness::Disabled);
}

#[test]
fn local_ca_mode_requires_user_consent_before_ca_check() {
    let state = TrustState {
        mode: TrustMode::LocalCa,
        ..TrustState::default()
    };

    let report = readiness_for_route(&https_openai_decision(), &state, false);

    assert_eq!(report.readiness, TlsInterceptionReadiness::NeedsUserConsent);
}

#[test]
fn local_ca_mode_requires_installed_ca_after_user_consent() {
    let state = TrustState {
        mode: TrustMode::LocalCa,
        ..TrustState::default()
    };

    let report = readiness_for_route(&https_openai_decision(), &state, true);

    assert_eq!(report.readiness, TlsInterceptionReadiness::NeedsLocalCa);
}

#[test]
fn installed_local_ca_and_user_consent_make_default_tls_route_ready() {
    let state = TrustState {
        mode: TrustMode::LocalCa,
        local_ca: Some(LocalCaRecord {
            id: "dam-local-ca".to_string(),
            label: "DAM Local CA".to_string(),
            fingerprint_sha256: "abc123".to_string(),
            fingerprint_sha1: Some("def456".to_string()),
            created_at_unix: 1,
            installed_at_unix: Some(2),
        }),
        ..TrustState::default()
    };

    let report = readiness_for_route(&https_openai_decision(), &state, true);

    assert_eq!(report.readiness, TlsInterceptionReadiness::Ready);
}

#[test]
fn default_route_readiness_reports_all_initial_https_routes() {
    let routes = dam_net::default_traffic_routes();
    let reports = readiness_for_default_routes(&TrustState::default(), false);

    assert_eq!(reports.len(), routes.len());
    assert_eq!(reports[0].route.target_name, "openai");
    assert_eq!(reports[0].protocol, dam_net::TrafficProtocol::Https);
    assert!(
        reports
            .iter()
            .all(|report| report.readiness == TlsInterceptionReadiness::Disabled)
    );
}

#[test]
fn configured_route_readiness_uses_route_host_scope() {
    let routes = vec![dam_net::TrafficRoute::custom(
        "api.enterprise-ai.example",
        "openai-compatible",
        "enterprise-ai",
        "https://api.enterprise-ai.example",
    )];
    let trust = TrustState {
        mode: TrustMode::LocalCa,
        allowed_hosts: routes.iter().map(|route| route.host.clone()).collect(),
        ..TrustState::default()
    };

    let reports = readiness_for_routes(&routes, &trust, true);

    assert!(
        reports
            .iter()
            .any(|report| report.route.target_name == "enterprise-ai")
    );
}

#[test]
fn local_ca_artifact_generates_inspects_and_deletes_without_installing() {
    let dir = tempfile::tempdir().unwrap();

    let artifact = generate_local_ca_artifact_at(dir.path(), 1).unwrap();

    assert_eq!(artifact.record.label, LOCAL_CA_LABEL);
    assert_eq!(artifact.record.created_at_unix, 1);
    assert_eq!(artifact.record.installed_at_unix, None);
    assert_eq!(artifact.record.fingerprint_sha256.len(), 64);
    assert_eq!(artifact.record.fingerprint_sha1.as_ref().unwrap().len(), 40);
    assert!(artifact.paths.manifest_path.exists());
    assert!(artifact.paths.certificate_path.exists());
    assert!(artifact.paths.private_key_path.exists());

    let inspected = inspect_local_ca_artifact(dir.path()).unwrap().unwrap();
    assert_eq!(inspected.record, artifact.record);

    let state = trust_state_for_state_dir(TrustMode::LocalCa, dir.path()).unwrap();
    assert_eq!(state.local_ca, Some(artifact.record));
    assert!(!state.local_ca_installed());

    assert!(delete_local_ca_artifact(dir.path()).unwrap());
    assert!(inspect_local_ca_artifact(dir.path()).unwrap().is_none());
    assert!(!delete_local_ca_artifact(dir.path()).unwrap());
}

#[test]
#[cfg(target_os = "macos")]
fn local_ca_install_plan_previews_generation_and_system_command() {
    let dir = tempfile::tempdir().unwrap();

    let plan =
        local_ca_install_plan_for_platform(dir.path(), PlatformTrustStore::MacosKeychain).unwrap();

    assert_eq!(plan.action, TrustAction::InstallLocalCa);
    assert_eq!(plan.support, TrustSupport::Implemented);
    assert!(plan.will_generate_artifact);
    assert!(plan.can_execute);
    assert!(!plan.requires_admin);
    assert_eq!(plan.commands.len(), 1);
    assert_eq!(plan.commands[0].program, MACOS_SECURITY);
    assert!(
        plan.commands[0]
            .args
            .contains(&"add-trusted-cert".to_string())
    );
    assert!(!plan.commands[0].args.contains(&"-d".to_string()));
    assert!(
        plan.system_store
            .contains("Library/Keychains/login.keychain")
    );
}

#[cfg(target_os = "linux")]
#[test]
fn linux_local_ca_install_plan_previews_trust_anchor_command() {
    let dir = tempfile::tempdir().unwrap();

    let plan =
        local_ca_install_plan_for_platform(dir.path(), PlatformTrustStore::LinuxNssOrSystemStore)
            .unwrap();

    assert_eq!(plan.action, TrustAction::InstallLocalCa);
    assert_eq!(plan.support, TrustSupport::Implemented);
    assert!(plan.will_generate_artifact);
    assert!(plan.can_execute);
    assert!(plan.requires_admin);
    assert_eq!(plan.commands.len(), 1);
    assert_eq!(plan.commands[0].program, LINUX_SUDO);
    assert_eq!(plan.commands[0].args[0], LINUX_TRUST);
    assert_eq!(plan.commands[0].args[1], "anchor");
    assert_eq!(plan.commands[0].args[2], "--store");
    assert!(
        Path::new(&plan.commands[0].args[3])
            .ends_with(Path::new("trust").join("local-ca").join("ca.pem"))
    );
    assert_eq!(plan.system_store, "linux_nss_or_system_store");
}

#[test]
#[cfg(target_os = "macos")]
fn local_ca_remove_plan_uses_certificate_fingerprint() {
    let dir = tempfile::tempdir().unwrap();
    let artifact = generate_local_ca_artifact_at(dir.path(), 1).unwrap();
    mark_local_ca_installed_at(dir.path(), 2).unwrap();

    let plan =
        local_ca_remove_plan_for_platform(dir.path(), PlatformTrustStore::MacosKeychain).unwrap();

    assert_eq!(plan.action, TrustAction::RemoveLocalCa);
    assert_eq!(plan.support, TrustSupport::Implemented);
    assert!(plan.can_execute);
    assert_eq!(plan.commands.len(), 1);
    assert_eq!(plan.commands[0].program, MACOS_SECURITY);
    assert_eq!(plan.commands[0].args[0], "delete-certificate");
    assert_eq!(plan.commands[0].args[1], "-Z");
    assert_eq!(plan.commands[0].args[2].len(), 40);
    assert_eq!(
        plan.commands[0].args[2],
        artifact.record.fingerprint_sha1.unwrap()
    );
    assert!(plan.commands[0].args[3].contains("Library/Keychains/login.keychain"));
}

#[test]
fn macos_install_command_uses_user_login_keychain() {
    let command = macos_install_command(Path::new("/tmp/DAM's CA/ca.pem"));

    assert_eq!(command.program, MACOS_SECURITY);
    assert_eq!(command.args[0], "add-trusted-cert");
    assert!(!command.args.contains(&"-d".to_string()));
    assert!(command.args.contains(&"-k".to_string()));
    assert!(
        command
            .args
            .iter()
            .any(|arg| arg.contains("Library/Keychains/login.keychain"))
    );
    assert_eq!(command.args.last().unwrap(), "/tmp/DAM's CA/ca.pem");
}

#[test]
fn linux_install_and_remove_commands_use_trust_anchor() {
    let install = linux_install_command(Path::new("/tmp/dam ca/ca.pem"));
    assert_eq!(install.program, LINUX_SUDO);
    assert_eq!(&install.args[0..3], &[LINUX_TRUST, "anchor", "--store"]);
    assert_eq!(
        Path::new(install.args.last().unwrap()),
        Path::new("/tmp/dam ca/ca.pem")
    );
    assert!(system_trust_command_inherits_stdio(&install));

    let dir = tempfile::tempdir().unwrap();
    let artifact = generate_local_ca_artifact_at(dir.path(), 1).unwrap();
    let remove = linux_remove_command(&artifact).unwrap();
    assert_eq!(remove.program, LINUX_SUDO);
    assert_eq!(remove.args[0], LINUX_TRUST);
    assert_eq!(remove.args[1], "anchor");
    assert_eq!(remove.args[2], "--remove");
    assert_eq!(
        remove.args[3],
        artifact.paths.certificate_path.display().to_string()
    );
}

#[test]
fn local_ca_manifest_marks_install_and_remove_without_system_trust() {
    let dir = tempfile::tempdir().unwrap();
    generate_local_ca_artifact_at(dir.path(), 1).unwrap();

    let installed = mark_local_ca_installed_at(dir.path(), 2).unwrap();
    assert_eq!(installed.record.installed_at_unix, Some(2));
    assert!(delete_local_ca_artifact(dir.path()).is_err());

    let uninstalled = mark_local_ca_uninstalled(dir.path()).unwrap();
    assert_eq!(uninstalled.record.installed_at_unix, None);
    assert!(delete_local_ca_artifact(dir.path()).unwrap());
}

#[test]
fn local_ca_artifact_generation_refuses_to_overwrite_existing_material() {
    let dir = tempfile::tempdir().unwrap();

    generate_local_ca_artifact_at(dir.path(), 1).unwrap();
    let error = generate_local_ca_artifact_at(dir.path(), 2).unwrap_err();

    assert!(matches!(error, TrustArtifactError::AlreadyExists(_)));
}

#[test]
fn local_ca_artifact_issues_leaf_certificates_for_hosts() {
    let dir = tempfile::tempdir().unwrap();
    generate_local_ca_artifact_at(dir.path(), 1).unwrap();

    let issued =
        issue_local_ca_leaf_certificate(dir.path(), "https://API.OPENAI.COM:443/v1").unwrap();

    assert_eq!(issued.host, "api.openai.com");
    assert!(!issued.certificate_der.is_empty());
    assert!(!issued.private_key_der.is_empty());
    assert!(issued.certificate_pem.contains("BEGIN CERTIFICATE"));
    assert!(issued.private_key_pem.contains("BEGIN PRIVATE KEY"));
    assert!(issued.ca_certificate_pem.contains("BEGIN CERTIFICATE"));
    assert!(!issued.ca_certificate_der.is_empty());
}
