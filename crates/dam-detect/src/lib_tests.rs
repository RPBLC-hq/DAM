use super::*;

#[test]
fn detects_email() {
    let detections = detect("email alice@example.com");

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].kind, SensitiveType::Email);
    assert_eq!(detections[0].value, "alice@example.com");
}

#[test]
fn detects_email_with_space_after_at() {
    let detections = detect("email alice@ example.com");

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].kind, SensitiveType::Email);
    assert_eq!(detections[0].value, "alice@ example.com");
}

#[test]
fn detects_email_with_space_before_at() {
    let detections = detect("email alice @example.com");

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].kind, SensitiveType::Email);
    assert_eq!(detections[0].value, "alice @example.com");
}

#[test]
fn detects_email_with_spaces_around_domain_dot() {
    let detections = detect("email alice@example . com");

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].kind, SensitiveType::Email);
    assert_eq!(detections[0].value, "alice@example . com");
}

#[test]
fn does_not_detect_package_version_strings_as_email() {
    let detections = detect("packages dam@0.1.0 and dam-web-ui@0.1.0");

    assert!(detections.is_empty());
}

#[test]
fn detects_email_without_absorbing_following_sentence() {
    let detections = detect("email alice@example.com. What domain?");

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].kind, SensitiveType::Email);
    assert_eq!(detections[0].value, "alice@example.com");
}

#[test]
fn does_not_detect_email_derived_domain_repeated_standalone() {
    let detections = detect("email alice@example.com domain example.com");

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].kind, SensitiveType::Email);
}

#[test]
fn does_not_detect_email_derived_hyphenated_domain_repeated_standalone() {
    let detections = detect("email alice@corp-example.com domain corp-example.com");

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].kind, SensitiveType::Email);
}

#[test]
fn does_not_detect_email_derived_domain_with_spaced_dot() {
    let detections = detect("email alice@example.com domain example . com");

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].kind, SensitiveType::Email);
}

#[test]
fn detects_supplied_related_domain_without_email_in_input() {
    let detections = detect_with_related_domains(
        "provider answered example.com",
        &["example.com".to_string()],
    );

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].kind, SensitiveType::Domain);
    assert_eq!(detections[0].value, "example.com");
}

#[test]
fn does_not_detect_domain_inside_email_only() {
    let detections =
        detect_with_related_domains("email alice@example.com", &["example.com".to_string()]);

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].kind, SensitiveType::Email);
}

#[test]
fn does_not_detect_email_domain_inside_subdomain() {
    let detections = detect_with_related_domains(
        "email alice@example.com route api.example.com",
        &["example.com".to_string()],
    );

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].kind, SensitiveType::Email);
}

#[test]
fn detects_supplied_related_domain_case_insensitively_with_spaced_dot() {
    let detections = detect_with_related_domains(
        "provider answered Example .COM.",
        &["example.com".to_string()],
    );

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].kind, SensitiveType::Domain);
    assert_eq!(detections[0].value, "Example .COM");
}

#[test]
fn does_not_detect_related_domain_inside_longer_domain() {
    let detections = detect_with_related_domains(
        "provider answered example.company and example.com.au",
        &["example.com".to_string()],
    );

    assert!(detections.is_empty());
}

#[test]
fn detects_phone() {
    let detections = detect("call +14155551234");

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].kind, SensitiveType::Phone);
}

#[test]
fn detects_valid_ssn() {
    let detections = detect("ssn 123-45-6789");

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].kind, SensitiveType::Ssn);
}

#[test]
fn rejects_invalid_ssn_area() {
    assert!(detect("ssn 666-45-6789").is_empty());
}

#[test]
fn detects_valid_credit_card() {
    let detections = detect("card 4111-1111-1111-1111");

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].kind, SensitiveType::CreditCard);
}

#[test]
fn rejects_invalid_credit_card() {
    assert!(detect("card 4111-1111-1111-1112").is_empty());
}

#[test]
fn detects_common_api_key_assignments() {
    let detections = detect("OPENAI_API_KEY=sk-proj-abcdefghijklmnopqrstuvwxyz123456 echo this");

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].kind, SensitiveType::ApiKey);
    assert_eq!(
        detections[0].value,
        "sk-proj-abcdefghijklmnopqrstuvwxyz123456"
    );
}

#[test]
fn detects_openai_project_keys_without_assignment_labels() {
    let detections = detect("token sk-proj-abcdefghijklmnopqrstuvwxyz123456");

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].kind, SensitiveType::ApiKey);
    assert_eq!(
        detections[0].value,
        "sk-proj-abcdefghijklmnopqrstuvwxyz123456"
    );
}

#[test]
fn detects_anthropic_keys_without_assignment_labels() {
    let detections = detect("token sk-ant-api03-abcdefghijklmnopqrstuvwxyz1234567890");

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].kind, SensitiveType::ApiKey);
    assert_eq!(
        detections[0].value,
        "sk-ant-api03-abcdefghijklmnopqrstuvwxyz1234567890"
    );
}

#[test]
fn detects_github_tokens_without_assignment_labels() {
    let detections = detect("token ghp_abcdefghijklmnopqrstuvwxyzABCDEFGH12");

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].kind, SensitiveType::ApiKey);
    assert_eq!(
        detections[0].value,
        "ghp_abcdefghijklmnopqrstuvwxyzABCDEFGH12"
    );
}

#[test]
fn detects_stripe_secret_keys_without_assignment_labels() {
    let detections = detect("token sk_live_1234567890abcdef");

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].kind, SensitiveType::ApiKey);
    assert_eq!(detections[0].value, "sk_live_1234567890abcdef");
}

#[test]
fn detects_google_api_keys_without_assignment_labels() {
    let token = format!("AIza{}", "A".repeat(36));
    let detections = detect(&format!("token {token}"));

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].kind, SensitiveType::ApiKey);
    assert_eq!(detections[0].value, token);
}

#[test]
fn detects_aws_access_key_ids_without_assignment_labels() {
    let token = "AKIAIOSFODNN7EXAMPLE";
    let detections = detect(&format!("aws key {token}"));

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].kind, SensitiveType::ApiKey);
    assert_eq!(detections[0].value, token);
}

#[test]
fn detects_stripe_webhook_signing_secrets_without_assignment_labels() {
    let token = format!("whsec_{}", "a".repeat(32));
    let detections = detect(&format!("webhook secret {token}"));

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].kind, SensitiveType::ApiKey);
    assert_eq!(detections[0].value, token);
}

#[test]
fn detects_bearer_jwts_as_api_keys() {
    let token = concat!(
        "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.",
        "eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkFsZXgifQ.",
        "SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c"
    );
    let detections = detect(&format!("Authorization: Bearer {token}"));

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].kind, SensitiveType::ApiKey);
    assert_eq!(detections[0].value, token);
}

#[test]
fn does_not_detect_short_api_key_like_values() {
    assert!(detect("OPENAI_API_KEY=sk-test").is_empty());
    assert!(detect("token ghp_short").is_empty());
    assert!(detect("token sk-ant-api03-short").is_empty());
    assert!(detect("token AIza_short").is_empty());
    assert!(detect("token AKIA_SHORT").is_empty());
    assert!(detect("token whsec_short").is_empty());
    assert!(detect("Authorization: Bearer short.jwt.parts").is_empty());
    assert!(detect("token ***").is_empty());
}

#[test]
fn returns_detections_in_text_order() {
    let detections = detect("ssn 123-45-6789 email alice@example.com");

    assert_eq!(detections.len(), 2);
    assert_eq!(detections[0].kind, SensitiveType::Ssn);
    assert_eq!(detections[1].kind, SensitiveType::Email);
}
