use super::*;
use axum::extract::{Path, State};
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn kind_from_key_extracts_prefix() {
    assert_eq!(kind_from_key("email:abc123"), "email");
    assert_eq!(kind_from_key("phone:xyz"), "phone");
    assert_eq!(kind_from_key("nokey"), "nokey");
}

#[test]
fn wallet_item_marks_active_grants_as_allowed() {
    let now = 1_000;
    let key = "email:1111111111111111111111";
    let item = wallet_item_from_entry(
        dam_vault::VaultEntry {
            key: key.to_string(),
            value: "ada@example.test".to_string(),
            created_at: now - 100,
            updated_at: now - 50,
        },
        &[dam_consent::ConsentEntry {
            id: "grant_1".to_string(),
            kind: dam_core::SensitiveType::Email,
            value_fingerprint: "fingerprint".to_string(),
            vault_key: Some(key.to_string()),
            scope: "global".to_string(),
            created_at: now - 20,
            expires_at: now + 60,
            revoked_at: None,
            created_by: "Claude Code".to_string(),
            reason: None,
        }],
        now,
    );

    assert_eq!(item.state, ItemState::Allowed);
    assert_eq!(item.shared_with[0].name, "Claude Code");
}

#[test]
fn wallet_item_dedupes_profile_grants_for_multiple_targets() {
    let now = 1_000;
    let key = "email:1111111111111111111111";
    let item = wallet_item_from_entry(
        dam_vault::VaultEntry {
            key: key.to_string(),
            value: "ada@example.test".to_string(),
            created_at: now - 100,
            updated_at: now - 50,
        },
        &[
            dam_consent::ConsentEntry {
                id: "grant_1".to_string(),
                kind: dam_core::SensitiveType::Email,
                value_fingerprint: "fingerprint".to_string(),
                vault_key: Some(key.to_string()),
                scope: dam_consent::target_scope("openai"),
                created_at: now - 20,
                expires_at: now + 60,
                revoked_at: None,
                created_by: "Codex".to_string(),
                reason: None,
            },
            dam_consent::ConsentEntry {
                id: "grant_2".to_string(),
                kind: dam_core::SensitiveType::Email,
                value_fingerprint: "fingerprint".to_string(),
                vault_key: Some(key.to_string()),
                scope: dam_consent::target_scope("chatgpt-codex"),
                created_at: now - 10,
                expires_at: now + 60,
                revoked_at: None,
                created_by: "Codex".to_string(),
                reason: None,
            },
        ],
        now,
    );

    assert_eq!(item.state, ItemState::Allowed);
    assert_eq!(item.shared_with.len(), 1);
    assert_eq!(item.shared_with[0].name, "Codex");
}

#[test]
fn traffic_app_profile_scopes_expand_to_all_targets() {
    let scopes = target_scopes_for_traffic_app_ids(
        &dam_net::llm_mvp_profile(),
        &["openai-api".to_string(), "chatgpt-codex".to_string()],
    );

    assert_eq!(
        scopes,
        vec![
            dam_consent::target_scope("openai"),
            dam_consent::target_scope("chatgpt-codex"),
        ]
    );
}

#[test]
fn wallet_state_filter_returns_only_allowed_values() {
    let now = 1_000;
    let allowed_reference = Reference::generate(dam_core::SensitiveType::Email);
    let protected_reference = Reference::generate(dam_core::SensitiveType::Phone);
    let allowed_key = allowed_reference.key();
    let protected_key = protected_reference.key();
    let query = ListQuery {
        q: None,
        state: Some("allowed".to_string()),
        sort: None,
        dir: None,
    };
    let items = wallet_items_from_entries(
        vec![
            dam_vault::VaultEntry {
                key: allowed_key.to_string(),
                value: "ada@example.test".to_string(),
                created_at: now,
                updated_at: now,
            },
            dam_vault::VaultEntry {
                key: protected_key.to_string(),
                value: "+14155550142".to_string(),
                created_at: now,
                updated_at: now,
            },
        ],
        &[dam_consent::ConsentEntry {
            id: "grant_1".to_string(),
            kind: dam_core::SensitiveType::Email,
            value_fingerprint: "fingerprint".to_string(),
            vault_key: Some(allowed_key.to_string()),
            scope: "global".to_string(),
            created_at: now,
            expires_at: now + 60,
            revoked_at: None,
            created_by: "Claude Code".to_string(),
            reason: None,
        }],
        &query,
        now,
    );

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, allowed_reference.id);
}

#[tokio::test]
async fn add_wallet_value_writes_to_vault_and_returns_detail() {
    let vault = Arc::new(dam_vault::Vault::open_in_memory().unwrap());
    let state = test_state(vault.clone(), None);

    let response = add(
        State(state),
        Json(AddWalletRequest {
            kind: "email".to_string(),
            value: "ada@example.test".to_string(),
        }),
    )
    .await
    .unwrap();

    assert_eq!(response.data.item.kind, "email");
    assert_eq!(response.data.item.value, "ada@example.test");
    assert!(!response.data.item.id.contains(':'));
    let key = response
        .data
        .reference
        .trim_start_matches('[')
        .trim_end_matches(']');
    assert_eq!(vault.get(key).unwrap().as_deref(), Some("ada@example.test"));
}

#[tokio::test]
async fn allow_wallet_value_with_global_scope_records_stable_party() {
    let vault = Arc::new(dam_vault::Vault::open_in_memory().unwrap());
    let consent_store = Arc::new(dam_consent::ConsentStore::open_in_memory().unwrap());
    let reference = vault
        .write(&VaultRecord {
            reference: Reference::generate(dam_core::SensitiveType::Email),
            kind: dam_core::SensitiveType::Email,
            value: "ada@example.test".to_string(),
        })
        .unwrap();
    let state = test_state(vault.clone(), Some(consent_store.clone()));

    let response = allow(
        State(state),
        Path(reference.id.clone()),
        Json(AllowRequest {
            party: "Tous les profils".to_string(),
            ttl_seconds: Some(60),
            reason: None,
            scope: Some(dam_consent::DEFAULT_SCOPE.to_string()),
            profile_id: None,
        }),
    )
    .await
    .unwrap();

    assert_eq!(response.data.item.state, ItemState::Allowed);
    let entries = consent_store.list().unwrap();
    assert_eq!(entries[0].created_by, "All profiles");
    assert_eq!(entries[0].scope, dam_consent::DEFAULT_SCOPE);
}

#[tokio::test]
async fn remove_wallet_value_deletes_vault_row_and_revokes_access() {
    let vault = Arc::new(dam_vault::Vault::open_in_memory().unwrap());
    let consent_store = Arc::new(dam_consent::ConsentStore::open_in_memory().unwrap());
    let reference = vault
        .write(&VaultRecord {
            reference: Reference::generate(dam_core::SensitiveType::Email),
            kind: dam_core::SensitiveType::Email,
            value: "ada@example.test".to_string(),
        })
        .unwrap();
    let key = reference.key();
    consent_store
        .grant_for_reference(
            &key,
            vault.as_ref(),
            60,
            "Claude Code",
            Some("test grant".to_string()),
        )
        .unwrap();
    let state = test_state(vault.clone(), Some(consent_store.clone()));

    let response = remove(State(state), Path(reference.id.clone()))
        .await
        .unwrap();

    assert_eq!(response.data.id, reference.id);
    assert!(vault.get(&key).unwrap().is_none());
    assert!(
        consent_store
            .list()
            .unwrap()
            .into_iter()
            .all(|entry| entry.revoked_at.is_some())
    );
}

fn test_state(
    vault: Arc<dam_vault::Vault>,
    consent_store: Option<Arc<dam_consent::ConsentStore>>,
) -> crate::AppState {
    crate::AppState {
        surface: crate::Surface::Web,
        tray_post_token: None,
        vault,
        consent_store,
        logs: Arc::new(dam_log::LogStore::open_in_memory().unwrap()),
        config: Arc::new(dam_config::DamConfig::default()),
        config_path: None,
        db_path: Arc::new(PathBuf::from("vault.db")),
        log_path: Arc::new(PathBuf::from("log.db")),
        client: reqwest::Client::new(),
        requests: Arc::new(crate::request_store::RequestStore::default()),
        events: Arc::new(crate::events_bus::EventBus::new()),
    }
}
