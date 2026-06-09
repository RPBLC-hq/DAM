use super::*;

fn detection(kind: dam_core::SensitiveType, value: &str) -> dam_core::Detection {
    dam_core::Detection {
        kind,
        value: value.to_string(),
        span: dam_core::Span {
            start: 0,
            end: value.len(),
        },
    }
}

#[test]
fn filter_report_does_not_serialize_raw_detection_values_or_previews() {
    let decision = dam_core::PolicyDecision::new(
        detection(dam_core::SensitiveType::Email, "alice@example.com"),
        dam_core::PolicyAction::Tokenize,
    );
    let plan = dam_core::ReplacementPlan {
        replacements: vec![dam_core::Replacement {
            span: decision.detection.span,
            text: "[email]".to_string(),
            mode: dam_core::ReplacementMode::RedactOnlyFallback,
            reference: None,
        }],
        vault_failures: vec![dam_core::VaultFailure {
            kind: dam_core::SensitiveType::Email,
            value_preview: "alic...".to_string(),
            error: "vault unavailable for alice@example.com".to_string(),
        }],
        blocked: Vec::new(),
    };

    let report = filter_report_from_decisions("op-1", &[decision], &plan);
    let json = serde_json::to_string(&report).unwrap();

    assert_eq!(report.summary.fallback_redactions, 1);
    assert_eq!(
        report.vault_failures[0].error,
        VAULT_WRITE_FAILURE_REPORT_ERROR
    );
    assert!(!json.contains("alice@example.com"));
    assert!(!json.contains("alic..."));
    assert!(!json.contains("vault unavailable"));
}

#[test]
fn resolve_report_marks_strict_unresolved_as_failed_strict() {
    let reference = dam_core::Reference::generate(dam_core::SensitiveType::Ssn);
    let plan = dam_core::ResolvePlan {
        references: vec![dam_core::ReferenceMatch {
            span: dam_core::Span { start: 0, end: 1 },
            reference: reference.clone(),
        }],
        missing: vec![dam_core::MissingReference {
            span: dam_core::Span { start: 0, end: 1 },
            reference,
        }],
        ..dam_core::ResolvePlan::default()
    };

    let report = resolve_report("op-1", &plan, true);

    assert_eq!(report.status, ResolveStatus::FailedStrict);
    assert_eq!(report.summary.references, 1);
    assert_eq!(report.summary.missing, 1);
}

#[test]
fn resolve_report_does_not_serialize_vault_read_error_details() {
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

    let report = resolve_report("op-1", &plan, false);
    let json = serde_json::to_string(&report).unwrap();

    assert_eq!(
        report.read_failures[0].error,
        VAULT_READ_FAILURE_REPORT_ERROR
    );
    assert!(report.diagnostics[0].message.contains("vault read failed"));
    assert!(!json.contains("backend echoed"));
    assert!(!json.contains("alice@example.com"));
}

#[test]
fn credit_card_kind_serializes_as_reference_tag() {
    let json = serde_json::to_string(&SensitiveKind::CreditCard).unwrap();

    assert_eq!(json, r#""cc""#);
}

#[test]
fn api_key_kind_serializes_as_snake_case() {
    let json =
        serde_json::to_string(&SensitiveKind::from(dam_core::SensitiveType::ApiKey)).unwrap();

    assert_eq!(json, r#""api_key""#);
}
