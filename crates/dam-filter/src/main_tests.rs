use super::*;

#[test]
fn parse_args_defaults_to_stdin() {
    let cli = parse_args(Vec::new()).unwrap();

    assert_eq!(cli.config, dam_config::ConfigOverrides::default());
    assert!(cli.file.is_none());
    assert!(!cli.report);
    assert!(!cli.json_report);
}

#[test]
fn parse_args_accepts_config_db_log_report_and_file() {
    let cli = parse_args([
        "--config".to_string(),
        "/tmp/dam.toml".to_string(),
        "--db".to_string(),
        "/tmp/vault.db".to_string(),
        "--log".to_string(),
        "/tmp/log.db".to_string(),
        "--report".to_string(),
        "--json-report".to_string(),
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
}

#[test]
fn parse_args_accepts_no_log_override() {
    let cli = parse_args(["--no-log".to_string()]).unwrap();

    assert_eq!(cli.config.log_enabled, Some(false));
}

#[test]
fn preview_short_values_are_unchanged() {
    assert_eq!(preview("abc"), "abc");
}

#[test]
fn preview_long_values_are_truncated() {
    assert_eq!(preview("abcdef"), "abcd...");
}
