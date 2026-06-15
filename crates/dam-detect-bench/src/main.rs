use std::{collections::BTreeMap, env, process};

use dam_core::Span;

use crate::{
    cases::{Case, ExpectedDetection, cases},
    metrics::{
        DetectionKey, DetectionRecord, MetricKind, increment_false_negative,
        increment_false_positive, increment_true_positive, summary,
    },
    report::{BenchmarkReport, FailureRecord, OutputFormat, print_report},
};

mod cases;
mod metrics;
mod report;

#[cfg(test)]
#[path = "main_tests.rs"]
mod main_tests;

fn main() {
    let format = match parse_args() {
        Ok(format) => format,
        Err(message) => {
            eprintln!("{message}");
            process::exit(2);
        }
    };

    match run_benchmark() {
        Ok(report) => {
            print_report(&report, format);
            if report.summary.false_negatives > 0 || report.summary.false_positives > 0 {
                process::exit(1);
            }
        }
        Err(message) => {
            eprintln!("{message}");
            process::exit(2);
        }
    }
}

fn parse_args() -> Result<OutputFormat, String> {
    let mut args = env::args().skip(1);
    let mut format = OutputFormat::Text;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--format" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--format requires text, json, or markdown".to_string())?;
                format = match value.as_str() {
                    "text" => OutputFormat::Text,
                    "json" => OutputFormat::Json,
                    "markdown" => OutputFormat::Markdown,
                    _ => {
                        return Err(format!(
                            "unsupported format {value:?}; use text, json, or markdown"
                        ));
                    }
                };
            }
            "-h" | "--help" => {
                println!("Usage: cargo run -p dam-detect-bench -- [--format text|json|markdown]");
                process::exit(0);
            }
            other => return Err(format!("unexpected argument: {other}")),
        }
    }

    Ok(format)
}

fn run_benchmark() -> Result<BenchmarkReport, String> {
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
