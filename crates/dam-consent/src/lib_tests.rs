use super::*;
use dam_core::{VaultRecord, VaultWriter};

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
            target_scope("chatgpt-codex"),
        )
        .unwrap();

    assert_eq!(entry.scope, "target:chatgpt-codex");
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
                &["target:chatgpt-codex".to_string()],
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
                &["target:chatgpt-codex".to_string()],
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
