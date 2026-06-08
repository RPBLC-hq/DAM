use super::*;
use dam_core::{Reference, ReplacementMode, SensitiveType, Span};

fn replacement(text: &str, start: usize, end: usize) -> Replacement {
    Replacement {
        span: Span { start, end },
        text: text.to_string(),
        mode: ReplacementMode::Tokenized,
        reference: Some(Reference {
            kind: SensitiveType::Email,
            id: "7B2HkqFn9xR4mWpD3nYvKt".to_string(),
        }),
    }
}

#[test]
fn applies_one_replacement() {
    let input = "email alice@example.com";
    let replacements = [replacement("[email:7B2HkqFn9xR4mWpD3nYvKt]", 6, 23)];

    assert_eq!(
        redact(input, &replacements),
        "email [email:7B2HkqFn9xR4mWpD3nYvKt]"
    );
}

#[test]
fn applies_multiple_replacements_without_offset_errors() {
    let input = "alice@example.com 123-45-6789";
    let replacements = [
        replacement("[email:7B2HkqFn9xR4mWpD3nYvKt]", 0, 17),
        replacement("[ssn:7B2HkqFn9xR4mWpD3nYvKt]", 18, 29),
    ];

    assert_eq!(
        redact(input, &replacements),
        "[email:7B2HkqFn9xR4mWpD3nYvKt] [ssn:7B2HkqFn9xR4mWpD3nYvKt]"
    );
}
