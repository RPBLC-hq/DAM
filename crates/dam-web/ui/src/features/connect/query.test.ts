import { test } from 'node:test'
import * as assert from 'node:assert/strict'

import { connectRefetchInterval, CONNECT_STATS_REFETCH_INTERVAL_MS } from './refresh.ts'

test('connect polling stays on while the document is visible', () => {
  const previousDocument = globalThis.document
  Object.defineProperty(globalThis, 'document', {
    configurable: true,
    value: { visibilityState: 'visible' },
  })

  try {
    assert.equal(connectRefetchInterval(), CONNECT_STATS_REFETCH_INTERVAL_MS)
  } finally {
    restoreDocument(previousDocument)
  }
})

test('connect polling pauses while the document is hidden', () => {
  const previousDocument = globalThis.document
  Object.defineProperty(globalThis, 'document', {
    configurable: true,
    value: { visibilityState: 'hidden' },
  })

  try {
    assert.equal(connectRefetchInterval(), false)
  } finally {
    restoreDocument(previousDocument)
  }
})

test('connect polling still works in non-browser test contexts', () => {
  const previousDocument = globalThis.document
  Object.defineProperty(globalThis, 'document', {
    configurable: true,
    value: undefined,
  })

  try {
    assert.equal(connectRefetchInterval(), CONNECT_STATS_REFETCH_INTERVAL_MS)
  } finally {
    restoreDocument(previousDocument)
  }
})

function restoreDocument(previousDocument: typeof globalThis.document) {
  Object.defineProperty(globalThis, 'document', {
    configurable: true,
    value: previousDocument,
  })
}
