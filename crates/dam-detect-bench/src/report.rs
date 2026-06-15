use std::collections::BTreeMap;

use serde::Serialize;

use crate::metrics::{MetricKind, MetricSummary};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputFormat {
    Text,
    Json,
    Markdown,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct FailureRecord {
    pub(crate) case: String,
    pub(crate) failure: String,
    pub(crate) kind: MetricKind,
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) value: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct BenchmarkReport {
    pub(crate) suite: &'static str,
    pub(crate) cases: usize,
    pub(crate) expected_detections: usize,
    pub(crate) actual_detections: usize,
    pub(crate) summary: MetricSummary,
    pub(crate) per_kind: BTreeMap<MetricKind, MetricSummary>,
    pub(crate) failures: Vec<FailureRecord>,
}

pub(crate) fn print_report(report: &BenchmarkReport, format: OutputFormat) {
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
