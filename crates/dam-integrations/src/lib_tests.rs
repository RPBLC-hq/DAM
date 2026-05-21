use super::*;

fn claude_traffic_app_ids() -> Vec<String> {
    [
        "anthropic-api",
        "claude-web",
        "anthropic-console",
        "claude-mcp-proxy",
        "claude-platform",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn chatgpt_traffic_app_ids() -> Vec<String> {
    [
        "openai-api",
        "openai-platform",
        "chatgpt-web",
        "chatgpt-legacy-web",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

#[test]
fn lists_dynamic_profile_ids_and_enables_all_by_default() {
    assert_eq!(profile_ids(), ["claude", "chatgpt"]);
    assert_eq!(default_enabled_profile_ids(), ["claude", "chatgpt"]);
}

#[test]
fn claude_code_profile_uses_proxy_env_not_anthropic_base_url() {
    let profile = profile("claude", "http://127.0.0.1:7828/").unwrap();

    assert!(!profile.connect_args.contains(&"--anthropic".to_string()));
    assert!(profile.connect_args.contains(&"--network-mode".to_string()));
    assert!(profile.connect_args.contains(&"tun".to_string()));
    assert!(profile.connect_args.contains(&"--trust-mode".to_string()));
    assert!(profile.connect_args.contains(&"local_ca".to_string()));
    assert_eq!(profile.traffic_app_ids, claude_traffic_app_ids());
    assert_eq!(profile.settings[0].key, HTTPS_PROXY_ENV);
    assert_eq!(profile.settings[0].value, "http://127.0.0.1:7828");
    assert_eq!(profile.settings[1].key, HTTP_PROXY_ENV);
    assert!(
        !profile
            .settings
            .iter()
            .any(|setting| setting.key == "ANTHROPIC_BASE_URL")
    );
}

#[test]
fn chatgpt_profile_merges_api_platform_and_subscription_traffic() {
    let profile = profile("chatgpt", DEFAULT_PROXY_URL).unwrap();
    let command = &profile.commands[1].command;

    assert_eq!(profile.provider, "openai-compatible");
    assert!(!profile.connect_args.contains(&"--openai".to_string()));
    assert!(profile.connect_args.contains(&"--network-mode".to_string()));
    assert!(profile.connect_args.contains(&"tun".to_string()));
    assert!(profile.connect_args.contains(&"--trust-mode".to_string()));
    assert!(profile.connect_args.contains(&"local_ca".to_string()));
    assert_eq!(profile.settings[0].key, HTTPS_PROXY_ENV);
    assert_eq!(profile.settings[1].key, HTTP_PROXY_ENV);
    assert_eq!(profile.traffic_app_ids, chatgpt_traffic_app_ids());
    assert!(command.contains(&format!("{HTTPS_PROXY_ENV}={DEFAULT_PROXY_URL}")));
    assert!(command.contains(&format!("{HTTP_PROXY_ENV}={DEFAULT_PROXY_URL}")));
    assert!(!command.iter().any(|arg| arg.contains("dam_openai")));
}

#[test]
fn removed_profiles_are_not_visible_catalog_entries() {
    let visible_ids = profiles(DEFAULT_PROXY_URL)
        .into_iter()
        .map(|profile| profile.id)
        .collect::<Vec<_>>();

    assert_eq!(visible_ids, vec!["claude", "chatgpt"]);
    assert!(profile("xai-compatible", DEFAULT_PROXY_URL).is_none());
}

#[test]
fn retired_profile_aliases_resolve_to_current_profiles() {
    assert_eq!(
        profile("claude-code", DEFAULT_PROXY_URL).unwrap().id,
        "claude"
    );
    assert_eq!(
        profile("anthropic", DEFAULT_PROXY_URL).unwrap().id,
        "claude"
    );
    assert_eq!(
        profile("openai-compatible", DEFAULT_PROXY_URL).unwrap().id,
        "chatgpt"
    );
    assert_eq!(profile("codex", DEFAULT_PROXY_URL).unwrap().id, "chatgpt");
}

#[test]
fn chatgpt_default_path_lives_under_integration_state() {
    let dir = tempfile::tempdir().unwrap();
    let integration_dir = dir.path().join("integrations");
    let path = default_apply_path("chatgpt", &integration_dir).unwrap();
    let alias_path = default_apply_path("codex", &integration_dir).unwrap();

    assert_eq!(path, integration_dir.join("profiles").join("chatgpt.json"));
    assert_eq!(alias_path, path);
}

#[test]
fn claude_default_path_lives_under_profile_folder() {
    let dir = tempfile::tempdir().unwrap();
    let integration_dir = dir.path().join("integrations");
    let path = default_apply_path("claude", &integration_dir).unwrap();

    assert_eq!(path, integration_dir.join("profiles").join("claude.json"));
}

#[test]
fn bundled_profile_files_are_seeded_as_json() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("integrations");

    let written = ensure_bundled_profile_files(&state_dir).unwrap();

    assert_eq!(written.len(), 2);
    for profile_id in ["claude", "chatgpt"] {
        let path = profile_definition_path(&state_dir, profile_id);
        let raw = fs::read_to_string(path).unwrap();
        let profile: IntegrationProfile = serde_json::from_str(&raw).unwrap();
        assert_eq!(profile.id, profile_id);
    }
}

#[test]
fn ensure_bundled_profile_files_refreshes_stale_known_profiles() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("integrations");
    let profiles_dir = profile_definitions_dir(&state_dir);
    fs::create_dir_all(&profiles_dir).unwrap();
    let path = profile_definition_path(&state_dir, "claude");
    fs::write(
        &path,
        r#"{
          "id": "claude",
          "name": "Claude Code",
          "summary": "Stale profile",
          "provider": "anthropic",
          "traffic_app_ids": ["anthropic-api"],
          "connect_args": ["--anthropic", "--network-mode", "tun", "--trust-mode", "local_ca"],
          "settings": [],
          "commands": [],
          "notes": [],
          "automation": "connect_preset"
        }"#,
    )
    .unwrap();

    let written = ensure_bundled_profile_files(&state_dir).unwrap();
    let raw = fs::read_to_string(&path).unwrap();
    let profile: IntegrationProfile = serde_json::from_str(&raw).unwrap();

    assert!(written.contains(&path));
    assert!(written.contains(&profile_definition_path(&state_dir, "chatgpt")));
    assert!(!profile.connect_args.contains(&"--anthropic".to_string()));
    assert!(profile.connect_args.contains(&"--network-mode".to_string()));
    assert!(!profile.settings.is_empty());
}

#[test]
fn ensure_bundled_profile_files_removes_retired_alias_profile_files() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("integrations");
    let profiles_dir = profile_definitions_dir(&state_dir);
    fs::create_dir_all(&profiles_dir).unwrap();
    let retired_path = profiles_dir.join("claude-code.json");
    fs::write(&retired_path, "{\"id\":\"claude-code\"}\n").unwrap();

    ensure_bundled_profile_files(&state_dir).unwrap();

    assert!(!retired_path.exists());
    assert!(profile_definition_path(&state_dir, "claude").exists());
}

#[test]
fn profiles_from_state_does_not_seed_profile_files() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("integrations");

    let profiles = profiles_from_state(DEFAULT_PROXY_URL, &state_dir).unwrap();

    assert_eq!(
        profiles
            .iter()
            .map(|profile| profile.id.as_str())
            .collect::<Vec<_>>(),
        vec!["claude", "chatgpt"]
    );
    assert!(!state_dir.exists());
}

#[test]
fn profiles_from_state_loads_valid_custom_profile_files() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("integrations");
    let profiles_dir = profile_definitions_dir(&state_dir);
    fs::create_dir_all(&profiles_dir).unwrap();
    fs::write(
        profiles_dir.join("example-mail.json"),
        r#"{
          "id": "example-mail",
          "name": "Example Mail",
          "summary": "Route Example Mail traffic through DAM.",
          "provider": "generic-http",
          "traffic_app_ids": ["example-mail"],
          "connect_args": ["--network-mode", "tun", "--trust-mode", "local_ca"],
          "settings": [],
          "commands": [],
          "notes": [],
          "automation": "connect_preset"
        }"#,
    )
    .unwrap();

    let profiles = profiles_from_state(DEFAULT_PROXY_URL, &state_dir).unwrap();

    assert!(profiles.iter().any(|profile| profile.id == "example-mail"));
    assert_eq!(
        runtime_enabled_profile_ids(&state_dir).unwrap(),
        Some(vec![
            "claude".to_string(),
            "chatgpt".to_string(),
            "example-mail".to_string()
        ])
    );
}

#[test]
fn profiles_from_state_uses_current_bundled_profile_for_stale_catalog_files() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("integrations");
    let profiles_dir = profile_definitions_dir(&state_dir);
    fs::create_dir_all(&profiles_dir).unwrap();
    fs::write(
        profiles_dir.join("claude.json"),
        r#"{
          "id": "claude",
          "name": "Claude Code",
          "summary": "Stale profile",
          "provider": "anthropic",
          "traffic_app_ids": ["anthropic-api"],
          "connect_args": ["--anthropic", "--network-mode", "tun", "--trust-mode", "local_ca"],
          "settings": [],
          "commands": [],
          "notes": [],
          "automation": "connect_preset"
        }"#,
    )
    .unwrap();
    fs::write(
        profiles_dir.join("chatgpt.json"),
        r#"{
          "id": "chatgpt",
          "name": "ChatGPT",
          "summary": "Stale profile",
          "provider": "openai-compatible",
          "traffic_app_ids": ["openai-api"],
          "connect_args": ["--openai", "--network-mode", "tun", "--trust-mode", "local_ca"],
          "settings": [],
          "commands": [],
          "notes": [],
          "automation": "connect_preset"
        }"#,
    )
    .unwrap();

    let profiles = profiles_from_state(DEFAULT_PROXY_URL, &state_dir).unwrap();
    let claude = profiles
        .iter()
        .find(|profile| profile.id == "claude")
        .unwrap();
    let chatgpt = profiles
        .iter()
        .find(|profile| profile.id == "chatgpt")
        .unwrap();

    assert!(!claude.connect_args.contains(&"--anthropic".to_string()));
    assert_eq!(claude.traffic_app_ids, claude_traffic_app_ids());
    assert!(!chatgpt.connect_args.contains(&"--openai".to_string()));
    assert_eq!(chatgpt.traffic_app_ids, chatgpt_traffic_app_ids());
}

#[test]
fn profiles_from_state_ignores_retired_stored_profile_files() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("integrations");
    let profiles_dir = profile_definitions_dir(&state_dir);
    fs::create_dir_all(&profiles_dir).unwrap();
    fs::write(
        profiles_dir.join("anthropic.json"),
        r#"{
          "id": "anthropic",
          "name": "Anthropic",
          "summary": "Retired profile",
          "provider": "anthropic",
          "traffic_app_ids": ["anthropic-api"],
          "connect_args": [],
          "settings": [],
          "commands": [],
          "notes": [],
          "automation": "connect_preset"
        }"#,
    )
    .unwrap();

    let profiles = profiles_from_state(DEFAULT_PROXY_URL, &state_dir).unwrap();

    assert_eq!(
        profiles
            .iter()
            .map(|profile| profile.id.as_str())
            .collect::<Vec<_>>(),
        vec!["claude", "chatgpt"]
    );
}

#[test]
fn ensure_bundled_profile_files_migrates_legacy_rollback_records() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("integrations");
    let target_path = dir.path().join("chatgpt.json");
    let legacy_dir = legacy_profile_state_dir(&state_dir, "codex");
    let legacy_backup_dir = legacy_dir.join("backups").join("123");
    let legacy_backup_path = legacy_backup_dir.join("target.backup");
    fs::create_dir_all(&legacy_backup_dir).unwrap();
    fs::write(&target_path, "{\"id\":\"codex\"}\n").unwrap();
    fs::write(&legacy_backup_path, "{\"id\":\"old\"}\n").unwrap();
    write_json_file(
        &legacy_dir.join("latest.json"),
        &IntegrationApplyRecord {
            profile_id: "codex".to_string(),
            applied_at_unix: 123,
            files: vec![IntegrationBackupFile {
                path: target_path.clone(),
                existed: true,
                backup_path: Some(legacy_backup_path),
            }],
        },
    )
    .unwrap();

    ensure_bundled_profile_files(&state_dir).unwrap();
    let migrated_record_path = profile_state_dir(&state_dir, "chatgpt").join("latest.json");
    let migrated_raw = fs::read_to_string(&migrated_record_path).unwrap();
    let migrated_record: IntegrationApplyRecord = serde_json::from_str(&migrated_raw).unwrap();
    let migrated_backup_path = migrated_record.files[0].backup_path.as_ref().unwrap();

    assert_eq!(migrated_record.profile_id, "chatgpt");
    assert!(migrated_record_path.exists());
    assert!(migrated_backup_path.starts_with(profile_state_dir(&state_dir, "chatgpt")));
    assert!(migrated_backup_path.exists());
    assert!(!legacy_dir.exists());

    let rollback = rollback_profile("codex", &state_dir).unwrap();
    assert_eq!(rollback.profile_id, "chatgpt");
    assert_eq!(rollback.changes[0].action, FileAction::Restore);
    assert_eq!(
        fs::read_to_string(&target_path).unwrap(),
        "{\"id\":\"old\"}\n"
    );
}

#[test]
fn ensure_bundled_profile_files_migrates_retired_claude_rollback_records() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("integrations");
    let target_path = dir.path().join("claude.json");
    let alias_dir = profile_state_dir(&state_dir, "claude-code");
    let alias_backup_dir = alias_dir.join("backups").join("123");
    let alias_backup_path = alias_backup_dir.join("target.backup");
    fs::create_dir_all(&alias_backup_dir).unwrap();
    fs::write(&target_path, "{\"id\":\"claude\"}\n").unwrap();
    fs::write(&alias_backup_path, "{\"id\":\"old\"}\n").unwrap();
    write_json_file(
        &alias_dir.join("latest.json"),
        &IntegrationApplyRecord {
            profile_id: "claude-code".to_string(),
            applied_at_unix: 123,
            files: vec![IntegrationBackupFile {
                path: target_path.clone(),
                existed: true,
                backup_path: Some(alias_backup_path),
            }],
        },
    )
    .unwrap();

    ensure_bundled_profile_files(&state_dir).unwrap();

    let migrated_record_path = profile_state_dir(&state_dir, "claude").join("latest.json");
    let migrated_raw = fs::read_to_string(&migrated_record_path).unwrap();
    let migrated_record: IntegrationApplyRecord = serde_json::from_str(&migrated_raw).unwrap();

    assert_eq!(migrated_record.profile_id, "claude");
    assert!(migrated_record_path.exists());
    assert!(!alias_dir.exists());
    assert!(
        migrated_record.files[0]
            .backup_path
            .as_ref()
            .unwrap()
            .starts_with(profile_state_dir(&state_dir, "claude"))
    );

    let rollback = rollback_profile("claude-code", &state_dir).unwrap();
    assert_eq!(rollback.profile_id, "claude");
    assert_eq!(rollback.changes[0].action, FileAction::Restore);
    assert_eq!(
        fs::read_to_string(&target_path).unwrap(),
        "{\"id\":\"old\"}\n"
    );
}

#[test]
fn prepare_apply_in_state_refreshes_known_catalog_profile_content() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("integrations");
    let profiles_dir = profile_definitions_dir(&state_dir);
    fs::create_dir_all(&profiles_dir).unwrap();
    let target_path = profile_definition_path(&state_dir, "chatgpt");
    fs::write(
        &target_path,
        r#"{
          "id": "stale-chatgpt",
          "name": "Stale ChatGPT",
          "summary": "Stale profile",
          "provider": "openai-compatible",
          "traffic_app_ids": ["openai-api"],
          "connect_args": [],
          "settings": [],
          "commands": [],
          "notes": [],
          "automation": "connect_preset"
        }"#,
    )
    .unwrap();

    let prepared =
        prepare_apply_in_state("codex", DEFAULT_PROXY_URL, target_path, &state_dir).unwrap();

    assert_eq!(prepared.profile_id, "chatgpt");
    assert_eq!(prepared.profile_name, "ChatGPT");
    assert!(prepared.desired_content.contains("\"id\": \"chatgpt\""));
    assert!(!prepared.desired_content.contains("stale-chatgpt"));
    assert!(!prepared.desired_content.contains("--openai"));
}

#[test]
fn catalog_profile_file_is_already_applied_when_seeded() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("integrations");
    ensure_bundled_profile_files(&state_dir).unwrap();
    let target_path = profile_definition_path(&state_dir, "claude");

    let inspection = inspect_apply_in_state(
        "claude",
        DEFAULT_PROXY_URL,
        target_path,
        &state_dir,
        &state_dir,
    )
    .unwrap();

    assert_eq!(inspection.status, IntegrationApplyStatus::Applied);
    assert_eq!(inspection.planned_action, FileAction::Unchanged);
}

#[test]
fn active_profile_state_roundtrips_and_clears() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("integrations");

    assert_eq!(read_active_profile(&state_dir).unwrap(), None);

    let selected = set_active_profile("claude", &state_dir).unwrap();
    assert_eq!(selected.profile_id, "claude");
    assert_eq!(read_active_profile(&state_dir).unwrap(), Some(selected));

    assert!(clear_active_profile(&state_dir).unwrap());
    assert_eq!(read_active_profile(&state_dir).unwrap(), None);
    assert!(!clear_active_profile(&state_dir).unwrap());
}

#[test]
fn active_profile_rejects_unknown_profile() {
    let dir = tempfile::tempdir().unwrap();
    let error = set_active_profile("missing", dir.path()).unwrap_err();

    assert!(error.contains("unknown integration profile: missing"));
}

#[test]
fn enabled_integrations_roundtrip_and_fallback_to_active_profile() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("integrations");

    let active = set_active_profile("claude", &state_dir).unwrap();
    assert_eq!(
        read_effective_enabled_integrations(&state_dir).unwrap(),
        vec![EnabledIntegrationState {
            profile_id: "claude".to_string(),
            enabled_at_unix: active.selected_at_unix,
        }]
    );

    let enabled = set_integration_enabled("codex", true, &state_dir).unwrap();
    assert_eq!(
        enabled
            .iter()
            .map(|profile| profile.profile_id.as_str())
            .collect::<Vec<_>>(),
        vec!["claude", "chatgpt"]
    );
    assert_eq!(
        enabled_profile_ids(&state_dir).unwrap(),
        vec!["claude".to_string(), "chatgpt".to_string()]
    );

    let enabled = set_integration_enabled("claude", true, &state_dir).unwrap();
    assert_eq!(
        enabled
            .iter()
            .map(|profile| profile.profile_id.as_str())
            .collect::<Vec<_>>(),
        vec!["chatgpt", "claude"]
    );

    let enabled = set_integration_enabled("codex", false, &state_dir).unwrap();
    assert_eq!(enabled.len(), 1);
    assert_eq!(enabled[0].profile_id, "claude");

    assert!(clear_enabled_integrations(&state_dir).unwrap());
    assert!(!clear_enabled_integrations(&state_dir).unwrap());
}

#[test]
fn runtime_enabled_integrations_default_to_all_profiles() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("integrations");

    assert_eq!(
        runtime_enabled_profile_ids(&state_dir).unwrap(),
        Some(vec!["claude".to_string(), "chatgpt".to_string()])
    );

    set_active_profile("claude", &state_dir).unwrap();
    assert_eq!(
        runtime_enabled_profile_ids(&state_dir).unwrap(),
        Some(vec!["claude".to_string()])
    );

    set_integration_enabled("claude", false, &state_dir).unwrap();
    assert_eq!(
        runtime_enabled_profile_ids(&state_dir).unwrap(),
        Some(Vec::new())
    );
    assert_eq!(
        traffic_app_ids_for_profile_ids(&["claude".to_string(), "chatgpt".to_string()]).unwrap(),
        vec![
            "anthropic-api".to_string(),
            "claude-web".to_string(),
            "anthropic-console".to_string(),
            "claude-mcp-proxy".to_string(),
            "claude-platform".to_string(),
            "openai-api".to_string(),
            "openai-platform".to_string(),
            "chatgpt-web".to_string(),
            "chatgpt-legacy-web".to_string()
        ]
    );
}

#[test]
fn retired_enabled_profile_ids_are_migrated_for_runtime_state() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("integrations");
    fs::create_dir_all(&state_dir).unwrap();
    write_json_file(
        &enabled_integrations_path(&state_dir),
        &EnabledIntegrationsState {
            profiles: vec![
                EnabledIntegrationState {
                    profile_id: "openai-compatible".to_string(),
                    enabled_at_unix: 1,
                },
                EnabledIntegrationState {
                    profile_id: "codex-chatgpt".to_string(),
                    enabled_at_unix: 2,
                },
                EnabledIntegrationState {
                    profile_id: "anthropic".to_string(),
                    enabled_at_unix: 3,
                },
                EnabledIntegrationState {
                    profile_id: "xai-compatible".to_string(),
                    enabled_at_unix: 4,
                },
            ],
        },
    )
    .unwrap();

    assert_eq!(
        runtime_enabled_profile_ids(&state_dir).unwrap(),
        Some(vec!["chatgpt".to_string(), "claude".to_string()])
    );
}

#[test]
fn ensure_bundled_profile_files_rewrites_retired_runtime_state_ids() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("integrations");
    fs::create_dir_all(&state_dir).unwrap();
    write_json_file(
        &enabled_integrations_path(&state_dir),
        &EnabledIntegrationsState {
            profiles: vec![
                EnabledIntegrationState {
                    profile_id: "claude-code".to_string(),
                    enabled_at_unix: 1,
                },
                EnabledIntegrationState {
                    profile_id: "anthropic".to_string(),
                    enabled_at_unix: 2,
                },
            ],
        },
    )
    .unwrap();
    write_json_file(
        &active_profile_path(&state_dir),
        &ActiveProfileState {
            profile_id: "claude-code".to_string(),
            selected_at_unix: 3,
        },
    )
    .unwrap();

    ensure_bundled_profile_files(&state_dir).unwrap();

    let enabled_raw = fs::read_to_string(enabled_integrations_path(&state_dir)).unwrap();
    let enabled: EnabledIntegrationsState = serde_json::from_str(&enabled_raw).unwrap();
    let active = read_active_profile(&state_dir).unwrap().unwrap();

    assert_eq!(enabled.profiles.len(), 1);
    assert_eq!(enabled.profiles[0].profile_id, "claude");
    assert_eq!(enabled.profiles[0].enabled_at_unix, 1);
    assert_eq!(active.profile_id, "claude");
    assert_eq!(active.selected_at_unix, 3);
}

#[test]
fn enabled_integrations_reject_unknown_profile() {
    let dir = tempfile::tempdir().unwrap();
    let error = set_integration_enabled("missing", true, dir.path()).unwrap_err();

    assert!(error.contains("unknown integration profile: missing"));
}

#[test]
fn chatgpt_apply_writes_profile_json_and_rollback_restores_backup() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    let profile_path = dir.path().join("chatgpt.json");
    let original = "{\"id\":\"old-profile\"}\n";
    fs::write(&profile_path, original).unwrap();

    let prepared = prepare_apply("chatgpt", "http://127.0.0.1:9000", profile_path.clone()).unwrap();
    let result = run_apply(prepared, false, &state_dir).unwrap();

    assert!(!result.dry_run);
    assert_eq!(result.changes[0].action, FileAction::Update);
    let applied = fs::read_to_string(&profile_path).unwrap();
    let profile: IntegrationProfile = serde_json::from_str(&applied).unwrap();
    assert_eq!(profile.id, "chatgpt");
    assert_eq!(profile.traffic_app_ids, chatgpt_traffic_app_ids());
    assert_eq!(profile.settings[0].value, "http://127.0.0.1:9000");
    assert!(!applied.contains("dam_openai"));

    let rollback = rollback_profile("chatgpt", &state_dir).unwrap();

    assert_eq!(rollback.changes[0].action, FileAction::Restore);
    assert_eq!(fs::read_to_string(&profile_path).unwrap(), original);
}

#[test]
fn claude_code_apply_writes_profile_json_and_rollback_restores_backup() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    let profile_path = dir.path().join("claude.json");
    let original = "{\"id\":\"old-profile\"}\n";
    fs::write(&profile_path, original).unwrap();

    let prepared = prepare_apply("claude", "http://127.0.0.1:9000/", profile_path.clone()).unwrap();
    let result = run_apply(prepared, false, &state_dir).unwrap();

    assert_eq!(result.changes[0].action, FileAction::Update);
    let applied = fs::read_to_string(&profile_path).unwrap();
    let profile: IntegrationProfile = serde_json::from_str(&applied).unwrap();
    assert_eq!(profile.id, "claude");
    assert_eq!(profile.traffic_app_ids, claude_traffic_app_ids());
    assert_eq!(profile.settings[0].value, "http://127.0.0.1:9000");
    assert_eq!(profile.settings[1].value, "http://127.0.0.1:9000");

    let rollback = rollback_profile("claude", &state_dir).unwrap();

    assert_eq!(rollback.changes[0].action, FileAction::Restore);
    assert_eq!(fs::read_to_string(&profile_path).unwrap(), original);
}

#[test]
fn profile_apply_creates_json_file_and_rollback_deletes_it() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    let profile_path = dir.path().join("chatgpt.json");

    let prepared = prepare_apply("chatgpt", "http://127.0.0.1:9000", profile_path.clone()).unwrap();
    let result = run_apply(prepared, false, &state_dir).unwrap();

    assert_eq!(result.changes[0].action, FileAction::Create);
    let applied = fs::read_to_string(&profile_path).unwrap();
    let profile: IntegrationProfile = serde_json::from_str(&applied).unwrap();
    assert_eq!(profile.id, "chatgpt");
    assert_eq!(profile.settings[0].value, "http://127.0.0.1:9000");

    let rollback = rollback_profile("chatgpt", &state_dir).unwrap();

    assert_eq!(rollback.changes[0].action, FileAction::Delete);
    assert!(!profile_path.exists());
}

#[test]
fn inspect_apply_reports_missing_applied_and_modified_states() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    let profile_path = dir.path().join("chatgpt.json");

    let missing = inspect_apply(
        "chatgpt",
        "http://127.0.0.1:9000",
        profile_path.clone(),
        &state_dir,
    )
    .unwrap();
    assert_eq!(missing.status, IntegrationApplyStatus::NeedsApply);
    assert_eq!(missing.planned_action, FileAction::Create);
    assert!(!missing.rollback_available);

    let prepared = prepare_apply("chatgpt", "http://127.0.0.1:9000", profile_path.clone()).unwrap();
    run_apply(prepared, false, &state_dir).unwrap();

    let applied = inspect_apply(
        "chatgpt",
        "http://127.0.0.1:9000",
        profile_path.clone(),
        &state_dir,
    )
    .unwrap();
    assert_eq!(applied.status, IntegrationApplyStatus::Applied);
    assert_eq!(applied.planned_action, FileAction::Unchanged);
    assert!(applied.rollback_available);

    fs::write(&profile_path, "{\"id\":\"changed\"}\n").unwrap();

    let modified =
        inspect_apply("chatgpt", "http://127.0.0.1:9000", profile_path, &state_dir).unwrap();
    assert_eq!(modified.status, IntegrationApplyStatus::Modified);
    assert_eq!(modified.planned_action, FileAction::Update);
    assert!(modified.rollback_available);
}

#[test]
fn run_apply_refuses_modified_target_with_existing_rollback_record() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    let profile_path = dir.path().join("chatgpt.json");

    let prepared = prepare_apply("chatgpt", "http://127.0.0.1:9000", profile_path.clone()).unwrap();
    run_apply(prepared, false, &state_dir).unwrap();
    fs::write(&profile_path, "{\"id\":\"changed\"}\n").unwrap();

    let prepared = prepare_apply("chatgpt", "http://127.0.0.1:9000", profile_path).unwrap();
    let error = run_apply(prepared, false, &state_dir).unwrap_err();

    assert!(error.contains("already has a rollback record"));
}

#[test]
fn run_apply_does_not_rebackup_already_applied_target() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    let profile_path = dir.path().join("chatgpt.json");

    let prepared = prepare_apply("chatgpt", "http://127.0.0.1:9000", profile_path.clone()).unwrap();
    run_apply(prepared, false, &state_dir).unwrap();
    let backups_dir = profile_state_dir(&state_dir, "chatgpt").join("backups");
    let backup_count = fs::read_dir(&backups_dir).unwrap().count();

    let prepared = prepare_apply("chatgpt", "http://127.0.0.1:9000", profile_path).unwrap();
    let result = run_apply(prepared, false, &state_dir).unwrap();

    assert_eq!(result.changes[0].action, FileAction::Unchanged);
    assert_eq!(fs::read_dir(backups_dir).unwrap().count(), backup_count);
}

#[test]
fn inspect_apply_reports_unreadable_rollback_record() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    let profile_path = dir.path().join("chatgpt.json");
    let record_path = profile_state_dir(&state_dir, "chatgpt").join("latest.json");
    fs::create_dir_all(record_path.parent().unwrap()).unwrap();
    fs::write(&record_path, "not json").unwrap();

    let report =
        inspect_apply("chatgpt", "http://127.0.0.1:9000", profile_path, &state_dir).unwrap();

    assert_eq!(report.status, IntegrationApplyStatus::NeedsApply);
    assert!(!report.rollback_available);
    assert!(report.record_error.unwrap().contains("failed to parse"));
}
