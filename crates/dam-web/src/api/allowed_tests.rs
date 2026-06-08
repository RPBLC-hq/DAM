use super::*;

#[test]
fn allowed_view_groups_active_expired_and_revoked_consents() {
    let vault = dam_vault::Vault::open_in_memory().unwrap();
    vault.put("email:active", "ada@example.test").unwrap();
    vault.put("phone:expired", "+1 415 555 0142").unwrap();
    vault.put("email:revoked", "revoked@example.test").unwrap();

    let store = dam_consent::ConsentStore::open_in_memory().unwrap();
    store
        .grant(&dam_consent::GrantConsent {
            kind: dam_core::SensitiveType::Email,
            value: "ada@example.test".to_string(),
            vault_key: Some("email:active".to_string()),
            ttl_seconds: 60,
            created_by: "anthropic".to_string(),
            reason: None,
        })
        .unwrap();
    store
        .grant(&dam_consent::GrantConsent {
            kind: dam_core::SensitiveType::Phone,
            value: "+1 415 555 0142".to_string(),
            vault_key: Some("phone:expired".to_string()),
            ttl_seconds: 0,
            created_by: "openai".to_string(),
            reason: None,
        })
        .unwrap();
    let revoked = store
        .grant(&dam_consent::GrantConsent {
            kind: dam_core::SensitiveType::Email,
            value: "revoked@example.test".to_string(),
            vault_key: Some("email:revoked".to_string()),
            ttl_seconds: 60,
            created_by: "codex".to_string(),
            reason: None,
        })
        .unwrap();
    assert!(store.revoke(&revoked.id).unwrap());

    let now = now_unix_secs().unwrap();
    let view = allowed_view_from_entries(&vault, store.list().unwrap(), &ListQuery::default(), now)
        .unwrap();

    assert_eq!(view.active.len(), 1);
    assert_eq!(view.active[0].value, "ada@example.test");
    assert_eq!(view.expired.len(), 1);
    assert_eq!(view.revoked.len(), 1);
}

#[test]
fn allowed_view_filters_by_query() {
    let vault = dam_vault::Vault::open_in_memory().unwrap();
    vault.put("email:active", "ada@example.test").unwrap();
    let store = dam_consent::ConsentStore::open_in_memory().unwrap();
    store
        .grant(&dam_consent::GrantConsent {
            kind: dam_core::SensitiveType::Email,
            value: "ada@example.test".to_string(),
            vault_key: Some("email:active".to_string()),
            ttl_seconds: 60,
            created_by: "anthropic".to_string(),
            reason: None,
        })
        .unwrap();

    let query = ListQuery {
        q: Some("anthropic".to_string()),
        ..ListQuery::default()
    };
    let view = allowed_view_from_entries(
        &vault,
        store.list().unwrap(),
        &query,
        now_unix_secs().unwrap(),
    )
    .unwrap();

    assert_eq!(view.active.len(), 1);

    let query = ListQuery {
        q: Some("openai".to_string()),
        ..ListQuery::default()
    };
    let view = allowed_view_from_entries(
        &vault,
        store.list().unwrap(),
        &query,
        now_unix_secs().unwrap(),
    )
    .unwrap();

    assert_eq!(view.active.len(), 0);
}

#[test]
fn raw_value_grants_do_not_expose_fingerprints() {
    let vault = dam_vault::Vault::open_in_memory().unwrap();
    let store = dam_consent::ConsentStore::open_in_memory().unwrap();
    store
        .grant(&dam_consent::GrantConsent {
            kind: dam_core::SensitiveType::Email,
            value: "ada@example.test".to_string(),
            vault_key: None,
            ttl_seconds: 60,
            created_by: "test".to_string(),
            reason: None,
        })
        .unwrap();

    let view = allowed_view_from_entries(
        &vault,
        store.list().unwrap(),
        &ListQuery::default(),
        now_unix_secs().unwrap(),
    )
    .unwrap();

    assert_eq!(view.active[0].value, "[email grant]");
    assert!(!view.active[0].value.contains("ada@example.test"));
}
