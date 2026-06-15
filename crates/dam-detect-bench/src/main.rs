use std::{collections::BTreeMap, env, process};

use dam_core::{SensitiveType, Span};
use serde::Serialize;

#[cfg(test)]
#[path = "main_tests.rs"]
mod main_tests;

#[derive(Clone, Copy)]
enum OutputFormat {
    Text,
    Json,
    Markdown,
}

#[derive(Clone, Copy)]
struct ExpectedDetection {
    kind: SensitiveType,
    value: &'static str,
}

struct Case {
    name: &'static str,
    input: &'static str,
    expected: &'static [ExpectedDetection],
    related_domains: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
enum MetricKind {
    Email,
    Domain,
    Phone,
    Ssn,
    CreditCard,
    ApiKey,
}

impl From<SensitiveType> for MetricKind {
    fn from(value: SensitiveType) -> Self {
        match value {
            SensitiveType::Email => Self::Email,
            SensitiveType::Domain => Self::Domain,
            SensitiveType::Phone => Self::Phone,
            SensitiveType::Ssn => Self::Ssn,
            SensitiveType::CreditCard => Self::CreditCard,
            SensitiveType::ApiKey => Self::ApiKey,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DetectionKey {
    kind: MetricKind,
    span: Span,
}

#[derive(Debug, Clone, Serialize)]
struct DetectionRecord {
    kind: MetricKind,
    start: usize,
    end: usize,
    value: String,
}

#[derive(Debug, Clone, Serialize)]
struct MetricSummary {
    true_positives: usize,
    false_positives: usize,
    false_negatives: usize,
    precision: f64,
    recall: f64,
    f1: f64,
}

#[derive(Debug, Clone, Serialize)]
struct FailureRecord {
    case: String,
    failure: String,
    kind: MetricKind,
    start: usize,
    end: usize,
    value: String,
}

#[derive(Debug, Clone, Serialize)]
struct BenchmarkReport {
    suite: &'static str,
    cases: usize,
    expected_detections: usize,
    actual_detections: usize,
    summary: MetricSummary,
    per_kind: BTreeMap<MetricKind, MetricSummary>,
    failures: Vec<FailureRecord>,
}

const CASES: &[Case] = &[
    Case {
        name: "email/basic",
        input: "reach alice@example.com for access",
        expected: &[ExpectedDetection {
            kind: SensitiveType::Email,
            value: "alice@example.com",
        }],
        related_domains: &[],
    },
    Case {
        name: "email/spaced",
        input: "reach alice @example.com for access",
        expected: &[ExpectedDetection {
            kind: SensitiveType::Email,
            value: "alice @example.com",
        }],
        related_domains: &[],
    },
    Case {
        name: "domain/related",
        input: "provider answered example.com",
        expected: &[ExpectedDetection {
            kind: SensitiveType::Domain,
            value: "example.com",
        }],
        related_domains: &["example.com"],
    },
    Case {
        name: "phone/e164",
        input: "call +14155552671 now",
        expected: &[ExpectedDetection {
            kind: SensitiveType::Phone,
            value: "+14155552671",
        }],
        related_domains: &[],
    },
    Case {
        name: "ssn/basic",
        input: "ssn 123-45-6789 for tax form",
        expected: &[ExpectedDetection {
            kind: SensitiveType::Ssn,
            value: "123-45-6789",
        }],
        related_domains: &[],
    },
    Case {
        name: "cc/basic",
        input: "card 4111-1111-1111-1111 to verify",
        expected: &[ExpectedDetection {
            kind: SensitiveType::CreditCard,
            value: "4111-1111-1111-1111",
        }],
        related_domains: &[],
    },
    Case {
        name: "api_key/openai_assignment",
        input: "OPENAI_API_KEY=sk-proj-AAAAAAAAAAAAAAAAAAAAAAAA rotate this",
        expected: &[ExpectedDetection {
            kind: SensitiveType::ApiKey,
            value: "sk-proj-AAAAAAAAAAAAAAAAAAAAAAAA",
        }],
        related_domains: &[],
    },
    Case {
        name: "api_key/openai_direct",
        input: "token sk-proj-BBBBBBBBBBBBBBBBBBBBBBBB",
        expected: &[ExpectedDetection {
            kind: SensitiveType::ApiKey,
            value: "sk-proj-BBBBBBBBBBBBBBBBBBBBBBBB",
        }],
        related_domains: &[],
    },
    Case {
        name: "api_key/google_direct",
        input: "token AIzaAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        expected: &[ExpectedDetection {
            kind: SensitiveType::ApiKey,
            value: "AIzaAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        }],
        related_domains: &[],
    },
    Case {
        name: "api_key/sendgrid_direct",
        input: "token SG.AAAAAAAAAAAAAAAAAAAAAA.bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        expected: &[ExpectedDetection {
            kind: SensitiveType::ApiKey,
            value: "SG.AAAAAAAAAAAAAAAAAAAAAA.bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        }],
        related_domains: &[],
    },
    Case {
        name: "api_key/mailgun_direct",
        input: "token key-0123456789abcdef0123456789abcdef",
        expected: &[ExpectedDetection {
            kind: SensitiveType::ApiKey,
            value: "key-0123456789abcdef0123456789abcdef",
        }],
        related_domains: &[],
    },
    Case {
        name: "api_key/aws_access_key_id",
        input: "token AKIAABCDEFGHIJKLMNOP",
        expected: &[ExpectedDetection {
            kind: SensitiveType::ApiKey,
            value: "AKIAABCDEFGHIJKLMNOP",
        }],
        related_domains: &[],
    },
    Case {
        name: "api_key/slack_webhook",
        input: "webhook https://hooks.slack.com/services/T12345678/B12345678/abcdefghijklmnopqrstuvwxyzAB",
        expected: &[ExpectedDetection {
            kind: SensitiveType::ApiKey,
            value: "https://hooks.slack.com/services/T12345678/B12345678/abcdefghijklmnopqrstuvwxyzAB",
        }],
        related_domains: &[],
    },
    Case {
        name: "api_key/db_url_with_password",
        input: "db postgres://alice:supersecret@db.example.com:5432/app",
        expected: &[ExpectedDetection {
            kind: SensitiveType::ApiKey,
            value: "postgres://alice:supersecret@db.example.com:5432/app",
        }],
        related_domains: &[],
    },
    Case {
        name: "api_key/bearer_jwt",
        input: "Authorization: Bearer aaaaaaaaaa.bbbbbbbbbb.cccccccccc",
        expected: &[ExpectedDetection {
            kind: SensitiveType::ApiKey,
            value: "aaaaaaaaaa.bbbbbbbbbb.cccccccccc",
        }],
        related_domains: &[],
    },
    Case {
        name: "api_key/pem_private_key",
        input: "-----BEGIN PRIVATE KEY-----\nQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFB\n-----END PRIVATE KEY-----",
        expected: &[ExpectedDetection {
            kind: SensitiveType::ApiKey,
            value: "-----BEGIN PRIVATE KEY-----\nQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFB\n-----END PRIVATE KEY-----",
        }],
        related_domains: &[],
    },
    Case {
        name: "negative/package_version",
        input: "packages dam@0.1.0 and dam-web-ui@0.1.0",
        expected: &[],
        related_domains: &[],
    },
    Case {
        name: "negative/invalid_ssn",
        input: "ssn 666-45-6789",
        expected: &[],
        related_domains: &[],
    },
    Case {
        name: "negative/short_sendgrid",
        input: "sendgrid token SG.short.too_short",
        expected: &[],
        related_domains: &[],
    },
    Case {
        name: "negative/db_url_without_password",
        input: "db postgres://alice@db.example.com:5432/app",
        expected: &[],
        related_domains: &[],
    },
];

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
    let mut failures = Vec::new();
    let mut per_kind_counts: BTreeMap<MetricKind, (usize, usize, usize)> = BTreeMap::new();
    let mut total_true_positives = 0usize;
    let mut total_false_positives = 0usize;
    let mut total_false_negatives = 0usize;
    let mut total_expected = 0usize;
    let mut total_actual = 0usize;

    for case in CASES {
        let related_domains = case
            .related_domains
            .iter()
            .map(|domain| (*domain).to_string())
            .collect::<Vec<_>>();
        let actual = dam_detect::detect_with_related_domains(case.input, &related_domains);
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
        .map(|(kind, (tp, fp, fn_))| {
            (
                kind,
                MetricSummary {
                    true_positives: tp,
                    false_positives: fp,
                    false_negatives: fn_,
                    precision: ratio(tp, tp + fp),
                    recall: ratio(tp, tp + fn_),
                    f1: f1(tp, fp, fn_),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();

    Ok(BenchmarkReport {
        suite: "contract_baseline",
        cases: CASES.len(),
        expected_detections: total_expected,
        actual_detections: total_actual,
        summary: MetricSummary {
            true_positives: total_true_positives,
            false_positives: total_false_positives,
            false_negatives: total_false_negatives,
            precision: ratio(
                total_true_positives,
                total_true_positives + total_false_positives,
            ),
            recall: ratio(
                total_true_positives,
                total_true_positives + total_false_negatives,
            ),
            f1: f1(
                total_true_positives,
                total_false_positives,
                total_false_negatives,
            ),
        },
        per_kind,
        failures,
    })
}

fn expected_records(case: &Case) -> Result<Vec<DetectionRecord>, String> {
    case.expected
        .iter()
        .map(|expected| {
            let start = case.input.find(expected.value).ok_or_else(|| {
                format!(
                    "case {} is missing expected value {:?}",
                    case.name, expected.value
                )
            })?;
            Ok(DetectionRecord {
                kind: expected.kind.into(),
                start,
                end: start + expected.value.len(),
                value: expected.value.to_string(),
            })
        })
        .collect()
}

fn increment_true_positive(
    counts: &mut BTreeMap<MetricKind, (usize, usize, usize)>,
    kind: MetricKind,
) {
    let entry = counts.entry(kind).or_default();
    entry.0 += 1;
}

fn increment_false_positive(
    counts: &mut BTreeMap<MetricKind, (usize, usize, usize)>,
    kind: MetricKind,
) {
    let entry = counts.entry(kind).or_default();
    entry.1 += 1;
}

fn increment_false_negative(
    counts: &mut BTreeMap<MetricKind, (usize, usize, usize)>,
    kind: MetricKind,
) {
    let entry = counts.entry(kind).or_default();
    entry.2 += 1;
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        1.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn f1(true_positives: usize, false_positives: usize, false_negatives: usize) -> f64 {
    let precision = ratio(true_positives, true_positives + false_positives);
    let recall = ratio(true_positives, true_positives + false_negatives);
    if precision + recall == 0.0 {
        0.0
    } else {
        2.0 * precision * recall / (precision + recall)
    }
}

fn print_report(report: &BenchmarkReport, format: OutputFormat) {
    match format {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(report).expect("report should serialize")
        ),
        OutputFormat::Markdown => print_markdown(report),
        OutputFormat::Text => print_text(report),
    }
}

fn print_text(report: &BenchmarkReport) {
    println!("DAM detector benchmark: {}", report.suite);
    println!("cases: {}", report.cases);
    println!("expected detections: {}", report.expected_detections);
    println!("actual detections: {}", report.actual_detections);
    println!(
        "summary: tp={} fp={} fn={} precision={:.3} recall={:.3} f1={:.3}",
        report.summary.true_positives,
        report.summary.false_positives,
        report.summary.false_negatives,
        report.summary.precision,
        report.summary.recall,
        report.summary.f1,
    );
    println!("per kind:");
    for (kind, summary) in &report.per_kind {
        println!(
            "  {:?}: tp={} fp={} fn={} precision={:.3} recall={:.3} f1={:.3}",
            kind,
            summary.true_positives,
            summary.false_positives,
            summary.false_negatives,
            summary.precision,
            summary.recall,
            summary.f1,
        );
    }
    if report.failures.is_empty() {
        println!("failures: none");
    } else {
        println!("failures:");
        for failure in &report.failures {
            println!(
                "  {} {} {:?} [{}..{}] {}",
                failure.case,
                failure.failure,
                failure.kind,
                failure.start,
                failure.end,
                failure.value
            );
        }
    }
}

fn print_markdown(report: &BenchmarkReport) {
    println!("# DAM detector benchmark\n");
    println!("- suite: `{}`", report.suite);
    println!("- cases: `{}`", report.cases);
    println!("- expected detections: `{}`", report.expected_detections);
    println!("- actual detections: `{}`", report.actual_detections);
    println!(
        "- summary: `tp={}` `fp={}` `fn={}` `precision={:.3}` `recall={:.3}` `f1={:.3}`\n",
        report.summary.true_positives,
        report.summary.false_positives,
        report.summary.false_negatives,
        report.summary.precision,
        report.summary.recall,
        report.summary.f1,
    );
    println!("## Per-kind metrics\n");
    for (kind, summary) in &report.per_kind {
        println!(
            "- `{:?}`: `tp={}` `fp={}` `fn={}` `precision={:.3}` `recall={:.3}` `f1={:.3}`",
            kind,
            summary.true_positives,
            summary.false_positives,
            summary.false_negatives,
            summary.precision,
            summary.recall,
            summary.f1,
        );
    }
    println!();
    if report.failures.is_empty() {
        println!("## Failures\n\n- none");
    } else {
        println!("## Failures\n");
        for failure in &report.failures {
            println!(
                "- `{}` `{}` `{:?}` [{}..{}] `{}`",
                failure.case,
                failure.failure,
                failure.kind,
                failure.start,
                failure.end,
                failure.value
            );
        }
    }
}
