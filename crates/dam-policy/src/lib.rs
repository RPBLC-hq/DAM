use dam_core::{Detection, PolicyAction, PolicyDecision, SensitiveType};
use std::collections::HashMap;

pub trait PolicyEngine: Send + Sync {
    fn decide(&self, detection: &Detection) -> PolicyDecision;

    fn decide_all(&self, detections: &[Detection]) -> Vec<PolicyDecision> {
        detections
            .iter()
            .map(|detection| self.decide(detection))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticPolicy {
    default_action: PolicyAction,
    kind_actions: HashMap<SensitiveType, PolicyAction>,
}

impl StaticPolicy {
    pub fn new(default_action: PolicyAction) -> Self {
        Self {
            default_action,
            kind_actions: HashMap::new(),
        }
    }

    pub fn with_kind_action(mut self, kind: SensitiveType, action: PolicyAction) -> Self {
        self.kind_actions.insert(kind, action);
        self
    }

    pub fn default_action(&self) -> PolicyAction {
        self.default_action
    }

    pub fn kind_action(&self, kind: SensitiveType) -> PolicyAction {
        self.kind_actions
            .get(&kind)
            .copied()
            .unwrap_or(self.default_action)
    }
}

impl PolicyEngine for StaticPolicy {
    fn decide(&self, detection: &Detection) -> PolicyDecision {
        PolicyDecision::new(detection.clone(), self.kind_action(detection.kind))
    }
}

impl From<dam_config::PolicyConfig> for StaticPolicy {
    fn from(config: dam_config::PolicyConfig) -> Self {
        let mut policy = Self::new(config.default_action);
        for (kind, action) in config.kind_actions {
            policy = policy.with_kind_action(kind, action);
        }
        policy
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
