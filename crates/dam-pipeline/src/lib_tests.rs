use super::*;
use dam_core::{
    Detection, LogEvent, LogWriteError, Reference, SensitiveType, Span, VaultReadError,
    VaultRecord, VaultWriteError,
};
use std::sync::Mutex;

#[derive(Default)]
struct RecordingVault {
    records: Mutex<Vec<VaultRecord>>,
}

impl VaultWriter for RecordingVault {
    fn write_with_options(
        &self,
        record: &VaultRecord,
        _options: dam_core::VaultWriteOptions,
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
            .find(|record| &record.reference == reference)
            .map(|record| record.value.clone()))
    }
}

#[derive(Default)]
struct RecordingSink {
    events: Mutex<Vec<LogEvent>>,
}

impl EventSink for RecordingSink {
    fn record(&self, event: &LogEvent) -> Result<(), LogWriteError> {
        self.events.lock().unwrap().push(event.clone());
        Ok(())
    }
}

fn detection(value: &str) -> Detection {
    Detection {
        kind: SensitiveType::Email,
        span: Span {
            start: 6,
            end: 6 + value.len(),
        },
        value: value.to_string(),
    }
}

#[test]
fn protect_text_tokenizes_and_logs_without_raw_values() {
    let vault = RecordingVault::default();
    let sink = RecordingSink::default();
    let policy = dam_policy::StaticPolicy::new(PolicyAction::Tokenize);

    let result = protect_text(
        "email alice@example.com",
        "op-test",
        &policy,
        &vault,
        ProtectTextContext {
            reference_vault: Some(&vault),
            event_sink: Some(&sink),
            ..ProtectTextContext::default()
        },
        ReplacementPlanOptions::default(),
    )
    .unwrap();

    assert_eq!(result.status, ProtectTextStatus::Protected);
    let output = result.output.unwrap();
    assert!(!output.contains("alice@example.com"));
    assert!(output.contains("[email:"));
    assert_eq!(vault.records.lock().unwrap().len(), 1);

    let event_text = sink
        .events
        .lock()
        .unwrap()
        .iter()
        .map(|event| event.message.clone())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!event_text.contains("alice@example.com"));
}

#[test]
fn protect_text_blocks_before_vault_write() {
    let vault = RecordingVault::default();
    let sink = RecordingSink::default();
    let policy = dam_policy::StaticPolicy::new(PolicyAction::Block);

    let result = protect_text(
        "email alice@example.com",
        "op-test",
        &policy,
        &vault,
        ProtectTextContext {
            reference_vault: Some(&vault),
            event_sink: Some(&sink),
            ..ProtectTextContext::default()
        },
        ReplacementPlanOptions::default(),
    )
    .unwrap();

    assert!(result.is_blocked());
    assert!(result.output.is_none());
    assert_eq!(result.plan.blocked_count(), 1);
    assert!(vault.records.lock().unwrap().is_empty());
}

#[test]
fn protect_text_applies_active_consent() {
    let vault = RecordingVault::default();
    let sink = RecordingSink::default();
    let consent_store = dam_consent::ConsentStore::open_in_memory().unwrap();
    consent_store
        .grant(&dam_consent::GrantConsent {
            kind: SensitiveType::Email,
            value: "alice@example.com".to_string(),
            vault_key: None,
            ttl_seconds: 60,
            created_by: "test".to_string(),
            reason: None,
        })
        .unwrap();
    let policy = dam_policy::StaticPolicy::new(PolicyAction::Tokenize);

    let result = protect_text(
        "email alice@example.com",
        "op-test",
        &policy,
        &vault,
        ProtectTextContext {
            reference_vault: Some(&vault),
            consent_store: Some(&consent_store),
            event_sink: Some(&sink),
            ..ProtectTextContext::default()
        },
        ReplacementPlanOptions::default(),
    )
    .unwrap();

    assert_eq!(result.output.unwrap(), "email alice@example.com");
    assert_eq!(result.consent_matches.len(), 1);
    assert!(vault.records.lock().unwrap().is_empty());
    assert!(sink.events.lock().unwrap().iter().any(|event| {
        event.event_type == LogEventType::Consent
            && event
                .action
                .as_deref()
                .is_some_and(|action| action.starts_with("allow:"))
    }));
}

#[test]
fn protect_text_applies_scoped_consent_only_for_matching_scope() {
    let vault = RecordingVault::default();
    let consent_store = dam_consent::ConsentStore::open_in_memory().unwrap();
    consent_store
        .grant_scoped(
            &dam_consent::GrantConsent {
                kind: SensitiveType::Email,
                value: "alice@example.com".to_string(),
                vault_key: None,
                ttl_seconds: 60,
                created_by: "Codex".to_string(),
                reason: None,
            },
            dam_consent::target_scope("chatgpt-codex"),
        )
        .unwrap();
    let policy = dam_policy::StaticPolicy::new(PolicyAction::Tokenize);
    let matching_scopes = vec![dam_consent::target_scope("chatgpt-codex")];
    let non_matching_scopes = vec![dam_consent::target_scope("anthropic")];

    let matching = protect_text(
        "email alice@example.com",
        "op-test",
        &policy,
        &vault,
        ProtectTextContext {
            consent_store: Some(&consent_store),
            consent_scopes: &matching_scopes,
            ..ProtectTextContext::default()
        },
        ReplacementPlanOptions::default(),
    )
    .unwrap();
    let non_matching = protect_text(
        "email alice@example.com",
        "op-test",
        &policy,
        &vault,
        ProtectTextContext {
            consent_store: Some(&consent_store),
            consent_scopes: &non_matching_scopes,
            ..ProtectTextContext::default()
        },
        ReplacementPlanOptions::default(),
    )
    .unwrap();

    assert_eq!(matching.output.unwrap(), "email alice@example.com");
    assert_ne!(non_matching.output.unwrap(), "email alice@example.com");
}

#[test]
fn protect_text_applies_active_consent_to_allowed_reference_history() {
    let vault = RecordingVault::default();
    let sink = RecordingSink::default();
    let reference = Reference::generate(SensitiveType::Email);
    vault
        .write(&VaultRecord {
            reference: reference.clone(),
            kind: SensitiveType::Email,
            value: "alice@example.com".to_string(),
        })
        .unwrap();
    let consent_store = dam_consent::ConsentStore::open_in_memory().unwrap();
    consent_store
        .grant(&dam_consent::GrantConsent {
            kind: SensitiveType::Email,
            value: "alice@example.com".to_string(),
            vault_key: Some(reference.key()),
            ttl_seconds: 60,
            created_by: "test".to_string(),
            reason: None,
        })
        .unwrap();
    let policy = dam_policy::StaticPolicy::new(PolicyAction::Tokenize);
    let input = format!("email {}", reference.display());

    let result = protect_text(
        &input,
        "op-test",
        &policy,
        &vault,
        ProtectTextContext {
            reference_vault: Some(&vault),
            consent_store: Some(&consent_store),
            event_sink: Some(&sink),
            ..ProtectTextContext::default()
        },
        ReplacementPlanOptions::default(),
    )
    .unwrap();

    assert_eq!(result.output.unwrap(), "email alice@example.com");
    assert_eq!(result.consent_matches.len(), 1);
    assert_eq!(vault.records.lock().unwrap().len(), 1);
    assert!(sink.events.lock().unwrap().iter().any(|event| {
        event.event_type == LogEventType::Consent
            && event
                .action
                .as_deref()
                .is_some_and(|action| action.starts_with("allow:"))
    }));
}

#[test]
fn protect_text_expands_allowed_references_for_matching_scope() {
    let vault = RecordingVault::default();
    let reference = Reference::generate(SensitiveType::Email);
    vault
        .write(&VaultRecord {
            reference: reference.clone(),
            kind: SensitiveType::Email,
            value: "alice@example.com".to_string(),
        })
        .unwrap();
    let consent_store = dam_consent::ConsentStore::open_in_memory().unwrap();
    consent_store
        .grant_scoped(
            &dam_consent::GrantConsent {
                kind: SensitiveType::Email,
                value: "alice@example.com".to_string(),
                vault_key: Some(reference.key()),
                ttl_seconds: 60,
                created_by: "Claude Code".to_string(),
                reason: None,
            },
            dam_consent::target_scope("anthropic"),
        )
        .unwrap();
    let policy = dam_policy::StaticPolicy::new(PolicyAction::Tokenize);
    let scopes = vec![dam_consent::target_scope("anthropic")];
    let input = format!("email {}", reference.display());

    let result = protect_text(
        &input,
        "op-test",
        &policy,
        &vault,
        ProtectTextContext {
            reference_vault: Some(&vault),
            consent_store: Some(&consent_store),
            consent_scopes: &scopes,
            ..ProtectTextContext::default()
        },
        ReplacementPlanOptions::default(),
    )
    .unwrap();

    assert_eq!(result.output.unwrap(), "email alice@example.com");
}

#[test]
fn protect_text_tokenizes_related_domain_without_email_in_input() {
    let vault = RecordingVault::default();
    let policy = dam_policy::StaticPolicy::new(PolicyAction::Tokenize);
    let related_domains = vec!["example.com".to_string()];

    let result = protect_text(
        "domain example.com",
        "op-test",
        &policy,
        &vault,
        ProtectTextContext {
            related_domains: &related_domains,
            ..ProtectTextContext::default()
        },
        ReplacementPlanOptions::default(),
    )
    .unwrap();

    let output = result.output.unwrap();
    assert!(!output.contains("example.com"));
    assert!(output.contains("[domain:"));
    assert_eq!(result.detections.len(), 1);
    assert_eq!(result.detections[0].kind, SensitiveType::Domain);
    assert_eq!(vault.records.lock().unwrap()[0].value, "example.com");
}

#[test]
fn resolve_text_restores_known_references_and_logs() {
    let vault = RecordingVault::default();
    let sink = RecordingSink::default();
    let reference = Reference::generate(SensitiveType::Email);
    vault
        .write(&VaultRecord {
            reference: reference.clone(),
            kind: SensitiveType::Email,
            value: "alice@example.com".to_string(),
        })
        .unwrap();
    let input = format!("email {}", reference.display());

    let result = resolve_text(&input, "op-test", &vault, Some(&sink));

    assert_eq!(result.output.unwrap(), "email alice@example.com");
    assert_eq!(result.plan.resolved_count(), 1);
    assert!(sink.events.lock().unwrap().iter().any(|event| {
        event.event_type == LogEventType::Resolve && event.action.as_deref() == Some("resolved")
    }));
}

#[test]
fn resolve_text_restores_markdown_escaped_references_and_logs() {
    let vault = RecordingVault::default();
    let sink = RecordingSink::default();
    let reference = Reference::generate(SensitiveType::Email);
    vault
        .write(&VaultRecord {
            reference: reference.clone(),
            kind: SensitiveType::Email,
            value: "alice@example.com".to_string(),
        })
        .unwrap();
    let input = format!(r#"email \{}"#, reference.display());

    let result = resolve_text(&input, "op-test", &vault, Some(&sink));

    assert_eq!(result.output.unwrap(), "email alice@example.com");
    assert_eq!(result.plan.resolved_count(), 1);
    assert!(sink.events.lock().unwrap().iter().any(|event| {
        event.event_type == LogEventType::VaultRead && event.reference.as_ref() == Some(&reference)
    }));
}

#[test]
fn resolve_text_leaves_unresolved_output_empty_but_logs_reference() {
    let vault = RecordingVault::default();
    let sink = RecordingSink::default();
    let reference = Reference::generate(SensitiveType::Email);
    let input = format!("email {}", reference.display());

    let result = resolve_text(&input, "op-test", &vault, Some(&sink));

    assert!(result.output.is_none());
    assert_eq!(result.plan.resolved_count(), 0);
    assert_eq!(result.plan.missing.len(), 1);
    assert!(sink.events.lock().unwrap().iter().any(|event| {
        event.event_type == LogEventType::Resolve && event.action.as_deref() == Some("missing")
    }));
}

#[test]
fn blocked_plan_contains_only_blocked_decisions() {
    let decisions = [
        PolicyDecision::new(detection("alice@example.com"), PolicyAction::Block),
        PolicyDecision::new(detection("bob@example.com"), PolicyAction::Tokenize),
    ];

    let plan = blocked_plan_from_decisions(&decisions);

    assert_eq!(plan.blocked_count(), 1);
}
