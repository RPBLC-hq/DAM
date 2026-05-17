import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import {
  Button,
  Dropdown,
  EmptyTile,
  ErrorTile,
  Input,
  RedactionLoader,
  SearchBar,
  SegmentedControl,
  WalletCard,
  type WalletCardState,
} from '@rpblc/design'

import { ApiError, api, apiPost } from '@/lib/api/client'
import { useI18n, type MessageKey } from '@/lib/i18n'
import { resolveSurface } from '@/lib/surface'
import { useUrlSearchParam, useUrlSearchString } from '@/lib/url-search'
import type { WalletDetail, WalletItem, WalletList } from './types'
import { WalletInlineDetail } from './WalletInlineDetail'

// Animation duration for the inline-detail expand/collapse. Must be
// >= the CSS transition on `.dam-wallet__inline-wrap` (420ms) so the
// previously-active row's content stays mounted through the entire
// close transition. The 40ms buffer absorbs any frame jitter and
// prevents the content from unmounting just before the visual end.
const INLINE_DETAIL_ANIM_MS = 460
type WalletKind = 'email' | 'domain' | 'phone' | 'ssn' | 'cc'
type WalletFilter = 'all' | 'protected' | 'allowed'

export function WalletListPage() {
  const { t, locale } = useI18n()
  const queryClient = useQueryClient()
  const surface = resolveSurface()
  // The wallet's `?q=` search param is URL-stable so refresh, share,
  // and the Insights kind-leaderboard's deep link all preserve filter
  // state. Tray uses a memory router; the helper degrades to a noop on
  // memory history because there is no real URL to update.
  const [query, setQuery] = useUrlSearchString('q')
  const [stateFilter, setStateFilter] = useUrlSearchParam(
    'state',
    'all',
    isWalletFilter,
  )
  const [activeId, setActiveId] = useState<string | null>(null)
  const [adding, setAdding] = useState(false)
  // The CSS class `--active` is gated behind a paint frame after
  // `activeId` flips. Without that gate, React commits the new content
  // and the `--active` class in a single paint, the browser starts at
  // the destination grid track size, and the transition catches up
  // backwards — the panel "jumps" 5–10% open before animating. Painting
  // first at `0fr`-with-content (no `--active`) gives the CSS transition
  // a real "from" state to interpolate from.
  const [openId, setOpenId] = useState<string | null>(null)
  // The previously active row keeps its content mounted while the close
  // transition runs. Cleared after the transition completes.
  const [closingId, setClosingId] = useState<string | null>(null)
  const inputRef = useRef<HTMLInputElement>(null)
  const listRef = useRef<HTMLDivElement>(null)
  // When a click switches the active row, we capture the clicked row's
  // current screen Y, then in a layout effect adjust the scroll
  // container so its post-render Y matches — so the focused row stays
  // at the same height even as siblings reflow around it.
  const pendingViewportSync = useRef<{
    targetId: string
    preY: number
  } | null>(null)
  const closeTimerRef = useRef<number | null>(null)
  const pendingAddedScrollId = useRef<string | null>(null)

  useEffect(() => {
    if (surface === 'web') inputRef.current?.focus()
  }, [surface])

  const wallet = useQuery({
    queryKey: ['wallet', { q: query, state: stateFilter }],
    queryFn: ({ signal }) =>
      api<WalletList>(walletPath(query, stateFilter), { signal }),
  })

  const addValue = useMutation({
    mutationFn: (body: { kind: WalletKind; value: string }) =>
      apiPost<WalletDetail>('/wallet', body),
    onSuccess: (detail) => {
      pendingAddedScrollId.current = detail.item.id
      setAdding(false)
      setQuery('')
      setStateFilter('all')
      setActiveId(detail.item.id)
      void queryClient.invalidateQueries({ queryKey: ['wallet'] })
      void queryClient.invalidateQueries({ queryKey: ['connect'] })
    },
  })

  const items = wallet.data?.items ?? []
  const total = wallet.data?.total ?? 0
  const formatter = useMemo(() => new Intl.NumberFormat(locale), [locale])
  const errorCode =
    wallet.error instanceof ApiError ? wallet.error.message : undefined

  // Collapse the active row if it's no longer in the filtered set.
  useEffect(() => {
    if (activeId && !items.some((i) => i.id === activeId)) {
      setActiveId(null)
      setClosingId(null)
      setOpenId(null)
    }
  }, [items, activeId])

  useEffect(() => {
    return () => {
      if (closeTimerRef.current != null) {
        window.clearTimeout(closeTimerRef.current)
      }
    }
  }, [])

  // Sync the visual `openId` to `activeId` after one paint frame. The
  // pre-frame paint shows the new row's wrapper at `grid-template-rows:
  // 0fr` with content already mounted, so the post-frame paint at `1fr`
  // can transition smoothly from a real starting state.
  useLayoutEffect(() => {
    if (activeId === openId) return
    const handle = window.requestAnimationFrame(() => {
      setOpenId(activeId)
    })
    return () => window.cancelAnimationFrame(handle)
  }, [activeId, openId])

  const onToggle = useCallback(
    (id: string) => {
      if (id === activeId) {
        // Same row clicked: close it. The detail content stays mounted
        // briefly so the close animation plays.
        setClosingId(id)
        setActiveId(null)
        if (closeTimerRef.current != null)
          window.clearTimeout(closeTimerRef.current)
        closeTimerRef.current = window.setTimeout(() => {
          setClosingId((prev) => (prev === id ? null : prev))
        }, INLINE_DETAIL_ANIM_MS)
        return
      }
      // Switching rows (or opening a fresh one): record the clicked
      // row's current screen position so we can keep the viewport
      // stable as the previous detail collapses and the new one opens.
      const rowEl = listRef.current?.querySelector<HTMLElement>(
        `[data-row-id="${cssEscape(id)}"]`,
      )
      if (rowEl) {
        pendingViewportSync.current = {
          targetId: id,
          preY: rowEl.getBoundingClientRect().top,
        }
      }
      // The currently-active row (if any) needs to keep rendering its
      // detail through the close transition.
      setClosingId(activeId)
      setActiveId(id)
      if (closeTimerRef.current != null)
        window.clearTimeout(closeTimerRef.current)
      closeTimerRef.current = window.setTimeout(() => {
        setClosingId((prev) => (prev === activeId ? null : prev))
      }, INLINE_DETAIL_ANIM_MS)
    },
    [activeId],
  )

  useLayoutEffect(() => {
    const sync = pendingViewportSync.current
    if (!sync) return
    pendingViewportSync.current = null
    const list = listRef.current
    if (!list) return
    const target = list.querySelector<HTMLElement>(
      `[data-row-id="${cssEscape(sync.targetId)}"]`,
    )
    if (!target) return
    const newY = target.getBoundingClientRect().top
    const delta = newY - sync.preY
    if (delta === 0) return
    // Walk up to the nearest scrollable ancestor (the tray content
    // area, or the body on web). Adjust its scrollTop by the same
    // delta so the clicked row visually stays at the same Y.
    const scroller = scrollableAncestor(list)
    if (scroller) {
      scroller.scrollTop += delta
    }
  }, [activeId])

  useLayoutEffect(() => {
    const id = pendingAddedScrollId.current
    if (!id) return
    const list = listRef.current
    if (!list) return
    const target = list.querySelector<HTMLElement>(
      `[data-row-id="${cssEscape(id)}"]`,
    )
    if (!target) return
    pendingAddedScrollId.current = null
    const handle = window.requestAnimationFrame(() => {
      target.scrollIntoView({
        block: 'center',
        behavior: prefersReducedMotion() ? 'auto' : 'smooth',
      })
    })
    return () => window.cancelAnimationFrame(handle)
  }, [items, openId])

  return (
    <section className="dam-wallet" aria-label={t('wallet.aria')}>
      <header className="dam-wallet__header">
        <h1 className="dam-wallet__heading">{t('wallet.heading')}</h1>
        <div className="dam-wallet__controls">
          <SearchBar
            ref={inputRef}
            value={query}
            onValueChange={setQuery}
            placeholder={t('wallet.searchPlaceholder')}
            aria-label={t('wallet.searchAria')}
            count={
              wallet.isSuccess
                ? `${formatter.format(items.length)}/${formatter.format(total)}`
                : undefined
            }
          />
          <Button
            variant={adding ? 'ghost' : 'secondary'}
            size="sm"
            bracketed
            type="button"
            onClick={() => setAdding((open) => !open)}
          >
            {t('wallet.addValue')}
          </Button>
        </div>
        <SegmentedControl
          value={stateFilter}
          onValueChange={setStateFilter}
          options={walletFilterOptions(t)}
          aria-label={t('wallet.filterAria')}
        />
      </header>

      {adding && (
        <AddValueForm
          pending={addValue.isPending}
          errorCode={addValue.error instanceof ApiError ? addValue.error.message : undefined}
          onCancel={() => {
            addValue.reset()
            setAdding(false)
          }}
          onSubmit={(kind, value) => addValue.mutate({ kind, value })}
        />
      )}

      <div className="dam-wallet__list" ref={listRef}>
        {wallet.isPending ? (
          <LoadingState />
        ) : wallet.isError ? (
          <ErrorTile
            message={t(errorMessageKey(errorCode))}
            action={
              <Button
                variant="ghost"
                size="sm"
                type="button"
                onClick={() => void wallet.refetch()}
              >
                {t('wallet.tryAgain')}
              </Button>
            }
          />
        ) : items.length === 0 ? (
          query ? (
            <EmptyTile
              message={`${t('wallet.empty.searchPrefix')} "${query}"`}
              action={
                <Button
                  variant="ghost"
                  size="sm"
                  type="button"
                  onClick={() => setQuery('')}
                >
                  {t('wallet.clearSearch')}
                </Button>
              }
            />
          ) : (
            <EmptyTile
              message={t('wallet.empty.first')}
              action={
                <Button
                  variant="ghost"
                  size="sm"
                  type="button"
                  onClick={() => setAdding(true)}
                >
                  {t('wallet.addValue')}
                </Button>
              }
            />
          )
        ) : (
          items.map((item) => {
            const isActive = item.id === activeId
            const isOpen = item.id === openId
            const isClosing = item.id === closingId
            // Mount content for the active row AND any row that's
            // mid-close (so the close animation can play). Visual
            // `--active` class is driven by `isOpen` (gated by rAF) so
            // the transition has a real "from" frame.
            const showDetail = isActive || isClosing
            return (
              <div
                key={item.id}
                data-row-id={item.id}
                className={`dam-wallet__row${isOpen ? ' dam-wallet__row--active' : ''}`}
              >
                <WalletRow
                  item={item}
                  active={isOpen}
                  onToggle={() => onToggle(item.id)}
                />
                <div className="dam-wallet__inline-wrap" aria-hidden={!isOpen}>
                  {showDetail && <WalletInlineDetail id={item.id} seed={item} />}
                </div>
              </div>
            )
          })
        )}
      </div>
    </section>
  )
}

function AddValueForm({
  pending,
  errorCode,
  onCancel,
  onSubmit,
}: {
  pending: boolean
  errorCode?: string
  onCancel: () => void
  onSubmit: (kind: WalletKind, value: string) => void
}) {
  const { t } = useI18n()
  const [kind, setKind] = useState<WalletKind>('email')
  const [value, setValue] = useState('')
  return (
    <form
      className="dam-wallet__add"
      onSubmit={(event) => {
        event.preventDefault()
        const trimmed = value.trim()
        if (!trimmed || pending) return
        onSubmit(kind, trimmed)
      }}
    >
      <Dropdown
        size="sm"
        label={t('wallet.addKind')}
        value={kind}
        onValueChange={(next) => setKind(next as WalletKind)}
        items={walletKindOptions(t)}
      />
      <Input
        label={t('wallet.addValueLabel')}
        value={value}
        onChange={(event) => setValue(event.currentTarget.value)}
        placeholder={t('wallet.addValuePlaceholder')}
        disabled={pending}
      />
      <div className="dam-wallet__add-actions">
        <Button variant="ghost" size="sm" type="button" disabled={pending} onClick={onCancel}>
          {t('wallet.addCancel')}
        </Button>
        <Button variant="primary" size="sm" bracketed type="submit" disabled={pending || !value.trim()}>
          {t('wallet.addSubmit')}
        </Button>
      </div>
      {errorCode && <p className="dam-wallet__add-error">{t(walletMutationErrorKey(errorCode))}</p>}
    </form>
  )
}

function WalletRow({
  item,
  active,
  onToggle,
}: {
  item: WalletItem
  active: boolean
  onToggle: () => void
}) {
  const { t } = useI18n()
  return (
    <WalletCard
      kind={item.kind}
      value={item.value}
      state={item.state as WalletCardState}
      active={active}
      onClick={onToggle}
      meta={renderMeta(item, t)}
    />
  )
}

function LoadingState() {
  const { t } = useI18n()
  return (
    <div className="dam-wallet__loading">
      <RedactionLoader
        redacted
        bars={4}
        width="11em"
        reason={t('wallet.loadingReason')}
        aria-label={t('wallet.loadingReason')}
        verbose
      />
    </div>
  )
}

function renderMeta(item: WalletItem, t: (key: MessageKey) => string) {
  if (item.state === 'allowed' && item.shared_with[0]) {
    const main = item.shared_with[0]
    const extra = item.shared_with.length - 1
    return (
      <>
        {t('wallet.meta.sharedWith')} <b>{main.name}</b>
        {extra > 0 ? ` +${extra}` : null}
      </>
    )
  }
  if (item.state === 'revoked' || item.state === 'expired') {
    const previous = item.shared_with[0]?.name
    if (previous) {
      return (
        <>
          {t('wallet.meta.revokedFrom')} <b>{previous}</b>
        </>
      )
    }
    return <>{t('wallet.meta.notShared')}</>
  }
  if (item.last_seen) {
    return (
      <>
        {t('wallet.meta.lastSeen')} <b>{item.last_seen}</b>
      </>
    )
  }
  return null
}

function walletPath(query: string, state: WalletFilter): string {
  const params = new URLSearchParams()
  const trimmed = query.trim()
  if (trimmed) params.set('q', trimmed)
  if (state !== 'all') params.set('state', state)
  const search = params.toString()
  return search ? `/wallet?${search}` : '/wallet'
}

function errorMessageKey(code: string | undefined): MessageKey {
  if (code === 'wallet_unreachable') return 'wallet.error.unreachable'
  if (code === 'daemon_unreachable') return 'wallet.error.daemon'
  return 'wallet.error.unknown'
}

function walletMutationErrorKey(code: string | undefined): MessageKey {
  if (code === 'invalid_request') return 'wallet.error.invalidRequest'
  if (code === 'wallet_unreachable') return 'wallet.error.unreachable'
  return 'wallet.error.unknown'
}

function walletKindOptions(t: (key: MessageKey) => string) {
  return [
    { value: 'email', label: t('wallet.kind.email') },
    { value: 'domain', label: t('wallet.kind.domain') },
    { value: 'phone', label: t('wallet.kind.phone') },
    { value: 'ssn', label: t('wallet.kind.ssn') },
    { value: 'cc', label: t('wallet.kind.cc') },
  ] satisfies Array<{ value: WalletKind; label: string }>
}

function walletFilterOptions(t: (key: MessageKey) => string) {
  return [
    { value: 'all', label: t('wallet.filter.all') },
    { value: 'protected', label: t('wallet.filter.protected') },
    { value: 'allowed', label: t('wallet.filter.allowed') },
  ] satisfies Array<{ value: WalletFilter; label: string }>
}

function isWalletFilter(value: string): value is WalletFilter {
  return value === 'all' || value === 'protected' || value === 'allowed'
}

function cssEscape(value: string): string {
  if (typeof CSS !== 'undefined' && typeof CSS.escape === 'function') {
    return CSS.escape(value)
  }
  return value.replace(/["\\]/g, '\\$&')
}

function scrollableAncestor(el: HTMLElement): HTMLElement | null {
  let cur: HTMLElement | null = el.parentElement
  while (cur) {
    const cs = getComputedStyle(cur)
    const overflowY = cs.overflowY
    if (
      (overflowY === 'auto' || overflowY === 'scroll') &&
      cur.scrollHeight > cur.clientHeight
    ) {
      return cur
    }
    cur = cur.parentElement
  }
  // Fall back to documentElement so we still adjust on web.
  return document.scrollingElement as HTMLElement | null
}

function prefersReducedMotion(): boolean {
  return (
    typeof window !== 'undefined' &&
    window.matchMedia('(prefers-reduced-motion: reduce)').matches
  )
}
