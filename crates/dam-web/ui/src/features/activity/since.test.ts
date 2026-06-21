import { test } from 'node:test'
import * as assert from 'node:assert/strict'

import { sinceTimestamp } from './since.ts'

test('today starts at UTC midnight', () => {
  const nowMs = Date.UTC(2026, 5, 21, 23, 45, 12)

  assert.equal(sinceTimestamp('today', nowMs), Date.UTC(2026, 5, 21, 0, 0, 0) / 1000)
})

test('rolling windows still anchor from the provided clock', () => {
  const nowMs = Date.UTC(2026, 5, 21, 23, 45, 12)

  assert.equal(sinceTimestamp('1h', nowMs), Math.floor(nowMs / 1000) - 3_600)
  assert.equal(sinceTimestamp('7d', nowMs), Math.floor(nowMs / 1000) - 7 * 86_400)
})
