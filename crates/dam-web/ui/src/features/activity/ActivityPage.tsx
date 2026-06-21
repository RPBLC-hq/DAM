import { useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import {
  Button,
  EmptyTile,
  ErrorTile,
  ProtectionMark,
  type ProtectionState,
  RedactionLoader,
  SearchBar,
  SegmentedControl,
} from '@rpblc/design'

import { ApiError, api, apiPost } from '@/lib/api/client'
import { useI18n, type MessageKey } from '@/lib/i18n'
import { useUrlSearchParam, useUrlSearchString } from '@/lib/url-search'
import { sinceTimestamp, type Since } from './since'
import type { ActivityDecision, ActivityEvent, ActivityView } from './types'
import type { WalletDetail, WalletKind } from '@/features/wallet/types'

type Decision = 'all' | ActivityDecision

const DECISION_VALUES: Decision[] = ['all', 'granted', 'sealed', 'denied']
const SINCE_VALUES: Since[] = ['1h', 'today', '7d', '30d', 'all']

const QUERY_KEY = 'activity' as const
const ACTIVITY_REFETCH_INTERVAL_MS = 5_000

type AddActivityValueRequest = {
  eventId: number
  kind: WalletKind
  value: string
}

export function ActivityPage() {
  const { t, locale } = useI18n()
  const queryClient = useQueryClient()
  const [addedActivityIds, setAddedActivityIds] = useState<ReadonlySet<number>>(new Set())
  const formatter = useMemo(
    () => new Intl.RelativeTimeFormat(locale, { numeric: 'auto' }),
    [locale],
  )

  // Filters: q + decision + since. URL-stable so refresh and share
  // preserve state. Tray surface uses memory history, but the same filter
  // state applies so Activity never opens as an unbounded log dump.
  const [query, setQuery] = useUrlSearchString('q')
  const [decision, setDecision] = useUrlSearchParam<Decision>(
    'decision',
    'all',
    isDecision,
  )
  const [since, setSince] = useUrlSearchParam<Since>('since', '1h', isSince)

  const activity = useQuery({
    queryKey: [QUERY_KEY, { query, decision, since }] as const,
    queryFn: ({ signal }) => {
      const params = new URLSearchParams()
      if (query) params.set('q', query)
      if (decision !== 'all') params.set('decision', decision)
      const sinceSeconds = sinceTimestamp(since)
      if (sinceSeconds !== null) params.set('since', String(sinceSeconds))
      const search = params.toString()
      return api<ActivityView>(
        `/activity${search ? `?${search}` : ''}`,
        { signal },
      )
    },
    refetchInterval: () =>
      typeof document !== 'undefined' && document.visibilityState === 'hidden'
        ? false
        : ACTIVITY_REFETCH_INTERVAL_MS,
    refetchOnWindowFocus: true,
    staleTime: 1_000,
  })

  const addToWallet = useMutation({
    mutationFn: ({ kind, value }: AddActivityValueRequest) =>
      apiPost<WalletDetail>('/wallet', { kind, value }),
    onSuccess: (_detail, variables) => {
      setAddedActivityIds((current) => {
        const next = new Set(current)
        next.add(variables.eventId)
        return next
      })
      void queryClient.invalidateQueries({ queryKey: ['wallet'] })
      void queryClient.invalidateQueries({ queryKey: ['connect'] })
    },
  })

  const decisionOptions = DECISION_VALUES.map((value) => ({
    value,
    label: t(decisionLabelKey(value)),
  }))
  const sinceOptions = SINCE_VALUES.map((value) => ({
    value,
    label: t(sinceLabelKey(value)),
  }))

  const errorCode =
    activity.error instanceof ApiError ? activity.error.message : undefined

  return (
    <section className="dam-activity" aria-label={t('activity.aria')}>
      <header className="dam-activity__header">
        <h1 className="dam-activity__heading">{t('activity.heading')}</h1>
        <p className="dam-activity__hint">{t('activity.hint')}</p>
        <div className="dam-activity__filters">
          <SearchBar
            value={query}
            onValueChange={setQuery}
            aria-label={t('activity.searchAria')}
            placeholder={t('activity.searchPlaceholder')}
          />
          <SegmentedControl<Decision>
            value={decision}
            onValueChange={setDecision}
            options={decisionOptions}
            aria-label={t('activity.decisionAria')}
          />
          <SegmentedControl<Since>
            value={since}
            onValueChange={setSince}
            options={sinceOptions}
            aria-label={t('activity.sinceAria')}
          />
        </div>
      </header>

      <div className="dam-activity__list">
        {activity.isPending ? (
          <LoadingState />
        ) : activity.isError ? (
          <ErrorTile
            message={t(errorMessageKey(errorCode))}
            action={
              <Button
                variant="ghost"
                size="sm"
                type="button"
                onClick={() => void activity.refetch()}
              >
                {t('activity.tryAgain')}
              </Button>
            }
          />
        ) : (activity.data?.events.length ?? 0) === 0 ? (
          <EmptyTile message={t('activity.empty')} />
        ) : (
          activity.data!.events.map((item) => (
            <ActivityRow
              key={item.id}
              item={item}
              relative={(seconds) => relativePast(formatter, seconds)}
              addBusy={addToWallet.isPending}
              addPending={addToWallet.isPending && addToWallet.variables?.eventId === item.id}
              addSucceeded={addedActivityIds.has(item.id)}
              addFailed={addToWallet.isError && addToWallet.variables?.eventId === item.id}
              onAddToWallet={(kind, value) => addToWallet.mutate({ eventId: item.id, kind, value })}
            />
          ))
        )}
      </div>
    </section>
  )
}

function ActivityRow({
  item,
  relative,
  addBusy,
  addPending,
  addSucceeded,
  addFailed,
  onAddToWallet,
}: {
  item: ActivityEvent
  relative: (secondsAgo: number) => string
  addBusy: boolean
  addPending: boolean
  addSucceeded: boolean
  addFailed: boolean
  onAddToWallet: (kind: WalletKind, value: string) => void
}) {
  const { t } = useI18n()
  const ago = relative(Math.max(0, Math.floor(Date.now() / 1000) - item.ts))
  const decision = t(activityDecisionLabelKey(item.decision))
  const detected = activityDetectedLabel(item, t('activity.valueUnavailable'))
  const identifier = activityIdentifierLabel(item)
  const walletKind = walletKindForActivityKind(item.kind)
  const walletValue = item.value?.trim() ?? ''
  const showWalletAction = walletKind !== null
  const addDisabled = addBusy || addPending || addSucceeded || !walletValue

  return (
    <article className="dam-activity__row">
      <header className="dam-activity__row-header">
        <div className="dam-activity__lead">
          <span className="dam-activity__time">{ago}</span>
          <span className="dam-activity__value">{detected}</span>
        </div>
        <ProtectionMark
          state={protectionStateForDecision(item.decision)}
          label={decision}
          className="dam-activity__outcome"
        />
      </header>
      <div className="dam-activity__facts" aria-label={t('activity.factsAria')}>
        <ActivityIdentifier value={identifier} />
        <ActivityFact label={t('activity.profile')} value={item.profile} />
      </div>
      {showWalletAction && (
        <div className="dam-activity__actions">
          {addFailed && (
            <p className="dam-activity__action-error">{t('activity.error.addFailed')}</p>
          )}
          <Button
            variant="secondary"
            size="sm"
            bracketed
            type="button"
            disabled={addDisabled}
            title={!walletValue ? t('activity.valueUnavailable') : undefined}
            onClick={() => {
              if (!walletKind || !walletValue) return
              onAddToWallet(walletKind, walletValue)
            }}
          >
            {addPending
              ? t('activity.adding')
              : addSucceeded
                ? t('activity.added')
                : t('activity.add')}
          </Button>
        </div>
      )}
    </article>
  )
}

function activityDetectedLabel(item: ActivityEvent, unavailable: string): string {
  if (item.value) return item.value
  if (item.kind !== 'unknown') return `[${item.kind}]`
  return unavailable
}

function activityIdentifierLabel(item: ActivityEvent): string {
  if (item.reference) return item.reference
  if (item.kind !== 'unknown') return item.kind
  return item.audit_id
}

function ActivityIdentifier({ value }: { value: string }) {
  return (
    <span className="dam-activity__fact dam-activity__identifier">
      [<b>{value}</b>]
    </span>
  )
}

function ActivityFact({ label, value }: { label: string; value: string }) {
  return (
    <span className="dam-activity__fact">
      [{label}: <b>{value}</b>]
    </span>
  )
}

function protectionStateForDecision(decision: ActivityDecision): ProtectionState {
  if (decision === 'granted') return 'allowed'
  if (decision === 'denied') return 'revoked'
  return 'protected'
}

function walletKindForActivityKind(kind: string): WalletKind | null {
  if (
    kind === 'email' ||
    kind === 'domain' ||
    kind === 'phone' ||
    kind === 'ssn' ||
    kind === 'cc'
  ) {
    return kind
  }
  if (kind === 'credit_card' || kind === 'credit-card') return 'cc'
  return null
}

function LoadingState() {
  const { t } = useI18n()
  return (
    <div className="dam-activity__loading">
      <RedactionLoader
        redacted
        bars={4}
        width="11em"
        reason={t('activity.loadingReason')}
        aria-label={t('activity.loadingReason')}
        verbose
      />
    </div>
  )
}

function relativePast(formatter: Intl.RelativeTimeFormat, secondsAgo: number): string {
  if (secondsAgo < 60) return formatter.format(-secondsAgo, 'second')
  if (secondsAgo < 3_600) return formatter.format(-Math.floor(secondsAgo / 60), 'minute')
  if (secondsAgo < 86_400) return formatter.format(-Math.floor(secondsAgo / 3_600), 'hour')
  return formatter.format(-Math.floor(secondsAgo / 86_400), 'day')
}

function errorMessageKey(code: string | undefined): MessageKey {
  if (code === 'daemon_unreachable') return 'wallet.error.daemon'
  return 'activity.error.unknown'
}

function isDecision(value: string): value is Decision {
  return (DECISION_VALUES as readonly string[]).includes(value)
}

function isSince(value: string): value is Since {
  return (SINCE_VALUES as readonly string[]).includes(value)
}

function decisionLabelKey(value: Decision): MessageKey {
  if (value === 'granted') return 'activity.decision.granted'
  if (value === 'sealed') return 'activity.decision.sealed'
  if (value === 'denied') return 'activity.decision.denied'
  return 'activity.decision.all'
}

function activityDecisionLabelKey(value: ActivityDecision): MessageKey {
  if (value === 'granted') return 'activity.decision.granted'
  if (value === 'sealed') return 'activity.decision.sealed'
  return 'activity.decision.denied'
}

function sinceLabelKey(value: Since): MessageKey {
  if (value === '1h') return 'activity.since.1h'
  if (value === 'today') return 'activity.since.today'
  if (value === '7d') return 'activity.since.7d'
  if (value === '30d') return 'activity.since.30d'
  return 'activity.since.all'
}
