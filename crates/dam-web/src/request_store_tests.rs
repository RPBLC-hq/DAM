use super::*;

#[test]
fn trigger_adds_pending_request_and_marks_protected() {
    let store = RequestStore::default();

    let request = store.trigger(TriggerRequest {
        actor: Some("anthropic".to_string()),
        value_label: Some("mobile phone".to_string()),
        value_preview: None,
        purpose: Some("confirm a wire".to_string()),
        expires_in_sec: Some(18_000),
    });

    assert!(store.is_protected());
    assert_eq!(request.expires_in_sec, 18_000);
    assert_eq!(store.pending().len(), 1);
}

#[test]
fn resolve_removes_only_matching_request() {
    let store = RequestStore::default();
    let first = store.trigger(TriggerRequest::default());
    let second = store.trigger(TriggerRequest::default());

    assert_eq!(
        store.resolve(&first.id).map(|request| request.id),
        Some(first.id)
    );

    let pending = store.pending();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, second.id);
}
