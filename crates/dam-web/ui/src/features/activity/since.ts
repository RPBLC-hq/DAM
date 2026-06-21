export type Since = '1h' | 'today' | '7d' | '30d' | 'all'

export function sinceTimestamp(value: Since, nowMs = Date.now()): number | null {
  if (value === 'all') return 0
  const now = Math.floor(nowMs / 1000)
  if (value === '1h') return now - 3_600
  if (value === 'today') {
    const start = new Date(nowMs)
    start.setUTCHours(0, 0, 0, 0)
    return Math.floor(start.getTime() / 1000)
  }
  if (value === '30d') return now - 30 * 86_400
  return now - 7 * 86_400
}
