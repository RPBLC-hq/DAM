use super::*;
use dam_core::{Reference, SensitiveType, VaultReadError, VaultReader, VaultRecord, VaultWriter};
use std::{sync::Arc, thread, time::Duration};

struct DelayedVaultReader {
    inner: Arc<dam_vault::Vault>,
    delay: Duration,
}

impl VaultReader for DelayedVaultReader {
    fn read(&self, reference: &Reference) -> Result<Option<String>, VaultReadError> {
        thread::sleep(self.delay);
        self.inner.read(reference)
    }
}

#[test]
fn grant_and_match_active_value() {
    let store = ConsentStore::open_in_memory().unwrap();
    let entry = store
        .grant(&GrantConsent {
            kind: SensitiveType::Email,
            value: "alice@example.test".to_string(),
            vault_key: None,
            ttl_seconds: 60,
            created_by: "test".to_string(),
            reason: None,
        })
        .unwrap();

    let matched = store
        .active_for_value(SensitiveType::Email, "alice@example.test")
        .unwrap()
        .unwrap();

    assert_eq!(matched.id, entry.id);
    assert_eq!(store.count().unwrap(), 1);
}

#[test]
fn grant_matches_canonical_email_variants() {
    let store = ConsentStore::open_in_memory().unwrap();
    let entry = store
        .grant(&GrantConsent {
            kind: SensitiveType::Email,
            value: "alice@example.com".to_string(),
            vault_key: None,
            ttl_seconds: 60,
            created_by: "test".to_string(),
            reason: None,
        })
        .unwrap();

    let matched = store
        .active_for_value(SensitiveType::Email, "alice@ example.COM")
        .unwrap()
        .unwrap();

    assert_eq!(matched.id, entry.id);
}

#[test]
fn scoped_grant_matches_only_when_scope_is_requested() {
    let store = ConsentStore::open_in_memory().unwrap();
    let entry = store
        .grant_scoped(
            &GrantConsent {
                kind: SensitiveType::Email,
                value: "alice@example.test".to_string(),
                vault_key: None,
                ttl_seconds: 60,
                created_by: "Codex".to_string(),
                reason: None,
            },
            target_scope("chatgpt-web"),
        )
        .unwrap();

    assert_eq!(entry.scope, "target:chatgpt-web");
    assert!(
        store
            .active_for_value(SensitiveType::Email, "alice@example.test")
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .active_for_value_in_scopes(
                SensitiveType::Email,
                "alice@example.test",
                &["target:chatgpt-web".to_string()],
            )
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .active_for_value_in_scopes(
                SensitiveType::Email,
                "alice@example.test",
                &["target:anthropic".to_string()],
            )
            .unwrap()
            .is_none()
    );
}

#[test]
fn global_grant_matches_scoped_lookup() {
    let store = ConsentStore::open_in_memory().unwrap();
    store
        .grant(&GrantConsent {
            kind: SensitiveType::Email,
            value: "alice@example.test".to_string(),
            vault_key: None,
            ttl_seconds: 60,
            created_by: "All profiles".to_string(),
            reason: None,
        })
        .unwrap();

    assert!(
        store
            .active_for_value_in_scopes(
                SensitiveType::Email,
                "alice@example.test",
                &["target:chatgpt-web".to_string()],
            )
            .unwrap()
            .is_some()
    );
}

#[test]
fn revoked_consent_does_not_match() {
    let store = ConsentStore::open_in_memory().unwrap();
    let entry = store
        .grant(&GrantConsent {
            kind: SensitiveType::Email,
            value: "alice@example.test".to_string(),
            vault_key: None,
            ttl_seconds: 60,
            created_by: "test".to_string(),
            reason: None,
        })
        .unwrap();

    assert!(store.revoke(&entry.id).unwrap());

    assert!(
        store
            .active_for_value(SensitiveType::Email, "alice@example.test")
            .unwrap()
            .is_none()
    );
}

#[test]
fn revoke_stops_all_active_grants_for_same_exact_value() {
    let store = ConsentStore::open_in_memory().unwrap();
    let first = store
        .grant(&GrantConsent {
            kind: SensitiveType::Email,
            value: "alice@example.test".to_string(),
            vault_key: Some("email:first".to_string()),
            ttl_seconds: 60,
            created_by: "test".to_string(),
            reason: None,
        })
        .unwrap();
    store
        .grant(&GrantConsent {
            kind: SensitiveType::Email,
            value: "alice@example.test".to_string(),
            vault_key: Some("email:second".to_string()),
            ttl_seconds: 60,
            created_by: "test".to_string(),
            reason: None,
        })
        .unwrap();

    assert!(store.revoke(&first.id).unwrap());

    assert!(
        store
            .active_for_value(SensitiveType::Email, "alice@example.test")
            .unwrap()
            .is_none()
    );
    assert_eq!(
        store
            .list()
            .unwrap()
            .iter()
            .filter(|entry| entry.revoked_at.is_some())
            .count(),
        2
    );
}

#[test]
fn expired_consent_does_not_match() {
    let store = ConsentStore::open_in_memory().unwrap();
    store
        .grant(&GrantConsent {
            kind: SensitiveType::Email,
            value: "alice@example.test".to_string(),
            vault_key: None,
            ttl_seconds: 0,
            created_by: "test".to_string(),
            reason: None,
        })
        .unwrap();

    assert!(
        store
            .active_for_value(SensitiveType::Email, "alice@example.test")
            .unwrap()
            .is_none()
    );
}

#[test]
fn revoke_for_vault_key_can_target_one_party_or_everyone() {
    let store = ConsentStore::open_in_memory().unwrap();
    let vault_key = "email:1111111111111111111111";

    store
        .grant(&GrantConsent {
            kind: SensitiveType::Email,
            value: "alex@example.com".into(),
            vault_key: Some(vault_key.into()),
            ttl_seconds: 60,
            created_by: "Claude Code".into(),
            reason: None,
        })
        .unwrap();
    store
        .grant(&GrantConsent {
            kind: SensitiveType::Email,
            value: "alex@example.com".into(),
            vault_key: Some(vault_key.into()),
            ttl_seconds: 60,
            created_by: "Codex".into(),
            reason: None,
        })
        .unwrap();

    assert_eq!(
        store
            .revoke_for_vault_key_and_created_by(vault_key, "Claude Code")
            .unwrap(),
        1
    );
    let active = store
        .list()
        .unwrap()
        .into_iter()
        .filter(|entry| entry.revoked_at.is_none())
        .map(|entry| entry.created_by)
        .collect::<Vec<_>>();
    assert_eq!(active, vec!["Codex"]);

    assert_eq!(store.revoke_for_vault_key(vault_key).unwrap(), 1);
    assert!(
        store
            .list()
            .unwrap()
            .into_iter()
            .all(|entry| entry.revoked_at.is_some())
    );
}

#[test]
fn grants_from_vault_reference_without_storing_raw_value() {
    let vault = dam_vault::Vault::open_in_memory().unwrap();
    let store = ConsentStore::open_in_memory().unwrap();
    let reference = Reference::generate(SensitiveType::Email);
    vault
        .write(&VaultRecord {
            reference: reference.clone(),
            kind: SensitiveType::Email,
            value: "alice@example.test".to_string(),
        })
        .unwrap();

    let entry = store
        .grant_for_reference(&reference.key(), &vault, 60, "test", None)
        .unwrap();

    assert_eq!(entry.vault_key, Some(reference.key()));
    assert_ne!(entry.value_fingerprint, "alice@example.test");
    assert!(
        store
            .active_for_value(SensitiveType::Email, "alice@example.test")
            .unwrap()
            .is_some()
    );
}

#[test]
fn direct_access_request_requires_approval_and_consumes_on_resolve() {
    let vault = dam_vault::Vault::open_in_memory().unwrap();
    let store = ConsentStore::open_in_memory().unwrap();
    let reference = Reference::generate(SensitiveType::Email);
    vault
        .write(&VaultRecord {
            reference: reference.clone(),
            kind: SensitiveType::Email,
            value: "alice@example.test".to_string(),
        })
        .unwrap();

    let request = store
        .create_direct_access_request(
            &CreateDirectAccessRequest {
                vault_key: reference.key(),
                actor_id: "actor-1".to_string(),
                requesting_actor: "Codex".to_string(),
                purpose: "fill local email field".to_string(),
                reason: Some("needs raw value for local autofill".to_string()),
                requested_duration_seconds: 45,
                pending_timeout_seconds: 60,
                correlation_id: Some("corr-1".to_string()),
            },
            &vault,
        )
        .unwrap();

    let pending = store
        .resolve_direct_access_request(&request.request_id, "actor-1", &vault)
        .unwrap()
        .unwrap();
    assert_eq!(pending.request.status, DirectAccessStatus::Pending);
    assert_eq!(pending.value, None);

    let approved = store
        .approve_direct_access_request(&request.request_id, 45, Some("approved".to_string()))
        .unwrap()
        .unwrap();
    assert_eq!(approved.status, DirectAccessStatus::Approved);
    assert!(approved.grant_id.is_some());

    let resolved = store
        .resolve_direct_access_request(&request.request_id, "actor-1", &vault)
        .unwrap()
        .unwrap();
    assert_eq!(resolved.value.as_deref(), Some("alice@example.test"));
    assert_eq!(resolved.request.status, DirectAccessStatus::Consumed);
    assert_eq!(resolved.request.resolve_count, 1);

    let consumed = store
        .resolve_direct_access_request(&request.request_id, "actor-1", &vault)
        .unwrap()
        .unwrap();
    assert_eq!(consumed.value, None);
    assert_eq!(consumed.request.status, DirectAccessStatus::Consumed);
    assert_eq!(consumed.outcome_reason.as_deref(), Some("grant_consumed"));
}

#[test]
fn direct_access_request_denial_and_actor_mismatch_fail_closed() {
    let vault = dam_vault::Vault::open_in_memory().unwrap();
    let store = ConsentStore::open_in_memory().unwrap();
    let reference = Reference::generate(SensitiveType::Email);
    vault
        .write(&VaultRecord {
            reference: reference.clone(),
            kind: SensitiveType::Email,
            value: "alice@example.test".to_string(),
        })
        .unwrap();

    let request = store
        .create_direct_access_request(
            &CreateDirectAccessRequest {
                vault_key: reference.key(),
                actor_id: "actor-1".to_string(),
                requesting_actor: "Codex".to_string(),
                purpose: "fill local email field".to_string(),
                reason: None,
                requested_duration_seconds: 30,
                pending_timeout_seconds: 60,
                correlation_id: None,
            },
            &vault,
        )
        .unwrap();

    let approved = store
        .approve_direct_access_request(&request.request_id, 30, None)
        .unwrap()
        .unwrap();
    let mismatched = store
        .resolve_direct_access_request(&approved.request_id, "actor-2", &vault)
        .unwrap()
        .unwrap();
    assert_eq!(mismatched.value, None);
    assert_eq!(mismatched.outcome_reason.as_deref(), Some("actor_mismatch"));
    assert_eq!(mismatched.request.status, DirectAccessStatus::Approved);

    let denied = store
        .deny_direct_access_request(&approved.request_id, Some("user_denied".to_string()))
        .unwrap()
        .unwrap();
    assert_eq!(denied.status, DirectAccessStatus::Denied);

    let after_deny = store
        .resolve_direct_access_request(&approved.request_id, "actor-1", &vault)
        .unwrap()
        .unwrap();
    assert_eq!(after_deny.value, None);
    assert_eq!(after_deny.request.status, DirectAccessStatus::Denied);
    assert_eq!(after_deny.outcome_reason.as_deref(), Some("user_denied"));
}

#[test]
fn direct_access_request_revoke_if_vault_value_changes_before_resolve() {
    let vault = dam_vault::Vault::open_in_memory().unwrap();
    let store = ConsentStore::open_in_memory().unwrap();
    let reference = Reference::generate(SensitiveType::Email);
    vault
        .write(&VaultRecord {
            reference: reference.clone(),
            kind: SensitiveType::Email,
            value: "alice@example.test".to_string(),
        })
        .unwrap();

    let request = store
        .create_direct_access_request(
            &CreateDirectAccessRequest {
                vault_key: reference.key(),
                actor_id: "actor-1".to_string(),
                requesting_actor: "Codex".to_string(),
                purpose: "fill local email field".to_string(),
                reason: None,
                requested_duration_seconds: 60,
                pending_timeout_seconds: 60,
                correlation_id: None,
            },
            &vault,
        )
        .unwrap();
    store
        .approve_direct_access_request(&request.request_id, 60, Some("approved".to_string()))
        .unwrap()
        .unwrap();
    vault
        .write(&VaultRecord {
            reference: reference.clone(),
            kind: SensitiveType::Email,
            value: "mallory@example.test".to_string(),
        })
        .unwrap();

    let resolved = store
        .resolve_direct_access_request(&request.request_id, "actor-1", &vault)
        .unwrap()
        .unwrap();
    assert_eq!(resolved.value, None);
    assert_eq!(resolved.request.status, DirectAccessStatus::Revoked);
    assert_eq!(
        resolved.outcome_reason.as_deref(),
        Some("grant_value_changed")
    );
    assert_eq!(
        resolved.request.decision_reason.as_deref(),
        Some("grant_value_changed")
    );
    assert_eq!(resolved.request.resolve_count, 0);
}

#[test]
fn direct_access_request_expiry_overwrites_prior_approval_reason() {
    let vault = dam_vault::Vault::open_in_memory().unwrap();
    let store = ConsentStore::open_in_memory().unwrap();
    let reference = Reference::generate(SensitiveType::Email);
    vault
        .write(&VaultRecord {
            reference: reference.clone(),
            kind: SensitiveType::Email,
            value: "alice@example.test".to_string(),
        })
        .unwrap();

    let request = store
        .create_direct_access_request(
            &CreateDirectAccessRequest {
                vault_key: reference.key(),
                actor_id: "actor-1".to_string(),
                requesting_actor: "Codex".to_string(),
                purpose: "fill local email field".to_string(),
                reason: None,
                requested_duration_seconds: 1,
                pending_timeout_seconds: 60,
                correlation_id: None,
            },
            &vault,
        )
        .unwrap();
    store
        .approve_direct_access_request(&request.request_id, 1, Some("approved".to_string()))
        .unwrap()
        .unwrap();

    thread::sleep(Duration::from_secs(2));

    let resolved = store
        .resolve_direct_access_request(&request.request_id, "actor-1", &vault)
        .unwrap()
        .unwrap();
    assert_eq!(resolved.value, None);
    assert_eq!(resolved.request.status, DirectAccessStatus::Expired);
    assert_eq!(resolved.outcome_reason.as_deref(), Some("grant_expired"));
    assert_eq!(
        resolved.request.decision_reason.as_deref(),
        Some("grant_expired")
    );
}

#[test]
fn direct_access_request_cannot_consume_after_grant_expires_during_vault_read() {
    let vault = Arc::new(dam_vault::Vault::open_in_memory().unwrap());
    let store = ConsentStore::open_in_memory().unwrap();
    let reference = Reference::generate(SensitiveType::Email);
    vault
        .write(&VaultRecord {
            reference: reference.clone(),
            kind: SensitiveType::Email,
            value: "alice@example.test".to_string(),
        })
        .unwrap();

    let request = store
        .create_direct_access_request(
            &CreateDirectAccessRequest {
                vault_key: reference.key(),
                actor_id: "actor-1".to_string(),
                requesting_actor: "Codex".to_string(),
                purpose: "fill local email field".to_string(),
                reason: None,
                requested_duration_seconds: 1,
                pending_timeout_seconds: 60,
                correlation_id: None,
            },
            vault.as_ref(),
        )
        .unwrap();
    store
        .approve_direct_access_request(&request.request_id, 1, Some("approved".to_string()))
        .unwrap()
        .unwrap();

    let delayed_vault = DelayedVaultReader {
        inner: vault,
        delay: Duration::from_secs(2),
    };

    let resolved = store
        .resolve_direct_access_request(&request.request_id, "actor-1", &delayed_vault)
        .unwrap()
        .unwrap();
    assert_eq!(resolved.value, None);
    assert_eq!(resolved.request.status, DirectAccessStatus::Expired);
    assert_eq!(resolved.request.resolve_count, 0);
    assert_eq!(resolved.outcome_reason.as_deref(), Some("grant_expired"));
    assert_eq!(
        resolved.request.decision_reason.as_deref(),
        Some("grant_expired")
    );
}

#[test]
fn direct_access_request_persists_and_expires_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let consent_path = dir.path().join("consent.db");
    let vault_path = dir.path().join("vault.db");
    let vault = dam_vault::Vault::open(&vault_path).unwrap();
    let reference = Reference::generate(SensitiveType::Email);
    vault
        .write(&VaultRecord {
            reference: reference.clone(),
            kind: SensitiveType::Email,
            value: "alice@example.test".to_string(),
        })
        .unwrap();

    let request_id = {
        let store = ConsentStore::open(&consent_path).unwrap();
        let request = store
            .create_direct_access_request(
                &CreateDirectAccessRequest {
                    vault_key: reference.key(),
                    actor_id: "actor-1".to_string(),
                    requesting_actor: "Codex".to_string(),
                    purpose: "fill local email field".to_string(),
                    reason: None,
                    requested_duration_seconds: 1,
                    pending_timeout_seconds: 60,
                    correlation_id: None,
                },
                &vault,
            )
            .unwrap();
        let approved = store
            .approve_direct_access_request(&request.request_id, 1, None)
            .unwrap()
            .unwrap();
        approved.request_id
    };

    std::thread::sleep(std::time::Duration::from_secs(2));

    let reopened = ConsentStore::open(&consent_path).unwrap();
    let resolved = reopened
        .resolve_direct_access_request(&request_id, "actor-1", &vault)
        .unwrap()
        .unwrap();
    assert_eq!(resolved.value, None);
    assert_eq!(resolved.request.status, DirectAccessStatus::Expired);
    assert_eq!(resolved.outcome_reason.as_deref(), Some("grant_expired"));
}
