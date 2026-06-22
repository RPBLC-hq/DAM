import { test } from 'node:test'
import * as assert from 'node:assert/strict'

import { activityDetectedLabel, activityIdentifierLabel } from './display.ts'
import type { ActivityEvent } from './types.ts'

function event(overrides: Partial<ActivityEvent> = {}): ActivityEvent {
  return {
    id: 7,
    ts: 1_718_000_000,
    day: '2026-06-22',
    profile: 'Claude',
    kind: 'email',
    value: 'alice@example.com',
    reference: '[email:abc123]',
    decision: 'sealed',
    purpose: undefined,
    audit_id: 'evt_0000000000000007',
    ...overrides,
  }
}

test('activity rows prefer the safe kind label over a raw stored value', () => {
  assert.equal(activityDetectedLabel(event(), 'value unavailable'), '[email]')
})

test('activity rows fall back to the reference when the kind is unknown', () => {
  assert.equal(
    activityDetectedLabel(
      event({
        kind: 'unknown',
        value: 'super-secret-value',
        reference: '[token:abc123]',
      }),
      'value unavailable',
    ),
    '[token:abc123]',
  )
})

test('activity rows do not surface a raw unknown value without a reference', () => {
  assert.equal(
    activityDetectedLabel(
      event({ kind: 'unknown', value: 'super-secret-value', reference: undefined }),
      'value unavailable',
    ),
    'value unavailable',
  )
})

test('activity identifier keeps the token reference when present', () => {
  assert.equal(activityIdentifierLabel(event()), '[email:abc123]')
})