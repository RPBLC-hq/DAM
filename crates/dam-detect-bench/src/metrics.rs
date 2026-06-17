use dam_core::{SensitiveType, Span};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MetricKind {
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
pub(crate) struct DetectionKey {
    pub(crate) kind: MetricKind,
    pub(crate) span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DetectionRecord {
    pub(crate) kind: MetricKind,
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) value: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MetricSummary {
    pub(crate) true_positives: usize,
    pub(crate) false_positives: usize,
    pub(crate) false_negatives: usize,
    pub(crate) precision: f64,
    pub(crate) recall: f64,
    pub(crate) f1: f64,
}

pub(crate) fn increment_true_positive(
    counts: &mut std::collections::BTreeMap<MetricKind, (usize, usize, usize)>,
    kind: MetricKind,
) {
    let entry = counts.entry(kind).or_default();
    entry.0 += 1;
}

pub(crate) fn increment_false_positive(
    counts: &mut std::collections::BTreeMap<MetricKind, (usize, usize, usize)>,
    kind: MetricKind,
) {
    let entry = counts.entry(kind).or_default();
    entry.1 += 1;
}

pub(crate) fn increment_false_negative(
    counts: &mut std::collections::BTreeMap<MetricKind, (usize, usize, usize)>,
    kind: MetricKind,
) {
    let entry = counts.entry(kind).or_default();
    entry.2 += 1;
}

pub(crate) fn summary(
    true_positives: usize,
    false_positives: usize,
    false_negatives: usize,
) -> MetricSummary {
    MetricSummary {
        true_positives,
        false_positives,
        false_negatives,
        precision: ratio(true_positives, true_positives + false_positives),
        recall: ratio(true_positives, true_positives + false_negatives),
        f1: f1(true_positives, false_positives, false_negatives),
    }
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
