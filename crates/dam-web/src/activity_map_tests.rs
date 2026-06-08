use super::*;

fn entry(event_type: &str, action: Option<&str>) -> dam_log::LogEntry {
    dam_log::LogEntry {
        id: 1,
        timestamp: 0,
        operation_id: "anthropic-test".into(),
        level: "info".into(),
        event_type: event_type.into(),
        kind: Some("email".into()),
        value: Some("ada@example.test".into()),
        reference: None,
        action: action.map(|s| s.into()),
        message: "test".into(),
    }
}

#[test]
fn decision_allow_is_granted() {
    let e = entry("policy_decision", Some("allow"));
    assert!(matches!(decision_for(&e), Some(Decision::Granted)));
}

#[test]
fn redaction_event_is_sealed() {
    let e = entry("redaction", Some("redacted"));
    assert!(matches!(decision_for(&e), Some(Decision::Sealed)));
}

#[test]
fn sealed_policy_decision_is_pipeline_internal() {
    let e = entry("policy_decision", Some("redact"));
    assert!(decision_for(&e).is_none());
}

#[test]
fn decision_block_is_denied() {
    let e = entry("policy_decision", Some("block"));
    assert!(matches!(decision_for(&e), Some(Decision::Denied)));
}

#[test]
fn non_user_event_returns_none() {
    let e = entry("vault_write", Some("ok"));
    assert!(decision_for(&e).is_none());
}

#[test]
fn proxy_request_summary_is_not_activity() {
    let e = dam_log::LogEntry {
        id: 1,
        timestamp: 0,
        operation_id: "op-1".into(),
        level: "info".into(),
        event_type: "proxy_forward".into(),
        kind: None,
        value: None,
        reference: None,
        action: Some("request_protection".into()),
        message: "request protection detections=0 replacements=0 tokenized=0 blocked=0".into(),
    };

    assert!(derive_event_with_actor(&e, Some("openai")).is_none());
    assert!(decision_for(&e).is_none());
}

#[test]
fn actor_can_be_derived_from_route_message() {
    assert_eq!(
        actor_from_message("route target=openai provider=openai-compatible request_bytes=10"),
        Some("openai".to_string())
    );
}

#[test]
fn day_label_is_iso_date() {
    // 2026-05-07 ≈ epoch 1_777_276_800
    let label = day_label(1_777_276_800);
    assert!(label.starts_with("2026-"));
    assert_eq!(label.len(), 10);
}
