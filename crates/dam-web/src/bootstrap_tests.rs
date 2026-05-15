use super::*;

#[test]
fn injects_bootstrap_block() {
    let template = "<head><!-- DAM_WEB_BOOTSTRAP --></head>";
    let boot = Bootstrap {
        surface: "tray",
        tray_post_token: Some("abc".into()),
        version: "0.0.0",
    };
    let html = render_index(template, &boot);
    assert!(html.contains("dam-web-bootstrap"));
    assert!(html.contains("\"surface\":\"tray\""));
    assert!(html.contains("\"tray_post_token\":\"abc\""));
    assert!(!html.contains(BOOTSTRAP_PLACEHOLDER));
}

#[test]
fn escapes_angle_brackets_in_payload() {
    let template = "<!-- DAM_WEB_BOOTSTRAP -->";
    // Token is unlikely to contain '<', but guard anyway.
    let boot = Bootstrap {
        surface: "web",
        tray_post_token: Some("a<b".into()),
        version: "0.0.0",
    };
    let html = render_index(template, &boot);
    assert!(!html.contains("a<b"));
    assert!(html.contains("a\\u003cb"));
}
