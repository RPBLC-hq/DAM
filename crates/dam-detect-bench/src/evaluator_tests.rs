use dam_core::SensitiveType;

use super::*;

#[test]
fn benchmark_contract_baseline_has_no_failures() {
    let report = run_benchmark().expect("benchmark should run");

    assert_eq!(report.suite, "contract_baseline");
    assert!(
        report.failures.is_empty(),
        "unexpected failures: {:?}",
        report.failures
    );
    assert_eq!(report.summary.false_positives, 0);
    assert_eq!(report.summary.false_negatives, 0);
    assert!(report.summary.true_positives > 0);
}

#[test]
fn expected_records_resolve_exact_spans_from_case_values() {
    let case = Case {
        name: "email/basic",
        input: "email alice@example.com".to_string(),
        expected: vec![ExpectedDetection {
            kind: SensitiveType::Email,
            value: "alice@example.com".to_string(),
        }],
        related_domains: Vec::new(),
    };

    let expected = expected_records(&case).expect("expected spans should resolve");
    assert_eq!(expected.len(), 1);
    assert_eq!(expected[0].kind, MetricKind::Email);
    assert_eq!(expected[0].start, 6);
    assert_eq!(expected[0].end, 23);
}

#[test]
fn expected_records_errors_on_ambiguous_value() {
    let case = Case {
        name: "ambiguous/duplicate",
        input: "alice@example.com and alice@example.com".to_string(),
        expected: vec![ExpectedDetection {
            kind: SensitiveType::Email,
            value: "alice@example.com".to_string(),
        }],
        related_domains: Vec::new(),
    };

    let err = expected_records(&case).expect_err("ambiguous value should fail");
    assert!(
        err.contains("ambiguous"),
        "error should mention ambiguous: {err}"
    );
}
