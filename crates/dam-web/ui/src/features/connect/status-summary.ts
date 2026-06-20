import type { ConnectView, SetupPlan, SetupStep } from './types.ts'

type SetupSummaryKind =
  | 'requested'
  | 'waiting_for_approval'
  | 'waiting_for_reboot'
  | 'configured'
  | 'enabled'
  | 'connected'
  | 'rolled_back'
  | 'failed'
  | 'setup_needed'

export function connectStatusMessageKey(view: ConnectView) {
  switch (view.state) {
    case 'protected':
      return 'connect.protectedStatus'
    case 'paused':
      return 'connect.pausedStatus'
    case 'degraded':
      return statusKeyForSetupSummary(summarizeSetupPlan(view.setup_plan), 'degraded')
    case 'needs_setup':
      return statusKeyForSetupSummary(summarizeSetupPlan(view.setup_plan), 'needs_setup')
    case 'disconnected':
    default:
      return 'connect.disconnectedLede'
  }
}

export function connectNavLabelKey(view: ConnectView) {
  switch (view.state) {
    case 'protected':
      return 'nav.protected'
    case 'paused':
      return 'nav.paused'
    case 'disconnected':
      return 'nav.off'
    case 'degraded':
      return navKeyForSetupSummary(summarizeSetupPlan(view.setup_plan), 'degraded')
    case 'needs_setup':
      return navKeyForSetupSummary(summarizeSetupPlan(view.setup_plan), 'needs_setup')
    default:
      return 'nav.off'
  }
}

function statusKeyForSetupSummary(
  summary: SetupSummaryKind | null,
  fallbackState: 'degraded' | 'needs_setup',
) {
  switch (summary) {
    case 'requested':
      return 'connect.summary.requested'
    case 'waiting_for_approval':
      return 'connect.summary.waiting_for_approval'
    case 'waiting_for_reboot':
      return 'connect.summary.waiting_for_reboot'
    case 'configured':
      return 'connect.summary.configured'
    case 'enabled':
      return 'connect.summary.enabled'
    case 'connected':
      return 'connect.summary.connected'
    case 'rolled_back':
      return 'connect.summary.rolled_back'
    case 'failed':
      return 'connect.summary.failed'
    case 'setup_needed':
      return 'connect.setupStatus'
    default:
      return fallbackState === 'degraded' ? 'connect.degradedStatus' : 'connect.setupStatus'
  }
}

function navKeyForSetupSummary(
  summary: SetupSummaryKind | null,
  fallbackState: 'degraded' | 'needs_setup',
) {
  switch (summary) {
    case 'waiting_for_approval':
      return 'nav.approvalNeeded'
    case 'rolled_back':
    case 'failed':
      return 'nav.repairNeeded'
    case 'requested':
    case 'configured':
    case 'enabled':
    case 'connected':
    case 'waiting_for_reboot':
      return 'nav.connecting'
    case 'setup_needed':
      return 'nav.setupNeeded'
    default:
      return fallbackState === 'degraded' ? 'nav.attention' : 'nav.setupNeeded'
  }
}

function summarizeSetupPlan(plan: SetupPlan | null): SetupSummaryKind | null {
  if (!plan) return null

  const current = currentSetupStep(plan)
  if (current) {
    if (current.detail === 'waiting_for_approval') return 'waiting_for_approval'
    if (current.detail === 'waiting_for_reboot') return 'waiting_for_reboot'
    if (current.detail === 'rolled_back') return 'rolled_back'
    if (current.detail === 'failed' && (current.state === 'blocked' || current.state === 'failed')) {
      return 'failed'
    }
    if (current.detail === 'requested') return 'requested'
  }

  if (hasCompletedDetail(plan.steps, 'connected', 'ne_start')) return 'connected'
  if (hasCompletedDetail(plan.steps, 'enabled', 'ne_enable')) return 'enabled'
  if (hasCompletedDetail(plan.steps, 'configured', 'ne_config')) return 'configured'
  if (hasAnyDetail(plan.steps, 'requested', 'ne_install')) return 'requested'

  if (plan.steps.some((step) => step.detail === 'rolled_back')) return 'rolled_back'
  if (
    plan.steps.some(
      (step) => step.detail === 'failed' && (step.state === 'blocked' || step.state === 'failed'),
    )
  ) {
    return 'failed'
  }

  return 'setup_needed'
}

function currentSetupStep(plan: SetupPlan): SetupStep | undefined {
  return (
    plan.steps.find((step) => step.id === plan.current_step_id) ??
    plan.steps.find((step) => step.state === 'current' || step.state === 'blocked' || step.state === 'failed')
  )
}

function hasCompletedDetail(
  steps: SetupStep[],
  detail: SetupStep['detail'],
  stepId: SetupStep['id'],
): boolean {
  return steps.some((step) => step.id === stepId && step.state === 'done' && step.detail === detail)
}

function hasAnyDetail(
  steps: SetupStep[],
  detail: SetupStep['detail'],
  stepId: SetupStep['id'],
): boolean {
  return steps.some((step) => step.id === stepId && step.detail === detail)
}
