use super::*;

#[test]
fn settings_errors_map_to_stable_codes() {
    assert_eq!(
        settings_error("target changed outside DAM".into()).code,
        WebErrorCode::ApplyModifiedTarget
    );
    assert_eq!(
        settings_error("failed to write target".into()).code,
        WebErrorCode::ApplyTargetUnwritable
    );
    assert_eq!(
        settings_error("some unexpected integration error".into()).code,
        WebErrorCode::Unknown
    );
}

#[test]
fn capture_scope_expands_enabled_profiles_to_hosts_apps_and_targets() {
    let dir = tempfile::tempdir().unwrap();
    let integration_dir = dir.path().join("integrations");
    dam_integrations::ensure_bundled_profile_files(&integration_dir).unwrap();
    dam_integrations::set_integration_enabled("codex", true, &integration_dir).unwrap();

    let scope = capture_scope_for_state(&dam_config::DamConfig::default(), dir.path()).unwrap();

    assert_eq!(
        scope.traffic_app_ids,
        Some(vec![
            "anthropic-api".to_string(),
            "openai-api".to_string(),
            "chatgpt-codex".to_string(),
        ])
    );
    assert!(scope.hosts.contains(&"api.anthropic.com".to_string()));
    assert!(scope.hosts.contains(&"api.openai.com".to_string()));
    assert!(scope.hosts.contains(&"chatgpt.com".to_string()));
    assert!(scope.hosts.contains(&"ab.chatgpt.com".to_string()));
    assert!(
        scope
            .proxy_targets
            .iter()
            .any(|target| target.name == "anthropic")
    );
    assert!(
        scope
            .proxy_targets
            .iter()
            .any(|target| target.name == "openai")
    );
    assert!(
        scope
            .proxy_targets
            .iter()
            .any(|target| target.name == "chatgpt-codex")
    );
}

#[test]
fn capture_scope_preserves_explicit_empty_enabled_profile_state() {
    let dir = tempfile::tempdir().unwrap();
    let integration_dir = dir.path().join("integrations");
    dam_integrations::ensure_bundled_profile_files(&integration_dir).unwrap();
    dam_integrations::set_integration_enabled("claude-code", false, &integration_dir).unwrap();

    let scope = capture_scope_for_state(&dam_config::DamConfig::default(), dir.path()).unwrap();

    assert_eq!(scope.traffic_app_ids, Some(Vec::new()));
    assert!(scope.hosts.is_empty());
    assert!(scope.proxy_targets.is_empty());
}
