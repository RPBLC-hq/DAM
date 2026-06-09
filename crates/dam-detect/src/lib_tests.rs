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
    let detections = detect("OPENAI_API_KEY=sk-proj-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");

    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].kind, SensitiveType::ApiKey);
    assert_eq!(
        detections[0].value,
        "sk-proj-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
}

#[test]
fn does_not_detect_short_api_key_like_values() {
    assert!(detect("OPENAI_API_KEY=sk-test").is_empty());
}

#[test]
fn returns_detections_in_text_order() {
    let detections = detect("ssn 123-45-6789 email alice@example.com");

    assert_eq!(detections.len(), 2);
    assert_eq!(detections[0].kind, SensitiveType::Ssn);
    assert_eq!(detections[1].kind, SensitiveType::Email);
}
