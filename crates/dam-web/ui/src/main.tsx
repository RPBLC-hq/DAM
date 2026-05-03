import { type KeyboardEvent, useEffect, useRef, useState } from 'react'
import { createRoot } from 'react-dom/client'

type ShellProps = {
  title: string
  active: string
  meta: string
  count: number
  countLabel: string
  contentClass: string
  contentHtml: string
  brandUrl: string
  isTray: boolean
}

type NavItem = {
  label: string
  href: string
  active: string
}

type ThemePreference = 'system' | 'light' | 'dark'

const themeStorageKey = 'rpblc.dam.theme'

const primaryNav: NavItem[] = [
  { label: 'Connect', href: '/connect', active: 'Connect' },
  { label: 'Wallet', href: '/vault', active: 'Vault' },
  { label: 'Allowed', href: '/allowed', active: 'Allowed' },
]

const moreNav: NavItem[] = [
  { label: 'Settings', href: '/settings', active: 'Settings' },
  { label: 'Insights', href: '/logs', active: 'Logs' },
  { label: 'Doctor', href: '/doctor', active: 'Doctor' },
  { label: 'Diagnostics', href: '/diagnostics', active: 'Diagnostics' },
]

const themeOptions: Array<{ label: string; value: ThemePreference }> = [
  { label: 'System', value: 'system' },
  { label: 'Light', value: 'light' },
  { label: 'Dark', value: 'dark' },
]

function readShellProps(): ShellProps | null {
  const node = document.getElementById('dam-web-props')
  if (!node?.textContent) {
    return null
  }

  try {
    return JSON.parse(node.textContent) as ShellProps
  } catch (error) {
    console.error('Failed to read DAM web props', error)
    return null
  }
}

function BrandMark() {
  return (
    <span className="brand-mark" aria-hidden="true">
      <span className="glyph bracket">[</span>
      <span className="glyph letter">R</span>
      <span className="glyph colon">:</span>
      <span className="glyph bracket">]</span>
    </span>
  )
}

function NavLink({ item, active }: { item: NavItem; active: string }) {
  return (
    <a className={active === item.active ? 'active' : ''} href={item.href}>
      {item.label}
    </a>
  )
}

function NavMenuLink({ item, active }: { item: NavItem; active: string }) {
  const selected = active === item.active

  return (
    <a
      className={`rpblc-dropdown__item${selected ? ' active rpblc-dropdown__item--selected' : ''}`}
      href={item.href}
      aria-current={selected ? 'page' : undefined}
    >
      <span className="rpblc-dropdown__item-body">
        <span className="rpblc-dropdown__item-label">{item.label}</span>
      </span>
      {selected && (
        <span className="rpblc-dropdown__item-mark" aria-hidden="true">
          :
        </span>
      )}
    </a>
  )
}

function isThemePreference(value: string | null): value is ThemePreference {
  return value === 'system' || value === 'light' || value === 'dark'
}

function readThemePreference(): ThemePreference {
  try {
    const stored = window.localStorage.getItem(themeStorageKey)
    return isThemePreference(stored) ? stored : 'system'
  } catch {
    return 'system'
  }
}

function applyThemePreference(preference: ThemePreference) {
  if (preference === 'system') {
    delete document.documentElement.dataset.theme
    return
  }

  document.documentElement.dataset.theme = preference
}

function persistThemePreference(preference: ThemePreference) {
  try {
    window.localStorage.setItem(themeStorageKey, preference)
  } catch {
    // The setting remains active for this page when storage is unavailable.
  }
}

function ThemeSettings({
  preference,
  onChange,
}: {
  preference: ThemePreference
  onChange: (preference: ThemePreference) => void
}) {
  const optionRefs = useRef<Array<HTMLButtonElement | null>>([])
  const selectedIndex = Math.max(
    0,
    themeOptions.findIndex((option) => option.value === preference),
  )

  function selectAt(index: number) {
    const next = themeOptions[index]
    if (!next) {
      return
    }
    onChange(next.value)
    optionRefs.current[index]?.focus()
  }

  function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (event.key === 'ArrowRight' || event.key === 'ArrowDown') {
      event.preventDefault()
      selectAt((selectedIndex + 1) % themeOptions.length)
    } else if (event.key === 'ArrowLeft' || event.key === 'ArrowUp') {
      event.preventDefault()
      selectAt((selectedIndex - 1 + themeOptions.length) % themeOptions.length)
    } else if (event.key === 'Home') {
      event.preventDefault()
      selectAt(0)
    } else if (event.key === 'End') {
      event.preventDefault()
      selectAt(themeOptions.length - 1)
    }
  }

  return (
    <section className="rpblc-section rpblc-section--compact settings-section theme-settings">
      <header className="rpblc-section__header">
        <h2 className="rpblc-section__title">Theme</h2>
      </header>
      <div className="rpblc-section__body rpblc-settings-section__body">
        <div
          className="rpblc-segmented rpblc-segmented--sm settings-theme-control"
          role="radiogroup"
          aria-label="Theme"
          onKeyDown={onKeyDown}
        >
          {themeOptions.map((option, index) => (
            <button
              type="button"
              role="radio"
              aria-checked={preference === option.value}
              tabIndex={preference === option.value ? 0 : -1}
              className={`rpblc-segmented__option${
                preference === option.value ? ' rpblc-segmented__option--selected' : ''
              }`}
              key={option.value}
              ref={(node) => {
                optionRefs.current[index] = node
              }}
              onClick={() => onChange(option.value)}
            >
              <span className="rpblc-segmented__label">{option.label}</span>
            </button>
          ))}
        </div>
      </div>
    </section>
  )
}

function bindAppCardDisclosures(root: ParentNode) {
  const cleanups: Array<() => void> = []
  const buttons = root.querySelectorAll<HTMLButtonElement>('.rpblc-app-card__disclosure')

  buttons.forEach((button) => {
    const panelId = button.getAttribute('aria-controls')
    const panel = panelId ? document.getElementById(panelId) : null
    const label = button.querySelector<HTMLElement>('.rpblc-app-card__disclosure-label')
    const chevron = button.querySelector<HTMLElement>('.rpblc-app-card__chevron')
    if (!panel || !label) {
      return
    }

    const setOpen = (open: boolean) => {
      button.setAttribute('aria-expanded', String(open))
      panel.hidden = !open
      label.textContent = open ? 'Hide details' : 'Show details'
      chevron?.classList.toggle('rpblc-app-card__chevron--open', open)
    }
    setOpen(button.getAttribute('aria-expanded') === 'true')

    const onClick = () => {
      setOpen(button.getAttribute('aria-expanded') !== 'true')
    }
    button.addEventListener('click', onClick)
    cleanups.push(() => button.removeEventListener('click', onClick))
  })

  return () => cleanups.forEach((cleanup) => cleanup())
}

function App(props: ShellProps) {
  const [themePreference, setThemePreference] = useState<ThemePreference>(readThemePreference)
  const moreActive = moreNav.some((item) => item.active === props.active)

  useEffect(() => {
    document.body.dataset.reactHydrated = 'true'
  }, [])

  useEffect(() => {
    applyThemePreference(themePreference)
    persistThemePreference(themePreference)
  }, [themePreference])

  useEffect(() => {
    if (props.active !== 'Settings') {
      return
    }
    return bindAppCardDisclosures(document.getElementById('dam-root') ?? document)
  }, [props.active, props.contentHtml])

  return (
    <main className="dam-react-shell">
      <div className="brand-bar">
        <a
          className="brand-home"
          href={props.brandUrl}
          target="_blank"
          rel="noopener noreferrer"
          aria-label="RPBLC home"
          data-tray-external={props.isTray ? 'rpblc' : undefined}
        >
          <BrandMark />
          <span className="brand-stamp">
            <span className="brand-product">DAM</span>
          </span>
        </a>

        <nav aria-label="Primary">
          {primaryNav.map((item) => (
            <NavLink key={item.href} item={item} active={props.active} />
          ))}
          <details className="nav-more">
            <summary
              aria-label="More"
              title="More"
              className={moreActive ? 'active' : undefined}
            >
              <span className="chevron-mark" aria-hidden="true" />
            </summary>
            <div className="nav-more-menu">
              {moreNav.map((item) => (
                <NavMenuLink key={item.href} item={item} active={props.active} />
              ))}
            </div>
          </details>
        </nav>

        <div className="brand-actions">
          {props.isTray && (
            <button
              className="tray-quit"
              type="button"
              data-tray-quit
              aria-label="Quit tray"
              title="Quit tray"
            >
              ⏻
            </button>
          )}
          <a
            className="brand-out"
            href={props.brandUrl}
            target="_blank"
            rel="noopener noreferrer"
            data-tray-external={props.isTray ? 'rpblc' : undefined}
          >
            RPBLC.com
          </a>
        </div>
      </div>

      <header>
        <div>
          <h1>{props.title}</h1>
          <div className="meta" dangerouslySetInnerHTML={{ __html: props.meta }} />
        </div>
        {props.countLabel && (
          <div className="count">
            <strong>{props.count}</strong> {props.countLabel}
          </div>
        )}
      </header>

      {props.active === 'Settings' && (
        <ThemeSettings preference={themePreference} onChange={setThemePreference} />
      )}
      <div
        className={props.contentClass}
        dangerouslySetInnerHTML={{ __html: props.contentHtml }}
      />
    </main>
  )
}

const props = readShellProps()
const root = document.getElementById('dam-root')

if (props && root) {
  createRoot(root).render(<App {...props} />)
}
