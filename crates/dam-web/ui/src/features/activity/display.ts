import type { ActivityEvent } from './types'

export function activityDetectedLabel(item: ActivityEvent, unavailable: string): string {
  if (item.kind !== 'unknown') return `[${item.kind}]`
  if (item.reference) return item.reference
  return unavailable
}

export function activityIdentifierLabel(item: ActivityEvent): string {
  if (item.reference) return item.reference
  if (item.kind !== 'unknown') return item.kind
  return item.audit_id
}