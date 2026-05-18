export type ActivityDecision = 'granted' | 'sealed' | 'denied'

export type ActivityView = {
  events: ActivityEvent[]
  summary: {
    total: number
    granted: number
    sealed: number
    denied: number
  }
}

export type ActivityEvent = {
  id: number
  ts: number
  day: string
  profile: string
  kind: string
  value?: string
  wallet_id?: string
  decision: ActivityDecision
  purpose?: string
  audit_id: string
}
