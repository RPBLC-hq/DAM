use super::*;

#[test]
fn parses_tray_args() {
    let args = parse_args([
        "--addr".to_string(),
        "127.0.0.1:3000".to_string(),
        "--dam-bin".to_string(),
        "/tmp/dam".to_string(),
        "--dam-web-bin".to_string(),
        "/tmp/dam-web".to_string(),
        "--config".to_string(),
        "dam.toml".to_string(),
        "--db".to_string(),
        "vault.db".to_string(),
        "--log".to_string(),
        "log.db".to_string(),
        "--activate-system-extension".to_string(),
        "com.rpblc.dam.network-extension".to_string(),
        "--deactivate-system-extension".to_string(),
        "com.rpblc.dam.network-extension".to_string(),
    ])
    .unwrap();

    assert_eq!(args.addr.as_deref(), Some("127.0.0.1:3000"));
    assert_eq!(args.dam_bin, Some(PathBuf::from("/tmp/dam")));
    assert_eq!(args.dam_web_bin, Some(PathBuf::from("/tmp/dam-web")));
    assert_eq!(args.config_path, Some(PathBuf::from("dam.toml")));
    assert_eq!(args.db_path, Some(PathBuf::from("vault.db")));
    assert_eq!(args.log_path, Some(PathBuf::from("log.db")));
    assert_eq!(
        args.activate_system_extension.as_deref(),
        Some("com.rpblc.dam.network-extension")
    );
    assert_eq!(
        args.deactivate_system_extension.as_deref(),
        Some("com.rpblc.dam.network-extension")
    );
}

#[test]
fn rejects_non_loopback_web_addr() {
    let error = choose_web_addr(Some("0.0.0.0:2896")).unwrap_err();

    assert!(error.contains("loopback"));
}

#[test]
fn rejects_occupied_explicit_web_addr() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let error = choose_web_addr(Some(&addr)).unwrap_err();

    assert!(error.contains("already in use"));
}

#[test]
fn builds_connect_url() {
    assert_eq!(
        connect_url("127.0.0.1:2896"),
        "http://127.0.0.1:2896/connect"
    );
}

#[test]
fn hex_encode_uses_lowercase_pairs() {
    assert_eq!(macos::hex_encode(&[0x00, 0x0f, 0xa5, 0xff]), "000fa5ff");
}
