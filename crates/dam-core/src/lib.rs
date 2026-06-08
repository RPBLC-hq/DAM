use std::{
    collections::{HashMap, HashSet},
    time::{SystemTime, UNIX_EPOCH},
};

mod normalization;

pub use normalization::canonical_sensitive_value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SensitiveType {
    Email,
    Domain,
    Phone,
    Ssn,
    CreditCard,
}

impl SensitiveType {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Email => "email",
            Self::Domain => "domain",
            Self::Phone => "phone",
            Self::Ssn => "ssn",
            Self::CreditCard => "cc",
        }
    }

    pub fn from_tag(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "email" => Some(Self::Email),
            "domain" => Some(Self::Domain),
            "phone" => Some(Self::Phone),
            "ssn" => Some(Self::Ssn),
            "cc" | "credit_card" | "credit-card" => Some(Self::CreditCard),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn overlaps(self, other: Span) -> bool {
        self.start < other.end && other.start < self.end
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detection {
    pub kind: SensitiveType,
    pub span: Span,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Reference {
    pub kind: SensitiveType,
    pub id: String,
}

impl Reference {
    pub fn generate(kind: SensitiveType) -> Self {
        loop {
            let uuid = uuid::Uuid::new_v4();
            let id = bs58::encode(uuid.as_bytes()).into_string();
            if id.len() == 22 {
                return Self { kind, id };
            }
        }
    }

    pub fn key(&self) -> String {
        format!("{}:{}", self.kind.tag(), self.id)
    }

    pub fn display(&self) -> String {
        format!("[{}]", self.key())
    }

    pub fn parse_key(value: &str) -> Option<Self> {
        let (kind, id) = value.split_once(':')?;
        let kind = SensitiveType::from_tag(kind)?;
        if !valid_reference_id(id) {
            return None;
        }

        Some(Self {
            kind,
            id: id.to_string(),
        })
    }

    pub fn parse_display(value: &str) -> Option<Self> {
        let key = value.strip_prefix('[')?.strip_suffix(']')?;
        Self::parse_key(key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultRecord {
    pub reference: Reference,
    pub kind: SensitiveType,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VaultWriteOptions {
    pub deduplicate: bool,
}

impl Default for VaultWriteOptions {
    fn default() -> Self {
        Self { deduplicate: true }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct VaultWriteError {
    pub message: String,
}

impl VaultWriteError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

pub trait VaultWriter: Send + Sync {
    fn write(&self, record: &VaultRecord) -> Result<Reference, VaultWriteError> {
        self.write_with_options(record, VaultWriteOptions::default())
    }

    fn write_with_options(
        &self,
        record: &VaultRecord,
        options: VaultWriteOptions,
    ) -> Result<Reference, VaultWriteError>;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct VaultReadError {
    pub message: String,
}

impl VaultReadError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

pub trait VaultReader: Send + Sync {
    fn read(&self, reference: &Reference) -> Result<Option<String>, VaultReadError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogEventType {
    Detection,
    PolicyDecision,
    VaultWrite,
    VaultWriteFailed,
    VaultRead,
    VaultReadFailed,
    Consent,
    Redaction,
    Resolve,
    ProxyForward,
    ProxyBypass,
    ProxyFailure,
}

impl LogEventType {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Detection => "detection",
            Self::PolicyDecision => "policy_decision",
            Self::VaultWrite => "vault_write",
            Self::VaultWriteFailed => "vault_write_failed",
            Self::VaultRead => "vault_read",
            Self::VaultReadFailed => "vault_read_failed",
            Self::Consent => "consent",
            Self::Redaction => "redaction",
            Self::Resolve => "resolve",
            Self::ProxyForward => "proxy_forward",
            Self::ProxyBypass => "proxy_bypass",
            Self::ProxyFailure => "proxy_failure",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEvent {
    pub timestamp: i64,
    pub operation_id: String,
    pub level: LogLevel,
    pub event_type: LogEventType,
    pub kind: Option<SensitiveType>,
    pub value: Option<String>,
    pub reference: Option<Reference>,
    pub action: Option<String>,
    pub message: String,
}

impl LogEvent {
    pub fn new(
        operation_id: impl Into<String>,
        level: LogLevel,
        event_type: LogEventType,
        message: impl Into<String>,
    ) -> Self {
        Self {
            timestamp: now_unix_secs(),
            operation_id: operation_id.into(),
            level,
            event_type,
            kind: None,
            value: None,
            reference: None,
            action: None,
            message: message.into(),
        }
    }

    pub fn with_kind(mut self, kind: SensitiveType) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    pub fn with_reference(mut self, reference: Reference) -> Self {
        self.reference = Some(reference);
        self
    }

    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.action = Some(action.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct LogWriteError {
    pub message: String,
}

impl LogWriteError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

pub trait EventSink: Send + Sync {
    fn record(&self, event: &LogEvent) -> Result<(), LogWriteError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PolicyAction {
    Tokenize,
    Redact,
    Allow,
    Block,
}

impl PolicyAction {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Tokenize => "tokenize",
            Self::Redact => "redact",
            Self::Allow => "allow",
            Self::Block => "block",
        }
    }

    pub fn from_tag(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "tokenize" => Some(Self::Tokenize),
            "redact" => Some(Self::Redact),
            "allow" => Some(Self::Allow),
            "block" => Some(Self::Block),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyDecision {
    pub detection: Detection,
    pub action: PolicyAction,
}

impl PolicyDecision {
    pub fn new(detection: Detection, action: PolicyAction) -> Self {
        Self { detection, action }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplacementMode {
    Tokenized,
    Redacted,
    RedactOnlyFallback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replacement {
    pub span: Span,
    pub text: String,
    pub mode: ReplacementMode,
    pub reference: Option<Reference>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultFailure {
    pub kind: SensitiveType,
    pub value_preview: String,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockedDetection {
    pub kind: SensitiveType,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReplacementPlan {
    pub replacements: Vec<Replacement>,
    pub vault_failures: Vec<VaultFailure>,
    pub blocked: Vec<BlockedDetection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplacementPlanOptions {
    pub deduplicate_replacements: bool,
}

impl Default for ReplacementPlanOptions {
    fn default() -> Self {
        Self {
            deduplicate_replacements: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ReplacementDedupKey {
    kind: SensitiveType,
    action: PolicyAction,
    value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CachedReplacement {
    Tokenized(Reference),
    RedactOnlyFallback,
    Redacted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceMatch {
    pub span: Span,
    pub reference: Reference,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveReplacement {
    pub span: Span,
    pub text: String,
    pub reference: Reference,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingReference {
    pub span: Span,
    pub reference: Reference,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultReadFailure {
    pub span: Span,
    pub reference: Reference,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResolvePlan {
    pub references: Vec<ReferenceMatch>,
    pub replacements: Vec<ResolveReplacement>,
    pub missing: Vec<MissingReference>,
    pub read_failures: Vec<VaultReadFailure>,
}

impl ReplacementPlan {
    pub fn tokenized_count(&self) -> usize {
        self.replacements
            .iter()
            .filter(|r| r.mode == ReplacementMode::Tokenized)
            .count()
    }

    pub fn fallback_count(&self) -> usize {
        self.replacements
            .iter()
            .filter(|r| r.mode == ReplacementMode::RedactOnlyFallback)
            .count()
    }

    pub fn redacted_count(&self) -> usize {
        self.replacements
            .iter()
            .filter(|r| r.mode == ReplacementMode::Redacted)
            .count()
    }

    pub fn blocked_count(&self) -> usize {
        self.blocked.len()
    }

    pub fn vault_write_count(&self) -> usize {
        self.replacements
            .iter()
            .filter_map(|replacement| replacement.reference.as_ref())
            .map(Reference::key)
            .collect::<HashSet<_>>()
            .len()
    }
}

impl ResolvePlan {
    pub fn resolved_count(&self) -> usize {
        self.replacements.len()
    }

    pub fn missing_count(&self) -> usize {
        self.missing.len()
    }

    pub fn read_failure_count(&self) -> usize {
        self.read_failures.len()
    }

    pub fn has_unresolved(&self) -> bool {
        !self.missing.is_empty() || !self.read_failures.is_empty()
    }
}

pub fn find_references(input: &str) -> Vec<ReferenceMatch> {
    let mut matches = Vec::new();
    let mut cursor = 0;

    while cursor < input.len() {
        let Some(start_offset) = input[cursor..].find('[') else {
            break;
        };
        let start = cursor + start_offset;
        let content_start = start + 1;
        let Some(end_offset) = input[content_start..].find(']') else {
            break;
        };
        let end = content_start + end_offset;
        let display_end = end + 1;
        let escaped_start = start > 0 && input.as_bytes()[start - 1] == b'\\';
        let display_start = if escaped_start { start - 1 } else { start };
        let key_end = if end > content_start && input.as_bytes()[end - 1] == b'\\' {
            end - 1
        } else {
            end
        };

        if let Some(reference) = Reference::parse_key(&input[content_start..key_end]) {
            matches.push(ReferenceMatch {
                span: Span {
                    start: display_start,
                    end: display_end,
                },
                reference,
            });
            cursor = display_end;
        } else {
            cursor = content_start;
        }
    }

    matches
}

pub fn build_resolve_plan(input: &str, vault: &(impl VaultReader + ?Sized)) -> ResolvePlan {
    let references = find_references(input);
    let mut plan = ResolvePlan {
        references: references.clone(),
        ..ResolvePlan::default()
    };

    for reference_match in references {
        match vault.read(&reference_match.reference) {
            Ok(Some(value)) => plan.replacements.push(ResolveReplacement {
                span: reference_match.span,
                text: value,
                reference: reference_match.reference,
            }),
            Ok(None) => plan.missing.push(MissingReference {
                span: reference_match.span,
                reference: reference_match.reference,
            }),
            Err(error) => plan.read_failures.push(VaultReadFailure {
                span: reference_match.span,
                reference: reference_match.reference,
                error: error.to_string(),
            }),
        }
    }

    plan
}

pub fn apply_resolve_plan(input: &str, plan: &ResolvePlan) -> String {
    let mut output = input.to_string();
    let mut sorted = plan.replacements.iter().collect::<Vec<_>>();
    sorted.sort_by_key(|detection| std::cmp::Reverse(detection.span.start));

    for replacement in sorted {
        if replacement.span.start <= output.len()
            && replacement.span.end <= output.len()
            && replacement.span.start <= replacement.span.end
        {
            output.replace_range(
                replacement.span.start..replacement.span.end,
                &replacement.text,
            );
        }
    }

    output
}

pub fn build_resolve_log_events(operation_id: &str, plan: &ResolvePlan) -> Vec<LogEvent> {
    let mut events = Vec::with_capacity(
        plan.replacements.len() * 2 + plan.missing.len() + plan.read_failures.len(),
    );

    for replacement in &plan.replacements {
        events.push(
            LogEvent::new(
                operation_id,
                LogLevel::Info,
                LogEventType::VaultRead,
                "vault read succeeded",
            )
            .with_kind(replacement.reference.kind)
            .with_reference(replacement.reference.clone())
            .with_action("vault_read_succeeded"),
        );
        events.push(
            LogEvent::new(
                operation_id,
                LogLevel::Info,
                LogEventType::Resolve,
                "reference resolved",
            )
            .with_kind(replacement.reference.kind)
            .with_reference(replacement.reference.clone())
            .with_action("resolved"),
        );
    }

    for missing in &plan.missing {
        events.push(
            LogEvent::new(
                operation_id,
                LogLevel::Warn,
                LogEventType::Resolve,
                "reference missing from vault",
            )
            .with_kind(missing.reference.kind)
            .with_reference(missing.reference.clone())
            .with_action("missing"),
        );
    }

    for failure in &plan.read_failures {
        events.push(
            LogEvent::new(
                operation_id,
                LogLevel::Warn,
                LogEventType::VaultReadFailed,
                "vault read failed",
            )
            .with_kind(failure.reference.kind)
            .with_reference(failure.reference.clone())
            .with_action("vault_read_failed"),
        );
    }

    events
}

pub fn build_replacement_plan(
    detections: &[Detection],
    vault: &(impl VaultWriter + ?Sized),
) -> ReplacementPlan {
    build_replacement_plan_with_options(detections, vault, ReplacementPlanOptions::default())
}

pub fn build_replacement_plan_with_options(
    detections: &[Detection],
    vault: &(impl VaultWriter + ?Sized),
    options: ReplacementPlanOptions,
) -> ReplacementPlan {
    let decisions = detections
        .iter()
        .cloned()
        .map(|detection| PolicyDecision::new(detection, PolicyAction::Tokenize))
        .collect::<Vec<_>>();
    build_replacement_plan_from_decisions_with_options(&decisions, vault, options)
}

pub fn build_replacement_plan_from_decisions(
    decisions: &[PolicyDecision],
    vault: &(impl VaultWriter + ?Sized),
) -> ReplacementPlan {
    build_replacement_plan_from_decisions_with_options(
        decisions,
        vault,
        ReplacementPlanOptions::default(),
    )
}

pub fn build_replacement_plan_from_decisions_with_options(
    decisions: &[PolicyDecision],
    vault: &(impl VaultWriter + ?Sized),
    options: ReplacementPlanOptions,
) -> ReplacementPlan {
    let mut plan = ReplacementPlan::default();
    let mut dedup_cache = HashMap::<ReplacementDedupKey, CachedReplacement>::new();

    for decision in decisions {
        let detection = &decision.detection;
        match decision.action {
            PolicyAction::Tokenize => {
                let dedup_key = options
                    .deduplicate_replacements
                    .then(|| ReplacementDedupKey::from_decision(decision));
                if let Some(cached) = dedup_key
                    .as_ref()
                    .and_then(|key| dedup_cache.get(key))
                    .cloned()
                {
                    plan.replacements.push(cached.into_replacement(detection));
                    continue;
                }

                let reference = Reference::generate(detection.kind);
                let stored_value = canonical_sensitive_value(detection.kind, &detection.value);
                let record = VaultRecord {
                    reference: reference.clone(),
                    kind: detection.kind,
                    value: stored_value.clone(),
                };

                let write_options = VaultWriteOptions {
                    deduplicate: options.deduplicate_replacements,
                };
                match vault.write_with_options(&record, write_options) {
                    Ok(reference) => {
                        if let Some(key) = dedup_key {
                            dedup_cache
                                .insert(key, CachedReplacement::Tokenized(reference.clone()));
                        }
                        plan.replacements.push(Replacement {
                            span: detection.span,
                            text: reference.display(),
                            mode: ReplacementMode::Tokenized,
                            reference: Some(reference),
                        });
                    }
                    Err(error) => {
                        if let Some(key) = dedup_key {
                            dedup_cache.insert(key, CachedReplacement::RedactOnlyFallback);
                        }
                        plan.vault_failures.push(VaultFailure {
                            kind: detection.kind,
                            value_preview: preview(&stored_value),
                            error: error.to_string(),
                        });
                        plan.replacements.push(Replacement {
                            span: detection.span,
                            text: redacted_placeholder(detection.kind),
                            mode: ReplacementMode::RedactOnlyFallback,
                            reference: None,
                        });
                    }
                }
            }
            PolicyAction::Redact => {
                let dedup_key = options
                    .deduplicate_replacements
                    .then(|| ReplacementDedupKey::from_decision(decision));
                if let Some(cached) = dedup_key
                    .as_ref()
                    .and_then(|key| dedup_cache.get(key))
                    .cloned()
                {
                    plan.replacements.push(cached.into_replacement(detection));
                    continue;
                }
                if let Some(key) = dedup_key {
                    dedup_cache.insert(key, CachedReplacement::Redacted);
                }
                plan.replacements
                    .push(CachedReplacement::Redacted.into_replacement(detection));
            }
            PolicyAction::Allow => {}
            PolicyAction::Block => {
                plan.blocked.push(BlockedDetection {
                    kind: detection.kind,
                    span: detection.span,
                });
            }
        }
    }

    plan
}

impl ReplacementDedupKey {
    fn from_decision(decision: &PolicyDecision) -> Self {
        Self {
            kind: decision.detection.kind,
            action: decision.action,
            value: canonical_sensitive_value(decision.detection.kind, &decision.detection.value),
        }
    }
}

impl CachedReplacement {
    fn into_replacement(self, detection: &Detection) -> Replacement {
        match self {
            Self::Tokenized(reference) => Replacement {
                span: detection.span,
                text: reference.display(),
                mode: ReplacementMode::Tokenized,
                reference: Some(reference),
            },
            Self::RedactOnlyFallback => Replacement {
                span: detection.span,
                text: redacted_placeholder(detection.kind),
                mode: ReplacementMode::RedactOnlyFallback,
                reference: None,
            },
            Self::Redacted => Replacement {
                span: detection.span,
                text: redacted_placeholder(detection.kind),
                mode: ReplacementMode::Redacted,
                reference: None,
            },
        }
    }
}

pub fn redacted_placeholder(kind: SensitiveType) -> String {
    format!("[{}]", kind.tag())
}

pub fn generate_operation_id() -> String {
    loop {
        let uuid = uuid::Uuid::new_v4();
        let id = bs58::encode(uuid.as_bytes()).into_string();
        if id.len() == 22 {
            return id;
        }
    }
}

pub fn build_filter_log_events(
    operation_id: &str,
    detections: &[Detection],
    plan: &ReplacementPlan,
) -> Vec<LogEvent> {
    let decisions = detections
        .iter()
        .cloned()
        .map(|detection| PolicyDecision::new(detection, PolicyAction::Tokenize))
        .collect::<Vec<_>>();
    build_filter_log_events_from_decisions(operation_id, &decisions, plan)
}

pub fn build_filter_log_events_from_decisions(
    operation_id: &str,
    decisions: &[PolicyDecision],
    plan: &ReplacementPlan,
) -> Vec<LogEvent> {
    let mut events = Vec::with_capacity(decisions.len() * 2 + plan.replacements.len() * 2);
    let mut logged_vault_writes = HashSet::<String>::new();

    for decision in decisions {
        let detection = &decision.detection;
        events.push(
            LogEvent::new(
                operation_id,
                LogLevel::Info,
                LogEventType::Detection,
                format!(
                    "sensitive value detected at span {}..{}",
                    detection.span.start, detection.span.end
                ),
            )
            .with_kind(detection.kind)
            .with_value(canonical_sensitive_value(detection.kind, &detection.value))
            .with_action("detected"),
        );

        events.push(
            LogEvent::new(
                operation_id,
                LogLevel::Info,
                LogEventType::PolicyDecision,
                "policy decision applied",
            )
            .with_kind(detection.kind)
            .with_value(canonical_sensitive_value(detection.kind, &detection.value))
            .with_action(decision.action.tag()),
        );
    }

    for replacement in &plan.replacements {
        let detection = detection_for_replacement(replacement, decisions);
        let kind = replacement
            .reference
            .as_ref()
            .map(|reference| reference.kind)
            .or_else(|| detection.map(|detection| detection.kind));
        let value =
            detection.map(|detection| canonical_sensitive_value(detection.kind, &detection.value));
        match replacement.mode {
            ReplacementMode::Tokenized => {
                let reference = replacement
                    .reference
                    .clone()
                    .expect("tokenized replacements must carry a reference");
                if logged_vault_writes.insert(reference.key()) {
                    let mut vault_event = LogEvent::new(
                        operation_id,
                        LogLevel::Info,
                        LogEventType::VaultWrite,
                        "vault write succeeded",
                    )
                    .with_reference(reference.clone())
                    .with_action("vault_write_succeeded");
                    if let Some(kind) = kind {
                        vault_event = vault_event.with_kind(kind);
                    }
                    events.push(vault_event);
                }

                let mut redaction_event = LogEvent::new(
                    operation_id,
                    LogLevel::Info,
                    LogEventType::Redaction,
                    "replacement applied with tokenized reference",
                )
                .with_reference(reference)
                .with_action("tokenized");
                if let Some(kind) = kind {
                    redaction_event = redaction_event.with_kind(kind);
                }
                if let Some(value) = value.clone() {
                    redaction_event = redaction_event.with_value(value);
                }
                events.push(redaction_event);
            }
            ReplacementMode::Redacted => {
                let mut redaction_event = LogEvent::new(
                    operation_id,
                    LogLevel::Info,
                    LogEventType::Redaction,
                    "replacement applied with policy redaction",
                )
                .with_action("redacted");
                if let Some(kind) = kind {
                    redaction_event = redaction_event.with_kind(kind);
                }
                if let Some(value) = value.clone() {
                    redaction_event = redaction_event.with_value(value);
                }
                events.push(redaction_event);
            }
            ReplacementMode::RedactOnlyFallback => {
                let mut redaction_event = LogEvent::new(
                    operation_id,
                    LogLevel::Warn,
                    LogEventType::Redaction,
                    "replacement applied with redact-only fallback",
                )
                .with_action("fallback_redacted");
                if let Some(kind) = kind {
                    redaction_event = redaction_event.with_kind(kind);
                }
                if let Some(value) = value.clone() {
                    redaction_event = redaction_event.with_value(value);
                }
                events.push(redaction_event);
            }
        }
    }

    for failure in &plan.vault_failures {
        events.push(
            LogEvent::new(
                operation_id,
                LogLevel::Warn,
                LogEventType::VaultWriteFailed,
                "vault write failed; redact-only fallback used",
            )
            .with_kind(failure.kind)
            .with_action("vault_write_failed"),
        );
    }

    events
}

fn detection_for_replacement<'a>(
    replacement: &Replacement,
    decisions: &'a [PolicyDecision],
) -> Option<&'a Detection> {
    decisions
        .iter()
        .find(|decision| decision.detection.span == replacement.span)
        .map(|decision| &decision.detection)
}

fn preview(value: &str) -> String {
    let mut preview = value.chars().take(4).collect::<String>();
    if value.chars().count() > 4 {
        preview.push_str("...");
    }
    preview
}

fn valid_reference_id(id: &str) -> bool {
    if id.len() != 22 {
        return false;
    }

    bs58::decode(id)
        .into_vec()
        .map(|bytes| bytes.len() == 16)
        .unwrap_or(false)
}

fn now_unix_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
