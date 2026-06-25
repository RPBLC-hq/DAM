use super::*;

#[test]
fn linux_connect_args_use_explicit_proxy_defaults() {
    let data_paths = DataPaths {
        state_dir: PathBuf::from("/tmp/dam-state"),
        vault_path: PathBuf::from("/tmp/dam-state/vault.db"),
        log_path: PathBuf::from("/tmp/dam-state/log.db"),
        consent_path: PathBuf::from("/tmp/dam-state/consent.db"),
    };

    let args = connect_args(&data_paths, Some(&PathBuf::from("dam.toml")), true);

    assert!(args.contains(&"--apply".to_string()));
    assert!(args.windows(2).any(|pair| pair == ["--config", "dam.toml"]));
    assert!(
        args.windows(2)
            .any(|pair| pair == ["--network-mode", "explicit_proxy"])
    );
    assert!(
        args.windows(2)
            .any(|pair| pair == ["--trust-mode", "disabled"])
    );
    assert!(
        args.windows(2)
            .any(|pair| pair == ["--consent-db", "/tmp/dam-state/consent.db"])
    );
}

#[test]
fn linux_url_guard_allows_only_tray_origin() {
    assert!(url_has_local_origin(
        "http://127.0.0.1:2896/connect?notice=ok",
        "127.0.0.1:2896"
    ));
    assert!(!url_has_local_origin("https://rpblc.com", "127.0.0.1:2896"));
    assert!(!url_has_local_origin(
        "http://127.0.0.1:2897/connect",
        "127.0.0.1:2896"
    ));
}

#[test]
fn linux_js_string_literal_escapes_html_sensitive_chars() {
    assert_eq!(
        js_string_literal("a\\b\"<>&\n"),
        "\"a\\\\b\\\"\\u003c\\u003e\\u0026\\n\""
    );
}

#[test]
fn linux_hex_encode_uses_lowercase_pairs() {
    assert_eq!(hex_encode(&[0x00, 0x0f, 0xa5, 0xff]), "000fa5ff");
}
