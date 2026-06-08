use super::*;

#[test]
fn canonicalizes_detector_supported_email_spacing() {
    assert_eq!(
        canonical_sensitive_value(SensitiveType::Email, "alice@ example .COM"),
        "alice@example.com"
    );
    assert_eq!(
        canonical_sensitive_value(SensitiveType::Email, "alice @example.com"),
        "alice@example.com"
    );
}

#[test]
fn leaves_non_email_values_unchanged() {
    assert_eq!(
        canonical_sensitive_value(SensitiveType::Phone, "+1 555 555 5555"),
        "+1 555 555 5555"
    );
}

#[test]
fn canonicalizes_domain_spacing_and_case() {
    assert_eq!(
        canonical_sensitive_value(SensitiveType::Domain, "Example .COM"),
        "example.com"
    );
}
