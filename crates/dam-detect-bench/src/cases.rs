use dam_core::SensitiveType;

#[derive(Clone)]
pub(crate) struct ExpectedDetection {
    pub(crate) kind: SensitiveType,
    pub(crate) value: String,
}

pub(crate) struct Case {
    pub(crate) name: &'static str,
    pub(crate) input: String,
    pub(crate) expected: Vec<ExpectedDetection>,
    pub(crate) related_domains: Vec<String>,
}

pub(crate) fn cases() -> Vec<Case> {
    vec![
        case(
            "email/basic",
            "reach alice@example.com for access",
            vec![expected(SensitiveType::Email, "alice@example.com")],
        ),
        case(
            "email/spaced",
            "reach alice @example.com for access",
            vec![expected(SensitiveType::Email, "alice @example.com")],
        ),
        Case {
            name: "domain/related",
            input: "provider answered example.com".to_string(),
            expected: vec![expected(SensitiveType::Domain, "example.com")],
            related_domains: vec!["example.com".to_string()],
        },
        case(
            "phone/e164",
            "call +14155552671 now",
            vec![expected(SensitiveType::Phone, "+14155552671")],
        ),
        case(
            "ssn/basic",
            "ssn 123-45-6789 for tax form",
            vec![expected(SensitiveType::Ssn, "123-45-6789")],
        ),
        case(
            "cc/basic",
            "card 4111-1111-1111-1111 to verify",
            vec![expected(SensitiveType::CreditCard, "4111-1111-1111-1111")],
        ),
        secret_case(
            "api_key/openai_assignment",
            "OPENAI_API_KEY=",
            openai_project_key('a'),
            " rotate this",
        ),
        secret_case(
            "api_key/openai_direct",
            "token ",
            openai_project_key('b'),
            "",
        ),
        secret_case("api_key/google_direct", "token ", google_key('a'), ""),
        secret_case("api_key/sendgrid_direct", "token ", sendgrid_key(), ""),
        secret_case(
            "api_key/mailgun_direct",
            "token ",
            secret(&["key", "-0123456789abcdef", "0123456789abcdef"]),
            "",
        ),
        secret_case(
            "api_key/aws_access_key_id",
            "token ",
            aws_access_key_id(),
            "",
        ),
        secret_case(
            "api_key/slack_webhook",
            "webhook ",
            secret(&[
                "https://hooks.slack.com/",
                "services/",
                "T12345678/",
                "B12345678/",
                "abcdefghijklmnopqrstuvwxyzAB",
            ]),
            "",
        ),
        secret_case(
            "api_key/db_url_with_password",
            "db ",
            database_url_with_password(),
            "",
        ),
        {
            let value = secret(&["aaaaaaaaaa", ".", "bbbbbbbbbb", ".", "cccccccccc"]);
            Case {
                name: "api_key/bearer_jwt",
                input: format!("{}{}er {value}", "Authorization: ", "Bear"),
                expected: vec![ExpectedDetection {
                    kind: SensitiveType::ApiKey,
                    value,
                }],
                related_domains: Vec::new(),
            }
        },
        secret_case("api_key/pem_private_key", "", pem_private_key(), ""),
        case(
            "negative/package_version",
            "packages dam@0.1.0 and dam-web-ui@0.1.0",
            vec![],
        ),
        case("negative/invalid_ssn", "ssn 666-45-6789", vec![]),
        case(
            "negative/short_sendgrid",
            "sendgrid token SG.short.too_short",
            vec![],
        ),
        case(
            "negative/db_url_without_password",
            "db postgres://alice@db.example.com:5432/app",
            vec![],
        ),
    ]
}

fn case(name: &'static str, input: &str, expected: Vec<ExpectedDetection>) -> Case {
    Case {
        name,
        input: input.to_string(),
        expected,
        related_domains: Vec::new(),
    }
}

fn secret_case(name: &'static str, prefix: &str, value: String, suffix: &str) -> Case {
    Case {
        name,
        input: format!("{prefix}{value}{suffix}"),
        expected: vec![ExpectedDetection {
            kind: SensitiveType::ApiKey,
            value,
        }],
        related_domains: Vec::new(),
    }
}

fn expected(kind: SensitiveType, value: &str) -> ExpectedDetection {
    ExpectedDetection {
        kind,
        value: value.to_string(),
    }
}

fn openai_project_key(fill: char) -> String {
    format!("{}{}{}", "sk", "-proj-", fill.to_string().repeat(32))
}

fn google_key(fill: char) -> String {
    format!("{}za{}", "AI", fill.to_string().repeat(36))
}

fn sendgrid_key() -> String {
    format!("{}.{}.{}", "SG", "A".repeat(22), "b".repeat(43))
}

fn aws_access_key_id() -> String {
    format!("{}{}", "AKIA", "A".repeat(16))
}

fn database_url_with_password() -> String {
    secret(&[
        "postgres://alice:",
        "password123",
        "@db.example.com:5432/app",
    ])
}

fn pem_private_key() -> String {
    format!(
        "{}{}\n{}\n{}{}",
        "-----BEGIN ",
        "PRIVATE KEY-----",
        "A".repeat(64),
        "-----END ",
        "PRIVATE KEY-----"
    )
}

fn secret(parts: &[&str]) -> String {
    parts.concat()
}
