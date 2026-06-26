import { test } from 'node:test'
import * as assert from 'node:assert/strict'

import { connectNavLabelKey, connectStatusMessageKey } from './status-summary.ts'
import type { ConnectView, SetupStep } from './types.ts'

function step(overrides: Partial<SetupStep> & Pick<SetupStep, 'id' | 'label' | 'state' | 'detail'>): SetupStep {
  return {
    reason_code: undefined,
    ...overrides,
  }
}

function view(overrides: Partial<ConnectView> = {}): ConnectView {
  return {
    state: 'needs_setup',
    message: 'needs_setup',
    proxy_url: null,
    pending_count: 0,
    counts: {
      grants: 0,
      redacted_today: 0,
      blocked_today: 0,
      apps_mediated: 0,
    },
    setup_plan: {
      current_step_id: 'ne_install',
      steps: [],
    },
    ...overrides,
  }
}

test('maps waiting-for-approval setup to approval-needed copy', () => {
  const connectView = view({
    setup_plan: {
      current_step_id: 'ne_install',
      steps: [
        step({
          id: 'ne_install',
          label: 'Approve network extension',
          state: 'blocked',
          detail: 'waiting_for_approval',
        }),
      ],
    },
  })

  assert.equal(connectStatusMessageKey(connectView), 'connect.summary.waiting_for_approval')
  assert.equal(connectNavLabelKey(connectView), 'nav.approvalNeeded')
})

test('maps configured-but-not-connected setup to configured copy', () => {
  const connectView = view({
    setup_plan: {
      current_step_id: 'ne_enable',
      steps: [
        step({ id: 'ne_install', label: 'Install network extension', state: 'done', detail: 'requested' }),
        step({ id: 'ne_config', label: 'Add network configuration', state: 'done', detail: 'configured' }),
        step({ id: 'ne_enable', label: 'Enable network extension', state: 'current', detail: 'needs_enable' }),
      ],
    },
  })

  assert.equal(connectStatusMessageKey(connectView), 'connect.summary.configured')
  assert.equal(connectNavLabelKey(connectView), 'nav.connecting')
})

test('maps connected-setup milestone before trust completion to connected copy', () => {
  const connectView = view({
    setup_plan: {
      current_step_id: 'ca_install',
      steps: [
        step({ id: 'ne_config', label: 'Add network configuration', state: 'done', detail: 'configured' }),
        step({ id: 'ne_enable', label: 'Enable network extension', state: 'done', detail: 'enabled' }),
        step({ id: 'ne_start', label: 'Enable protection layer', state: 'done', detail: 'connected' }),
        step({ id: 'ca_install', label: 'Install local CA', state: 'current', detail: 'needs_install' }),
      ],
    },
  })

  assert.equal(connectStatusMessageKey(connectView), 'connect.summary.connected')
  assert.equal(connectNavLabelKey(connectView), 'nav.connecting')
})

test('maps waiting-for-reboot setup to connecting copy', () => {
  const connectView = view({
    setup_plan: {
      current_step_id: 'ne_reboot',
      steps: [
        step({ id: 'ne_enable', label: 'Enable network extension', state: 'done', detail: 'enabled' }),
        step({ id: 'ne_reboot', label: 'Restart macOS', state: 'blocked', detail: 'waiting_for_reboot' }),
      ],
    },
  })

  assert.equal(connectStatusMessageKey(connectView), 'connect.summary.waiting_for_reboot')
  assert.equal(connectNavLabelKey(connectView), 'nav.connecting')
})

test('maps failed setup to repair-needed copy', () => {
  const connectView = view({
    state: 'degraded',
    message: 'degraded',
    setup_plan: {
      current_step_id: 'ne_start',
      steps: [
        step({
          id: 'ne_start',
          label: 'Enable protection layer',
          state: 'failed',
          detail: 'failed',
          reason_code: 'setup_step_failed',
        }),
      ],
    },
  })

  assert.equal(connectStatusMessageKey(connectView), 'connect.summary.failed')
  assert.equal(connectNavLabelKey(connectView), 'nav.repairNeeded')
})

test('maps generic needs-setup state without setup progress to setup-needed copy', () => {
  const connectView = view({
    setup_plan: {
      current_step_id: 'setup',
      steps: [],
    },
  })

  assert.equal(connectStatusMessageKey(connectView), 'connect.setupStatus')
  assert.equal(connectNavLabelKey(connectView), 'nav.setupNeeded')
})

test('maps disconnected state to the off brand-bar label', () => {
  const connectView = view({
    state: 'disconnected',
    message: 'disconnected',
    setup_plan: null,
  })

  assert.equal(connectStatusMessageKey(connectView), 'connect.disconnectedLede')
  assert.equal(connectNavLabelKey(connectView), 'nav.off')
})

test('maps rolled-back setup to repair-needed copy', () => {
  const connectView = view({
    state: 'degraded',
    message: 'degraded',
    setup_plan: {
      current_step_id: 'ne_enable',
      steps: [
        step({
          id: 'ne_enable',
          label: 'Repair network extension',
          state: 'blocked',
          detail: 'rolled_back',
        }),
      ],
    },
  })

  assert.equal(connectStatusMessageKey(connectView), 'connect.summary.rolled_back')
  assert.equal(connectNavLabelKey(connectView), 'nav.repairNeeded')
})

test('falls back to generic degraded attention copy when no setup plan is present', () => {
  const connectView = view({
    state: 'degraded',
    message: 'degraded',
    setup_plan: null,
  })

  assert.equal(connectStatusMessageKey(connectView), 'connect.degradedStatus')
  assert.equal(connectNavLabelKey(connectView), 'nav.attention')
})
