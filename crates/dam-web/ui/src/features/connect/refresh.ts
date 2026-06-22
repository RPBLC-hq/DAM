export const CONNECT_STATS_REFETCH_INTERVAL_MS = 5_000

export function connectRefetchInterval(): number | false {
  return typeof document !== 'undefined' && document.visibilityState === 'hidden'
    ? false
    : CONNECT_STATS_REFETCH_INTERVAL_MS
}
