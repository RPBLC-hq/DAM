import test from 'node:test'
import assert from 'node:assert/strict'

import { normalizeDetectorSettings } from './detectors.ts'

test('normalizeDetectorSettings keeps supported detector order and drops unknown keys', () => {
  const normalized = normalizeDetectorSettings([
    { key: 'api_key', label: '', enabled: false },
    { key: 'unknown', label: 'Unknown', enabled: true },
    { key: 'email', label: '', enabled: true },
    { key: 'phone', label: 'Phone', enabled: false },
  ])

  assert.deepEqual(normalized, [
    { key: 'email', label: 'Email', enabled: true },
    { key: 'phone', label: 'Phone', enabled: false },
    { key: 'api_key', label: 'API key', enabled: false },
  ])
})
