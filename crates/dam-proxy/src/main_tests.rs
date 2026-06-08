use super::*;

#[test]
fn parse_args_enables_proxy_and_accepts_upstream() {
    let cli = parse_args([
        "--listen".to_string(),
        "127.0.0.1:9000".to_string(),
        "--upstream".to_string(),
        "http://127.0.0.1:9999".to_string(),
        "--no-resolve-inbound".to_string(),
        "--no-api-key-env".to_string(),
    ])
    .unwrap();

    assert_eq!(cli.config.proxy_enabled, Some(true));
    assert_eq!(cli.config.proxy_listen, Some("127.0.0.1:9000".to_string()));
    assert_eq!(
        cli.config.proxy_target_upstream,
        Some("http://127.0.0.1:9999".to_string())
    );
    assert_eq!(cli.config.proxy_resolve_inbound, Some(false));
    assert_eq!(cli.config.proxy_target_api_key_env, Some(String::new()));
}

#[test]
fn parse_args_rejects_unknown_args() {
    assert!(parse_args(["--wat".to_string()]).is_err());
}
