pub use dam_core::{Detection, SensitiveType, Span};

use once_cell::sync::Lazy;
use regex::Regex;

static EMAIL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"[A-Za-z0-9._%+\-]+[ \t\r\n]*@[ \t\r\n]*[A-Za-z0-9\-]+(?:\.[A-Za-z0-9\-]+|[ \t\r\n]+\.[ \t\r\n]*[A-Za-z0-9\-]+)+",
    )
    .unwrap()
});

static PHONE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\+[1-9]\d{6,14}|\b(?:\(\d{3}\)\s?|\d{3}[\-.])\d{3}[\-.]\d{4}\b").unwrap()
});

static SSN_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap());

static CREDIT_CARD_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b(?:\d{4}[\s\-]?){3}\d{4}\b").unwrap());

pub fn detect(input: &str) -> Vec<Detection> {
    detect_with_related_domains(input, &[])
}

pub fn detect_with_related_domains(input: &str, _related_domains: &[String]) -> Vec<Detection> {
    let mut detections = Vec::new();

    detect_emails(input, &mut detections);
    detect_with_regex(input, &PHONE_RE, SensitiveType::Phone, &mut detections);
    detect_ssns(input, &mut detections);
    detect_credit_cards(input, &mut detections);

    dedup_overlaps(detections)
}

fn detect_emails(input: &str, detections: &mut Vec<Detection>) {
    detections.extend(
        EMAIL_RE
            .find_iter(input)
            .filter(|m| valid_email_match(m.as_str()))
            .map(|m| Detection {
                kind: SensitiveType::Email,
                span: Span {
                    start: m.start(),
                    end: m.end(),
                },
                value: m.as_str().to_string(),
            }),
    );
}

fn detect_with_regex(
    input: &str,
    regex: &Regex,
    kind: SensitiveType,
    detections: &mut Vec<Detection>,
) {
    detections.extend(regex.find_iter(input).map(|m| Detection {
        kind,
        span: Span {
            start: m.start(),
            end: m.end(),
        },
        value: m.as_str().to_string(),
    }));
}

fn valid_email_match(value: &str) -> bool {
    let compact = value
        .chars()
        .filter(|character| !matches!(character, ' ' | '\t' | '\r' | '\n'))
        .collect::<String>();
    let Some((_local, domain)) = compact.rsplit_once('@') else {
        return false;
    };
    let mut labels = domain.split('.').collect::<Vec<_>>();
    let Some(top_level) = labels.pop() else {
        return false;
    };

    labels.iter().all(|label| !label.is_empty())
        && top_level.len() >= 2
        && top_level
            .chars()
            .all(|character| character.is_ascii_alphabetic())
}

fn detect_ssns(input: &str, detections: &mut Vec<Detection>) {
    detections.extend(SSN_RE.find_iter(input).filter_map(|m| {
        let digits: String = m.as_str().chars().filter(char::is_ascii_digit).collect();
        if is_valid_ssn_area(&digits) {
            Some(Detection {
                kind: SensitiveType::Ssn,
                span: Span {
                    start: m.start(),
                    end: m.end(),
                },
                value: m.as_str().to_string(),
            })
        } else {
            None
        }
    }));
}

fn detect_credit_cards(input: &str, detections: &mut Vec<Detection>) {
    detections.extend(CREDIT_CARD_RE.find_iter(input).filter_map(|m| {
        let digits: String = m.as_str().chars().filter(char::is_ascii_digit).collect();
        if luhn(&digits) {
            Some(Detection {
                kind: SensitiveType::CreditCard,
                span: Span {
                    start: m.start(),
                    end: m.end(),
                },
                value: m.as_str().to_string(),
            })
        } else {
            None
        }
    }));
}

fn dedup_overlaps(mut detections: Vec<Detection>) -> Vec<Detection> {
    detections.sort_by_key(|d| d.span.start);

    let mut kept: Vec<Detection> = Vec::with_capacity(detections.len());
    for detection in detections {
        if !kept
            .iter()
            .any(|existing| existing.span.overlaps(detection.span))
        {
            kept.push(detection);
        }
    }

    kept
}

fn is_valid_ssn_area(digits: &str) -> bool {
    if digits.len() != 9 {
        return false;
    }

    let area = &digits[0..3];
    let group = &digits[3..5];
    let serial = &digits[5..9];

    if area == "000" || area == "666" || group == "00" || serial == "0000" {
        return false;
    }

    area.parse::<u16>().is_ok_and(|n| n < 900)
}

fn luhn(digits: &str) -> bool {
    if digits.len() < 13 || digits.len() > 19 || digits.chars().all(|c| c == '0') {
        return false;
    }

    let mut sum = 0;
    let mut double = false;
    for ch in digits.chars().rev() {
        let Some(mut n) = ch.to_digit(10) else {
            return false;
        };
        if double {
            n *= 2;
            if n > 9 {
                n -= 9;
            }
        }
        sum += n;
        double = !double;
    }

    sum % 10 == 0
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
