use dam_core::{
    EventSink, LogEvent, LogEventType, LogLevel, PolicyAction, PolicyDecision, ReplacementPlan,
    ReplacementPlanOptions, ResolvePlan, VaultReader, VaultWriter,
};
use dam_policy::PolicyEngine;

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("consent check failed: {0}")]
    Consent(#[from] dam_consent::ConsentError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtectTextStatus {
    Protected,
    Blocked,
}

#[derive(Debug, Clone)]
pub struct ProtectTextResult {
    pub status: ProtectTextStatus,
    pub output: Option<String>,
    pub detections: Vec<dam_core::Detection>,
    pub decisions: Vec<PolicyDecision>,
    pub plan: ReplacementPlan,
    pub consent_matches: Vec<dam_consent::ConsentMatch>,
}

impl ProtectTextResult {
    pub fn is_blocked(&self) -> bool {
        self.status == ProtectTextStatus::Blocked
    }
}

#[derive(Debug, Clone)]
pub struct ResolveTextResult {
    pub output: Option<String>,
    pub plan: ResolvePlan,
}

#[derive(Clone, Copy, Default)]
pub struct ProtectTextContext<'a> {
    pub reference_vault: Option<&'a dyn VaultReader>,
    pub consent_store: Option<&'a dam_consent::ConsentStore>,
    pub event_sink: Option<&'a dyn EventSink>,
    pub related_domains: &'a [String],
}

pub fn protect_text(
    input: &str,
    operation_id: &str,
    policy: &dyn PolicyEngine,
    vault: &dyn VaultWriter,
    context: ProtectTextContext<'_>,
    options: ReplacementPlanOptions,
) -> Result<ProtectTextResult, PipelineError> {
    let protected_input =
        expand_allowed_references(input, context.consent_store, context.reference_vault)?
            .unwrap_or_else(|| input.to_string());
    let input = protected_input.as_str();
    let detections = dam_detect::detect_with_related_domains(input, context.related_domains);
    let base_decisions = policy.decide_all(&detections);
    let (decisions, consent_matches) =
        dam_consent::apply_consents_to_decisions(&base_decisions, context.consent_store)?;

    if decisions
        .iter()
        .any(|decision| decision.action == PolicyAction::Block)
    {
        let plan = blocked_plan_from_decisions(&decisions);
        record_filter_events(
            context.event_sink,
            operation_id,
            &decisions,
            &plan,
            &consent_matches,
        );
        return Ok(ProtectTextResult {
            status: ProtectTextStatus::Blocked,
            output: None,
            detections,
            decisions,
            plan,
            consent_matches,
        });
    }

    let plan =
        dam_core::build_replacement_plan_from_decisions_with_options(&decisions, vault, options);
    record_filter_events(
        context.event_sink,
        operation_id,
        &decisions,
        &plan,
        &consent_matches,
    );
    let output = dam_redact::redact(input, &plan.replacements);

    Ok(ProtectTextResult {
        status: ProtectTextStatus::Protected,
        output: Some(output),
        detections,
        decisions,
        plan,
        consent_matches,
    })
}

fn expand_allowed_references(
    input: &str,
    consent_store: Option<&dam_consent::ConsentStore>,
    vault: Option<&dyn VaultReader>,
) -> Result<Option<String>, PipelineError> {
    let (Some(consent_store), Some(vault)) = (consent_store, vault) else {
        return Ok(None);
    };

    let references = dam_core::find_references(input);
    if references.is_empty() {
        return Ok(None);
    }

    let mut replacements = Vec::<dam_core::ResolveReplacement>::new();
    for reference_match in references {
        let Ok(Some(value)) = vault.read(&reference_match.reference) else {
            continue;
        };
        if consent_store
            .active_for_value(reference_match.reference.kind, &value)?
            .is_some()
        {
            replacements.push(dam_core::ResolveReplacement {
                span: reference_match.span,
                text: value,
                reference: reference_match.reference,
            });
        }
    }

    if replacements.is_empty() {
        return Ok(None);
    }

    Ok(Some(dam_core::apply_resolve_plan(
        input,
        &ResolvePlan {
            replacements,
            ..ResolvePlan::default()
        },
    )))
}

pub fn resolve_text(
    input: &str,
    operation_id: &str,
    vault: &dyn VaultReader,
    event_sink: Option<&dyn EventSink>,
) -> ResolveTextResult {
    let plan = dam_core::build_resolve_plan(input, vault);
    if plan.references.is_empty() {
        return ResolveTextResult { output: None, plan };
    }

    record_resolve_events(event_sink, operation_id, &plan);
    if plan.resolved_count() == 0 {
        return ResolveTextResult { output: None, plan };
    }

    let output = dam_core::apply_resolve_plan(input, &plan);
    ResolveTextResult {
        output: Some(output),
        plan,
    }
}

pub fn blocked_plan_from_decisions(decisions: &[PolicyDecision]) -> ReplacementPlan {
    ReplacementPlan {
        blocked: decisions
            .iter()
            .filter(|decision| decision.action == PolicyAction::Block)
            .map(|decision| dam_core::BlockedDetection {
                kind: decision.detection.kind,
                span: decision.detection.span,
            })
            .collect(),
        ..ReplacementPlan::default()
    }
}

pub fn record_filter_events(
    event_sink: Option<&dyn EventSink>,
    operation_id: &str,
    decisions: &[PolicyDecision],
    plan: &ReplacementPlan,
    consent_matches: &[dam_consent::ConsentMatch],
) {
    let Some(sink) = event_sink else {
        return;
    };

    for event in dam_core::build_filter_log_events_from_decisions(operation_id, decisions, plan) {
        let _ = sink.record(&event);
    }

    for consent_match in consent_matches {
        let event = LogEvent::new(
            operation_id,
            LogLevel::Info,
            LogEventType::Consent,
            "active consent allowed detected value",
        )
        .with_kind(consent_match.kind)
        .with_action(format!("allow:{}", consent_match.consent_id));
        let _ = sink.record(&event);
    }
}

pub fn record_resolve_events(
    event_sink: Option<&dyn EventSink>,
    operation_id: &str,
    plan: &ResolvePlan,
) {
    let Some(sink) = event_sink else {
        return;
    };

    for event in dam_core::build_resolve_log_events(operation_id, plan) {
        let _ = sink.record(&event);
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
