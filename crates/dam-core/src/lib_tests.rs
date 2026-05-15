use super::*;
use std::sync::Mutex;

struct RecordingVault {
    records: Mutex<Vec<VaultRecord>>,
}

impl RecordingVault {
    fn new() -> Self {
        Self {
            records: Mutex::new(Vec::new()),
        }
    }
}

impl VaultWriter for RecordingVault {
    fn write_with_options(
        &self,
        record: &VaultRecord,
        _options: VaultWriteOptions,
    ) -> Result<Reference, VaultWriteError> {
        self.records.lock().unwrap().push(record.clone());
        Ok(record.reference.clone())
    }
}

impl VaultReader for RecordingVault {
    fn read(&self, reference: &Reference) -> Result<Option<String>, VaultReadError> {
        Ok(self
            .records
            .lock()
            .unwrap()
            .iter()
            .find(|record| record.reference == *reference)
            .map(|record| record.value.clone()))
    }
}

struct FailingVault;

impl VaultWriter for FailingVault {
    fn write_with_options(
        &self,
        _record: &VaultRecord,
        _options: VaultWriteOptions,
    ) -> Result<Reference, VaultWriteError> {
        Err(VaultWriteError::new("vault unavailable"))
    }
}

struct CanonicalVault {
    reference: Reference,
    records: Mutex<Vec<VaultRecord>>,
}

impl VaultWriter for CanonicalVault {
    fn write_with_options(
        &self,
        record: &VaultRecord,
        _options: VaultWriteOptions,
    ) -> Result<Reference, VaultWriteError> {
        self.records.lock().unwrap().push(record.clone());
        Ok(self.reference.clone())
    }
}

impl VaultReader for FailingVault {
    fn read(&self, _reference: &Reference) -> Result<Option<String>, VaultReadError> {
        Err(VaultReadError::new("vault unavailable"))
    }
}

fn detection(kind: SensitiveType, value: &str, start: usize, end: usize) -> Detection {
    Detection {
        kind,
        value: value.to_string(),
        span: Span { start, end },
    }
}

#[test]
fn generated_references_use_standard_format() {
    let reference = Reference::generate(SensitiveType::Email);

    assert_eq!(reference.kind, SensitiveType::Email);
    assert_eq!(reference.id.len(), 22);
    assert_eq!(reference.key().len(), "email:".len() + 22);
    assert!(reference.display().starts_with("[email:"));
    assert!(reference.display().ends_with(']'));
}

#[test]
fn replacement_plan_saves_records_and_uses_references() {
    let vault = RecordingVault::new();
    let detections = [detection(SensitiveType::Email, "alice@example.com", 6, 23)];

    let plan = build_replacement_plan(&detections, &vault);

    assert_eq!(plan.tokenized_count(), 1);
    assert_eq!(plan.fallback_count(), 0);
    assert_eq!(plan.vault_failures.len(), 0);
    let records = vault.records.lock().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].value, "alice@example.com");
    assert_eq!(records[0].kind, SensitiveType::Email);
    assert!(plan.replacements[0].text.starts_with("[email:"));
}

#[test]
fn replacement_plan_reuses_references_for_duplicate_values_by_default() {
    let vault = RecordingVault::new();
    let detections = [
        detection(SensitiveType::Email, "alice@example.com", 6, 23),
        detection(SensitiveType::Email, "alice@example.com", 28, 45),
    ];

    let plan = build_replacement_plan(&detections, &vault);

    assert_eq!(plan.tokenized_count(), 2);
    assert_eq!(plan.vault_write_count(), 1);
    assert_eq!(vault.records.lock().unwrap().len(), 1);
    assert_eq!(
        plan.replacements[0].reference,
        plan.replacements[1].reference
    );
    assert_eq!(plan.replacements[0].text, plan.replacements[1].text);
}

#[test]
fn replacement_plan_deduplicates_canonical_email_values() {
    let vault = RecordingVault::new();
    let detections = [
        detection(SensitiveType::Email, "alice@example.COM", 6, 23),
        detection(SensitiveType::Email, "alice@ example.com", 28, 46),
        detection(SensitiveType::Email, "alice @example .com", 51, 70),
    ];

    let plan = build_replacement_plan(&detections, &vault);

    assert_eq!(plan.tokenized_count(), 3);
    assert_eq!(plan.vault_write_count(), 1);
    assert_eq!(vault.records.lock().unwrap().len(), 1);
    assert_eq!(vault.records.lock().unwrap()[0].value, "alice@example.com");
    assert_eq!(
        plan.replacements[0].reference,
        plan.replacements[1].reference
    );
    assert_eq!(
        plan.replacements[1].reference,
        plan.replacements[2].reference
    );
}

#[test]
fn replacement_plan_uses_canonical_reference_returned_by_vault() {
    let canonical = Reference::generate(SensitiveType::Email);
    let vault = CanonicalVault {
        reference: canonical.clone(),
        records: Mutex::new(Vec::new()),
    };
    let detections = [detection(SensitiveType::Email, "alice@example.com", 6, 23)];

    let plan = build_replacement_plan(&detections, &vault);

    assert_eq!(plan.tokenized_count(), 1);
    assert_eq!(plan.replacements[0].reference, Some(canonical.clone()));
    assert_eq!(plan.replacements[0].text, canonical.display());
    assert_eq!(vault.records.lock().unwrap().len(), 1);
}

#[test]
fn replacement_plan_can_disable_duplicate_value_reuse() {
    let vault = RecordingVault::new();
    let detections = [
        detection(SensitiveType::Email, "alice@example.com", 6, 23),
        detection(SensitiveType::Email, "alice@example.com", 28, 45),
    ];

    let plan = build_replacement_plan_with_options(
        &detections,
        &vault,
        ReplacementPlanOptions {
            deduplicate_replacements: false,
        },
    );

    assert_eq!(plan.tokenized_count(), 2);
    assert_eq!(plan.vault_write_count(), 2);
    assert_eq!(vault.records.lock().unwrap().len(), 2);
    assert_ne!(
        plan.replacements[0].reference,
        plan.replacements[1].reference
    );
    assert_ne!(plan.replacements[0].text, plan.replacements[1].text);
}

#[test]
fn replacement_plan_deduplicates_vault_failures_for_duplicate_values() {
    let detections = [
        detection(SensitiveType::Email, "alice@example.com", 6, 23),
        detection(SensitiveType::Email, "alice@example.com", 28, 45),
    ];

    let plan = build_replacement_plan(&detections, &FailingVault);

    assert_eq!(plan.tokenized_count(), 0);
    assert_eq!(plan.fallback_count(), 2);
    assert_eq!(plan.vault_failures.len(), 1);
    assert_eq!(plan.replacements[0].text, "[email]");
    assert_eq!(plan.replacements[1].text, "[email]");
}

#[test]
fn replacement_plan_uses_redact_only_fallback_on_vault_error() {
    let detections = [detection(SensitiveType::Email, "alice@example.com", 6, 23)];

    let plan = build_replacement_plan(&detections, &FailingVault);

    assert_eq!(plan.tokenized_count(), 0);
    assert_eq!(plan.fallback_count(), 1);
    assert_eq!(plan.replacements[0].text, "[email]");
    assert_eq!(plan.replacements[0].reference, None);
    assert_eq!(plan.vault_failures.len(), 1);
    assert_eq!(plan.vault_failures[0].value_preview, "alic...");
}

#[test]
fn replacement_plan_redacts_without_vault_write_when_policy_says_redact() {
    let vault = RecordingVault::new();
    let decisions = [PolicyDecision::new(
        detection(SensitiveType::Email, "alice@example.com", 6, 23),
        PolicyAction::Redact,
    )];

    let plan = build_replacement_plan_from_decisions(&decisions, &vault);

    assert_eq!(plan.tokenized_count(), 0);
    assert_eq!(plan.redacted_count(), 1);
    assert_eq!(plan.replacements[0].text, "[email]");
    assert_eq!(vault.records.lock().unwrap().len(), 0);
}

#[test]
fn replacement_plan_allows_without_replacement_or_vault_write() {
    let vault = RecordingVault::new();
    let decisions = [PolicyDecision::new(
        detection(SensitiveType::Email, "alice@example.com", 6, 23),
        PolicyAction::Allow,
    )];

    let plan = build_replacement_plan_from_decisions(&decisions, &vault);

    assert_eq!(plan.replacements.len(), 0);
    assert_eq!(plan.blocked_count(), 0);
    assert_eq!(vault.records.lock().unwrap().len(), 0);
}

#[test]
fn replacement_plan_tracks_blocked_detections() {
    let vault = RecordingVault::new();
    let decisions = [PolicyDecision::new(
        detection(SensitiveType::Ssn, "123-45-6789", 6, 17),
        PolicyAction::Block,
    )];

    let plan = build_replacement_plan_from_decisions(&decisions, &vault);

    assert_eq!(plan.replacements.len(), 0);
    assert_eq!(plan.blocked_count(), 1);
    assert_eq!(plan.blocked[0].kind, SensitiveType::Ssn);
    assert_eq!(vault.records.lock().unwrap().len(), 0);
}

#[test]
fn generated_operation_ids_use_standard_length() {
    assert_eq!(generate_operation_id().len(), 22);
}

#[test]
fn filter_log_events_do_not_include_raw_values() {
    let detections = [detection(SensitiveType::Email, "alice@example.com", 6, 23)];
    let plan = build_replacement_plan(&detections, &RecordingVault::new());

    let events = build_filter_log_events("op-1", &detections, &plan);

    assert_eq!(events.len(), 4);
    assert!(
        events
            .iter()
            .any(|event| event.event_type == LogEventType::Detection)
    );
    assert!(
        events
            .iter()
            .any(|event| event.event_type == LogEventType::PolicyDecision)
    );
    assert!(
        events
            .iter()
            .any(|event| event.event_type == LogEventType::VaultWrite)
    );
    assert!(
        events
            .iter()
            .any(|event| event.event_type == LogEventType::Redaction)
    );

    for event in events {
        assert!(!event.message.contains("alice@example.com"));
        assert!(!event.operation_id.contains("alice@example.com"));
        assert!(
            !event
                .action
                .unwrap_or_default()
                .contains("alice@example.com")
        );
    }
}

#[test]
fn filter_log_events_log_deduplicated_vault_write_once() {
    let detections = [
        detection(SensitiveType::Email, "alice@example.com", 6, 23),
        detection(SensitiveType::Email, "alice@example.com", 28, 45),
    ];
    let plan = build_replacement_plan(&detections, &RecordingVault::new());

    let events = build_filter_log_events("op-1", &detections, &plan);

    assert_eq!(
        events
            .iter()
            .filter(|event| event.event_type == LogEventType::VaultWrite)
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.event_type == LogEventType::Redaction)
            .count(),
        2
    );
}

#[test]
fn reference_parse_round_trips_generated_reference() {
    let reference = Reference::generate(SensitiveType::Email);

    assert_eq!(
        Reference::parse_key(&reference.key()),
        Some(reference.clone())
    );
    assert_eq!(
        Reference::parse_display(&reference.display()),
        Some(reference)
    );
}

#[test]
fn reference_parse_rejects_redact_only_and_malformed_values() {
    assert_eq!(Reference::parse_display("[email]"), None);
    assert_eq!(
        Reference::parse_display("[unknown:7B2HkqFn9xR4mWpD3nYvKt]"),
        None
    );
    assert_eq!(Reference::parse_display("[email:not-a-valid-id]"), None);
    assert_eq!(Reference::parse_key("email:short"), None);
}

#[test]
fn find_references_ignores_malformed_and_redact_only_placeholders() {
    let reference = Reference::generate(SensitiveType::Email);
    let input = format!("a [email] b {} c [ssn:not-valid] d", reference.display());

    let matches = find_references(&input);

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].reference, reference);
    assert_eq!(
        &input[matches[0].span.start..matches[0].span.end],
        reference.display()
    );
}

#[test]
fn find_references_detects_token_nested_after_json_array_bracket() {
    let reference = Reference::generate(SensitiveType::Email);
    let input = format!(
        r#"{{"messages":[{{"content":"email {}"}}]}}"#,
        reference.display()
    );

    let matches = find_references(&input);

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].reference, reference);
}

#[test]
fn find_references_detects_markdown_escaped_token_references() {
    let reference = Reference::generate(SensitiveType::Email);
    let input = format!(
        r#"reply \{} and again \[email:not-valid\]"#,
        reference.display()
    );

    let matches = find_references(&input);

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].reference, reference);
    assert_eq!(
        &input[matches[0].span.start..matches[0].span.end],
        format!(r#"\{}"#, reference.display())
    );
}

#[test]
fn resolve_plan_restores_known_references_and_leaves_missing_unresolved() {
    let vault = RecordingVault::new();
    let known = Reference::generate(SensitiveType::Email);
    let missing = Reference::generate(SensitiveType::Ssn);
    vault
        .write(&VaultRecord {
            reference: known.clone(),
            kind: SensitiveType::Email,
            value: "alice@example.com".to_string(),
        })
        .unwrap();
    let input = format!("known {} missing {}", known.display(), missing.display());

    let plan = build_resolve_plan(&input, &vault);
    let output = apply_resolve_plan(&input, &plan);

    assert_eq!(plan.references.len(), 2);
    assert_eq!(plan.resolved_count(), 1);
    assert_eq!(plan.missing_count(), 1);
    assert_eq!(
        output,
        format!("known alice@example.com missing {}", missing.display())
    );
}

#[test]
fn resolve_plan_restores_markdown_escaped_references() {
    let vault = RecordingVault::new();
    let reference = Reference::generate(SensitiveType::Email);
    vault
        .write(&VaultRecord {
            reference: reference.clone(),
            kind: SensitiveType::Email,
            value: "alice@example.com".to_string(),
        })
        .unwrap();
    let input = format!(r#"known \{}"#, reference.display());

    let plan = build_resolve_plan(&input, &vault);
    let output = apply_resolve_plan(&input, &plan);

    assert_eq!(plan.references.len(), 1);
    assert_eq!(plan.resolved_count(), 1);
    assert_eq!(output, "known alice@example.com");
}

#[test]
fn resolve_plan_records_read_failures_without_replacement() {
    let reference = Reference::generate(SensitiveType::Email);
    let input = format!("email {}", reference.display());

    let plan = build_resolve_plan(&input, &FailingVault);

    assert_eq!(plan.resolved_count(), 0);
    assert_eq!(plan.read_failure_count(), 1);
    assert!(plan.has_unresolved());
    assert_eq!(apply_resolve_plan(&input, &plan), input);
}

#[test]
fn resolve_log_events_do_not_include_resolved_raw_values() {
    let vault = RecordingVault::new();
    let reference = Reference::generate(SensitiveType::Email);
    vault
        .write(&VaultRecord {
            reference: reference.clone(),
            kind: SensitiveType::Email,
            value: "alice@example.com".to_string(),
        })
        .unwrap();
    let plan = build_resolve_plan(&reference.display(), &vault);

    let events = build_resolve_log_events("op-1", &plan);

    assert_eq!(events.len(), 2);
    assert!(
        events
            .iter()
            .any(|event| event.event_type == LogEventType::VaultRead)
    );
    assert!(
        events
            .iter()
            .any(|event| event.event_type == LogEventType::Resolve)
    );
    for event in events {
        assert!(!event.message.contains("alice@example.com"));
        assert!(
            !event
                .action
                .unwrap_or_default()
                .contains("alice@example.com")
        );
    }
}
