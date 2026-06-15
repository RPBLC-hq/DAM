use std::collections::BTreeMap;

use dam_core::Span;

use crate::{
    cases::{Case, ExpectedDetection, cases},
    metrics::{
        DetectionKey, DetectionRecord, MetricKind, increment_false_negative,
        increment_false_positive, increment_true_positive, summary,
    },
    report::{BenchmarkReport, FailureRecord},
};

pub(crate) fn run_benchmark() -> Result<BenchmarkReport, String> {
    let cases = cases();
    let mut failures = Vec::new();
    let mut per_kind_counts: BTreeMap<MetricKind, (usize, usize, usize)> = BTreeMap::new();
    let mut total_true_positives = 0usize;
    let mut total_false_positives = 0usize;
    let mut total_false_negatives = 0usize;
    let mut total_expected = 0usize;
    let mut total_actual = 0usize;

    for case in &cases {
        let actual = dam_detect::detect_with_related_domains(&case.input, &case.related_domains);
        let expected = expected_records(case)?;

        total_expected += expected.len();
        total_actual += actual.len();

        let mut actual_matched = vec![false; actual.len()];
        for expected_record in &expected {
            let expected_key = DetectionKey {
                kind: expected_record.kind,
                span: Span {
                    start: expected_record.start,
                    end: expected_record.end,
                },
            };
            if let Some((index, _)) = actual.iter().enumerate().find(|(index, detection)| {
                !actual_matched[*index]
                    && DetectionKey {
                        kind: detection.kind.into(),
                        span: detection.span,
                    } == expected_key
            }) {
                actual_matched[index] = true;
                increment_true_positive(&mut per_kind_counts, expected_record.kind);
                total_true_positives += 1;
            } else {
                increment_false_negative(&mut per_kind_counts, expected_record.kind);
                total_false_negatives += 1;
                failures.push(FailureRecord {
                    case: case.name.to_string(),
                    failure: "false_negative".to_string(),
                    kind: expected_record.kind,
                    start: expected_record.start,
                    end: expected_record.end,
                    value: expected_record.value.clone(),
                });
            }
        }

        for (matched, detection) in actual_matched.into_iter().zip(actual.iter()) {
            if matched {
                continue;
            }
            let kind: MetricKind = detection.kind.into();
            increment_false_positive(&mut per_kind_counts, kind);
            total_false_positives += 1;
            failures.push(FailureRecord {
                case: case.name.to_string(),
                failure: "false_positive".to_string(),
                kind,
                start: detection.span.start,
                end: detection.span.end,
                value: detection.value.clone(),
            });
        }
    }

    let per_kind = per_kind_counts
        .into_iter()
        .map(|(kind, (tp, fp, fn_))| (kind, summary(tp, fp, fn_)))
        .collect::<BTreeMap<_, _>>();

    Ok(BenchmarkReport {
        suite: "contract_baseline",
        cases: cases.len(),
        expected_detections: total_expected,
        actual_detections: total_actual,
        summary: summary(
            total_true_positives,
            total_false_positives,
            total_false_negatives,
        ),
        per_kind,
        failures,
    })
}

fn expected_records(case: &Case) -> Result<Vec<DetectionRecord>, String> {
    case.expected
        .iter()
        .map(|expected| expected_record(case, expected))
        .collect()
}

fn expected_record(case: &Case, expected: &ExpectedDetection) -> Result<DetectionRecord, String> {
    let start = case.input.find(&expected.value).ok_or_else(|| {
        format!(
            "case {} is missing expected value {:?}",
            case.name, expected.value
        )
    })?;
    Ok(DetectionRecord {
        kind: expected.kind.into(),
        start,
        end: start + expected.value.len(),
        value: expected.value.clone(),
    })
}

#[cfg(test)]
#[path = "evaluator_tests.rs"]
mod tests;
