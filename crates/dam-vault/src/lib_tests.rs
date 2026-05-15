use super::*;

#[test]
fn put_then_get_returns_value() {
    let vault = Vault::open_in_memory().unwrap();

    vault.put("email:alice", "alice@example.com").unwrap();

    assert_eq!(
        vault.get("email:alice").unwrap(),
        Some("alice@example.com".to_string())
    );
}

#[test]
fn get_missing_key_returns_none() {
    let vault = Vault::open_in_memory().unwrap();

    assert_eq!(vault.get("missing").unwrap(), None);
}

#[test]
fn put_existing_key_replaces_value_without_duplicate() {
    let vault = Vault::open_in_memory().unwrap();

    vault.put("email:alice", "old@example.com").unwrap();
    vault.put("email:alice", "new@example.com").unwrap();

    assert_eq!(vault.count().unwrap(), 1);
    assert_eq!(
        vault.get("email:alice").unwrap(),
        Some("new@example.com".to_string())
    );
}

#[test]
fn delete_existing_key_returns_true() {
    let vault = Vault::open_in_memory().unwrap();

    vault.put("email:alice", "alice@example.com").unwrap();

    assert!(vault.delete("email:alice").unwrap());
    assert_eq!(vault.get("email:alice").unwrap(), None);
}

#[test]
fn delete_missing_key_returns_false() {
    let vault = Vault::open_in_memory().unwrap();

    assert!(!vault.delete("missing").unwrap());
}

#[test]
fn list_returns_entries_ordered_by_key() {
    let vault = Vault::open_in_memory().unwrap();

    vault.put("phone:bob", "+14155551234").unwrap();
    vault.put("email:alice", "alice@example.com").unwrap();

    let entries = vault.list().unwrap();

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].key, "email:alice");
    assert_eq!(entries[0].value, "alice@example.com");
    assert_eq!(entries[1].key, "phone:bob");
    assert_eq!(entries[1].value, "+14155551234");
}

#[test]
fn list_deduplicates_equal_values_per_kind() {
    let vault = Vault::open_in_memory().unwrap();
    let first = Reference::generate(dam_core::SensitiveType::Email);
    let second = Reference::generate(dam_core::SensitiveType::Email);

    vault.put(&first.key(), "alice@example.com").unwrap();
    vault.put(&second.key(), "alice@example.com").unwrap();

    let entries = vault.list().unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].value, "alice@example.com");
}

#[test]
fn entries_persist_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("vault.db");

    {
        let vault = Vault::open(&db_path).unwrap();
        vault.put("email:alice", "alice@example.com").unwrap();
    }

    let vault = Vault::open(&db_path).unwrap();
    assert_eq!(
        vault.get("email:alice").unwrap(),
        Some("alice@example.com".to_string())
    );
}

#[test]
fn implements_vault_writer_contract() {
    let vault = Vault::open_in_memory().unwrap();
    let reference = dam_core::Reference {
        kind: dam_core::SensitiveType::Email,
        id: "7B2HkqFn9xR4mWpD3nYvKt".to_string(),
    };
    let record = dam_core::VaultRecord {
        reference: reference.clone(),
        kind: dam_core::SensitiveType::Email,
        value: "alice@example.com".to_string(),
    };

    let stored_reference = vault.write(&record).unwrap();

    assert_eq!(stored_reference, reference);
    assert_eq!(
        vault.get(&reference.key()).unwrap(),
        Some("alice@example.com".to_string())
    );
}

#[test]
fn vault_writer_returns_existing_reference_for_duplicate_value() {
    let vault = Vault::open_in_memory().unwrap();
    let first = dam_core::Reference::generate(dam_core::SensitiveType::Email);
    let second = dam_core::Reference::generate(dam_core::SensitiveType::Email);

    let first_stored = vault
        .write(&dam_core::VaultRecord {
            reference: first.clone(),
            kind: dam_core::SensitiveType::Email,
            value: "alice@example.com".to_string(),
        })
        .unwrap();
    let second_stored = vault
        .write(&dam_core::VaultRecord {
            reference: second.clone(),
            kind: dam_core::SensitiveType::Email,
            value: "alice@example.com".to_string(),
        })
        .unwrap();

    assert_eq!(first_stored, first);
    assert_eq!(second_stored, first);
    assert_eq!(vault.count().unwrap(), 1);
    assert_eq!(vault.get(&second.key()).unwrap(), None);
}

#[test]
fn vault_writer_refreshes_existing_reference_recency_for_duplicate_value() {
    let vault = Vault::open_in_memory().unwrap();
    let first = dam_core::Reference::generate(dam_core::SensitiveType::Email);
    let second = dam_core::Reference::generate(dam_core::SensitiveType::Email);

    vault
        .write(&dam_core::VaultRecord {
            reference: first.clone(),
            kind: dam_core::SensitiveType::Email,
            value: "alice@example.com".to_string(),
        })
        .unwrap();
    let before = vault.list().unwrap()[0].updated_at;

    std::thread::sleep(std::time::Duration::from_secs(1));
    let stored = vault
        .write(&dam_core::VaultRecord {
            reference: second,
            kind: dam_core::SensitiveType::Email,
            value: "alice@example.com".to_string(),
        })
        .unwrap();
    let after = vault.list().unwrap()[0].updated_at;

    assert_eq!(stored, first);
    assert!(after > before);
}

#[test]
fn vault_writer_can_store_duplicate_values_when_deduplication_is_disabled() {
    let vault = Vault::open_in_memory().unwrap();
    let first = dam_core::Reference::generate(dam_core::SensitiveType::Email);
    let second = dam_core::Reference::generate(dam_core::SensitiveType::Email);

    vault
        .write(&dam_core::VaultRecord {
            reference: first.clone(),
            kind: dam_core::SensitiveType::Email,
            value: "alice@example.com".to_string(),
        })
        .unwrap();
    let second_stored = vault
        .write_with_options(
            &dam_core::VaultRecord {
                reference: second.clone(),
                kind: dam_core::SensitiveType::Email,
                value: "alice@example.com".to_string(),
            },
            dam_core::VaultWriteOptions { deduplicate: false },
        )
        .unwrap();

    assert_eq!(second_stored, second);
    assert_eq!(vault.count().unwrap(), 2);
    assert_eq!(vault.list().unwrap().len(), 1);
}

#[test]
fn implements_vault_reader_contract() {
    let vault = Vault::open_in_memory().unwrap();
    let reference = dam_core::Reference {
        kind: dam_core::SensitiveType::Email,
        id: "7B2HkqFn9xR4mWpD3nYvKt".to_string(),
    };
    vault.put(&reference.key(), "alice@example.com").unwrap();

    assert_eq!(
        vault.read(&reference).unwrap(),
        Some("alice@example.com".to_string())
    );

    let missing = dam_core::Reference {
        kind: dam_core::SensitiveType::Email,
        id: "2D5hXQp8nJ9kLmN4rT6vWy".to_string(),
    };
    assert_eq!(vault.read(&missing).unwrap(), None);
}
