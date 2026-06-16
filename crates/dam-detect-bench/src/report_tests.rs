use std::collections::BTreeMap;

use super::*;
use crate::metrics::{MetricSummary, summary};

fn empty_report() -> BenchmarkReport {
    BenchmarkReport {
        suite: "test_suite",
        cases: 2,
        expected_detections: 0,
        actual_detections: 0,
        summary: summary(0, 0, 0),
        per_kind: BTreeMap::new(),
        failures: Vec::new(),
    }
}

fn report_with_failure() -> BenchmarkReport {
    use crate::metrics::MetricKind;
    let mut report = empty_report();
    report.summary = MetricSummary {
        true_positives: 1,
        false_positives: 1,
        false_negatives: 0,
        precision: 0.5,
        recall: 1.0,
        f1: 0.667,
    };
    report.failures.push(FailureRecord {
        case: "email/basic".to_string(),
        failure: "false_positive".to_string(),
        kind: MetricKind::Email,
        start: 0,
        end: 5,
        value: "extra".to_string(),
    });
    report
}

#[test]
fn text_format_contains_suite_name() {
    let mut output = Vec::new();
    print_report_to(&empty_report(), OutputFormat::Text, &mut output);
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("test_suite"), "missing suite name in text output");
}

#[test]
fn text_format_shows_no_failures() {
    let mut output = Vec::new();
    print_report_to(&empty_report(), OutputFormat::Text, &mut output);
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("failures: none"));
}

#[test]
fn text_format_lists_failures() {
    let mut output = Vec::new();
    print_report_to(&report_with_failure(), OutputFormat::Text, &mut output);
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("false_positive"));
    assert!(text.contains("email/basic"));
}

#[test]
fn json_format_is_valid_json_and_contains_suite() {
    let mut output = Vec::new();
    print_report_to(&empty_report(), OutputFormat::Json, &mut output);
    let text = String::from_utf8(output).unwrap();
    let value: serde_json::Value = serde_json::from_str(&text).expect("JSON should parse");
    assert_eq!(value["suite"], "test_suite");
}

#[test]
fn markdown_format_contains_heading() {
    let mut output = Vec::new();
    print_report_to(&empty_report(), OutputFormat::Markdown, &mut output);
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("# DAM detector benchmark"));
    assert!(text.contains("## Failures"));
    assert!(text.contains("none"));
}
