use std::collections::BTreeMap;
use std::io::{self, Write};

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
    print_report_to(report, format, &mut io::stdout());
}

pub(crate) fn print_report_to(report: &BenchmarkReport, format: OutputFormat, out: &mut dyn Write) {
    match format {
        OutputFormat::Json => writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(report).expect("report should serialize")
        )
        .unwrap(),
        OutputFormat::Markdown => write_markdown(report, out),
        OutputFormat::Text => write_text(report, out),
    }
}

fn write_text(report: &BenchmarkReport, out: &mut dyn Write) {
    writeln!(out, "DAM detector benchmark: {}", report.suite).unwrap();
    writeln!(out, "cases: {}", report.cases).unwrap();
    writeln!(out, "expected detections: {}", report.expected_detections).unwrap();
    writeln!(out, "actual detections: {}", report.actual_detections).unwrap();
    writeln!(
        out,
        "summary: tp={} fp={} fn={} precision={:.3} recall={:.3} f1={:.3}",
        report.summary.true_positives,
        report.summary.false_positives,
        report.summary.false_negatives,
        report.summary.precision,
        report.summary.recall,
        report.summary.f1,
    )
    .unwrap();
    writeln!(out, "per kind:").unwrap();
    for (kind, summary) in &report.per_kind {
        writeln!(
            out,
            "  {:?}: tp={} fp={} fn={} precision={:.3} recall={:.3} f1={:.3}",
            kind,
            summary.true_positives,
            summary.false_positives,
            summary.false_negatives,
            summary.precision,
            summary.recall,
            summary.f1,
        )
        .unwrap();
    }
    if report.failures.is_empty() {
        writeln!(out, "failures: none").unwrap();
    } else {
        writeln!(out, "failures:").unwrap();
        for failure in &report.failures {
            writeln!(
                out,
                "  {} {} {:?} [{}..{}] {}",
                failure.case,
                failure.failure,
                failure.kind,
                failure.start,
                failure.end,
                failure.value
            )
            .unwrap();
        }
    }
}

fn write_markdown(report: &BenchmarkReport, out: &mut dyn Write) {
    writeln!(out, "# DAM detector benchmark\n").unwrap();
    writeln!(out, "- suite: `{}`", report.suite).unwrap();
    writeln!(out, "- cases: `{}`", report.cases).unwrap();
    writeln!(out, "- expected detections: `{}`", report.expected_detections).unwrap();
    writeln!(out, "- actual detections: `{}`", report.actual_detections).unwrap();
    writeln!(
        out,
        "- summary: `tp={}` `fp={}` `fn={}` `precision={:.3}` `recall={:.3}` `f1={:.3}`\n",
        report.summary.true_positives,
        report.summary.false_positives,
        report.summary.false_negatives,
        report.summary.precision,
        report.summary.recall,
        report.summary.f1,
    )
    .unwrap();
    writeln!(out, "## Per-kind metrics\n").unwrap();
    for (kind, summary) in &report.per_kind {
        writeln!(
            out,
            "- `{:?}`: `tp={}` `fp={}` `fn={}` `precision={:.3}` `recall={:.3}` `f1={:.3}`",
            kind,
            summary.true_positives,
            summary.false_positives,
            summary.false_negatives,
            summary.precision,
            summary.recall,
            summary.f1,
        )
        .unwrap();
    }
    writeln!(out).unwrap();
    if report.failures.is_empty() {
        writeln!(out, "## Failures\n\n- none").unwrap();
    } else {
        writeln!(out, "## Failures\n").unwrap();
        for failure in &report.failures {
            writeln!(
                out,
                "- `{}` `{}` `{:?}` [{}..{}] `{}`",
                failure.case,
                failure.failure,
                failure.kind,
                failure.start,
                failure.end,
                failure.value
            )
            .unwrap();
        }
    }
}

#[cfg(test)]
#[path = "report_tests.rs"]
mod tests;
