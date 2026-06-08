use super::*;
use dam_core::Span;

fn detection(kind: SensitiveType) -> Detection {
    Detection {
        kind,
        span: Span { start: 0, end: 5 },
        value: "value".to_string(),
    }
}

#[test]
fn default_action_applies_when_no_kind_override_exists() {
    let policy = StaticPolicy::new(PolicyAction::Tokenize);

    let decision = policy.decide(&detection(SensitiveType::Email));

    assert_eq!(decision.action, PolicyAction::Tokenize);
}

#[test]
fn kind_action_overrides_default() {
    let policy = StaticPolicy::new(PolicyAction::Tokenize)
        .with_kind_action(SensitiveType::Ssn, PolicyAction::Redact);

    let decision = policy.decide(&detection(SensitiveType::Ssn));

    assert_eq!(decision.action, PolicyAction::Redact);
}

#[test]
fn decide_all_preserves_order() {
    let policy = StaticPolicy::new(PolicyAction::Allow)
        .with_kind_action(SensitiveType::CreditCard, PolicyAction::Block);
    let detections = [
        detection(SensitiveType::Email),
        detection(SensitiveType::CreditCard),
    ];

    let decisions = policy.decide_all(&detections);

    assert_eq!(decisions[0].action, PolicyAction::Allow);
    assert_eq!(decisions[1].action, PolicyAction::Block);
}
