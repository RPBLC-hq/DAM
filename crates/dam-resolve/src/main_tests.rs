use super::*;

#[test]
fn parse_args_defaults_to_stdin() {
    let cli = parse_args(Vec::new()).unwrap();

    assert_eq!(cli.config, dam_config::ConfigOverrides::default());
    assert!(cli.file.is_none());
    assert!(!cli.report);
    assert!(!cli.json_report);
    assert!(!cli.strict);
}

#[test]
fn parse_args_accepts_common_options() {
    let cli = parse_args([
        "--config".to_string(),
        "/tmp/dam.toml".to_string(),
        "--db".to_string(),
        "/tmp/vault.db".to_string(),
        "--log".to_string(),
        "/tmp/log.db".to_string(),
        "--report".to_string(),
        "--json-report".to_string(),
        "--strict".to_string(),
        "input.txt".to_string(),
    ])
    .unwrap();

    assert_eq!(cli.config.config_path, Some(PathBuf::from("/tmp/dam.toml")));
    assert_eq!(
        cli.config.vault_sqlite_path,
        Some(PathBuf::from("/tmp/vault.db"))
    );
    assert_eq!(
        cli.config.log_sqlite_path,
        Some(PathBuf::from("/tmp/log.db"))
    );
    assert_eq!(cli.file, Some(PathBuf::from("input.txt")));
    assert!(cli.report);
    assert!(cli.json_report);
    assert!(cli.strict);
}

#[test]
fn parse_args_accepts_no_log_override() {
    let cli = parse_args(["--no-log".to_string()]).unwrap();

    assert_eq!(cli.config.log_enabled, Some(false));
}

#[test]
fn plain_report_hides_vault_read_error_details() {
    let reference = dam_core::Reference::generate(dam_core::SensitiveType::Email);
    let plan = dam_core::ResolvePlan {
        references: vec![dam_core::ReferenceMatch {
            span: dam_core::Span { start: 0, end: 1 },
            reference: reference.clone(),
        }],
        read_failures: vec![dam_core::VaultReadFailure {
            span: dam_core::Span { start: 0, end: 1 },
            reference,
            error: "backend echoed alice@example.com".to_string(),
        }],
        ..dam_core::ResolvePlan::default()
    };

    let report = plain_report("op-1", &plan);

    assert!(report.contains("read_error email 0..1"));
    assert!(report.contains(dam_api::VAULT_READ_FAILURE_REPORT_ERROR));
    assert!(!report.contains("backend echoed"));
    assert!(!report.contains("alice@example.com"));
}
