# dam-web

`dam-web` is the local web UI.

It is for development inspection of local SQLite vault, consent, and log databases, and it hosts the visual Connect surface used directly in a browser or through `dam-tray`.

The app frame is a React shell served from `/assets/dam-web-ui.js`. Rust still renders the route content as a server-side fallback so the local UI remains usable without JavaScript and existing POST flows keep working.

## Routes

```text
/connect   local protection surface for enabled apps, setup, connect, and disconnect
/settings  theme and app/profile configuration surface for protected harnesses
/          smart landing route: Connect when disconnected, Wallet when connected
/vault     data wallet with protected values and row-level allow/protect actions
/vault/detail/:key  value metadata and audit detail view
/allowed   data currently allowed through DAM protection
/consents  compatibility alias for /allowed
/logs      operational log events
/doctor    local readiness checks shared with damctl doctor
/diagnostics  damctl-style config and proxy checks
/health    health check
```

Wallet-row Allowed state is exact-value based. If duplicate wallet rows hold the same value, an active consent grant from one row appears as allowed on every matching row, and protecting it again stops passthrough for that exact value.

The wallet and logs support ordering. Wallet uses a single cycle sort button and defaults to most recently seen first; logs keep table header ordering. They use query parameters:

```text
/vault?sort=value&dir=asc
/logs?sort=time&dir=desc
```

`/connect` uses the enabled integration state managed by `dam-integrations`. It can enable or disable known app profiles, start DAM, pause protection, and expose apply/rollback controls when rollback records are available. The primary Connect action consumes the shared `dam-diagnostics` setup plan and advances setup in order: explicit proxy fallback for enabled CLI profiles, macOS Network Extension routing, local CA trust, then daemon connect. Routing and trust changes require a short confirmation before the web UI shells out to `dam network ... --yes` or `dam trust ... --yes`. The final daemon start uses `dam connect --apply --network-mode tun --trust-mode local_ca` when apps are enabled; enabled profiles select the daemon targets and keep reversible proxy setup as a fallback for source builds and unsupported environments. Pause calls `dam disconnect`, which leaves the daemon active in pass-through mode so running clients keep network connectivity. Resuming protection closes selected-AI pass-through tunnels opened while paused and lets the client reconnect through protected interception. Full restore/stop remains an explicit CLI path.

Without enabled apps, the visible default is Protect Everything and Connect uses the default OpenAI-compatible target. With one or more enabled apps, the same Connect action applies reversible explicit-proxy fallback and starts one daemon with the required provider targets. The Apps toggle shows enabled apps inline, with the chevron at the far right. App profiles are shown as compact two-line rows with technical details behind disclosure. `/settings` exposes a compact theme segmented control plus app enable/disable controls rendered in the shared AppIntegrationCard pattern. `dam-tray` hosts this route in a native desktop shell.

When `DAM_WEB_SHELL=tray`, `dam-web` renders a compact tray shell with a navbar power-icon Quit tray button and routes the `[R:]` brand link through the native tray bridge so `https://rpblc.com` opens in the default browser. The tray-hosted Connect button is routed through native IPC so system trust prompts originate from `dam-tray`, not from the hosted web child. If `DAM_WEB_TRAY_POST_TOKEN` is set, tray-mode pages attach that token to same-origin POST form actions so embedded WebView submits do not depend on `Origin` / `Referer` headers. Browser mode remains the default and keeps the normal local-origin POST guard.

The Connect action shells out to the local `dam` binary from `PATH`. Set `DAM_BIN=/path/to/dam` for source-tree runs or custom installs.

## Usage

```bash
cargo run -p dam-web -- --config dam.example.toml
```

With explicit paths:

```bash
cargo run -p dam-web -- \
  --db vault.db \
  --log log.db \
  --addr 127.0.0.1:2896
```

Default address:

```text
127.0.0.1:2896
```

`--addr` must be loopback in the current local build.

## Config Requirements

`dam-web` currently requires:

- `vault.backend = "sqlite"`
- `consent.backend = "sqlite"` when consent is enabled
- `log.backend = "sqlite"`

Remote vault/consent/log views are not implemented yet.

## Diagnostics

`/doctor` shows the shared `dam-diagnostics` readiness report used by `damctl doctor`, with local SQLite paths redacted for the web surface.

`/diagnostics` shows:

- config health using the same `dam-api` `HealthReport` shape used by `damctl config check`;
- proxy protection state from `dam-proxy /health` using `dam-api` `ProxyReport`;
- local warnings such as disabled proxy, missing proxy API key env vars, unsupported providers, and unreachable proxy.

## Security Posture

This UI displays vault values in clear text and can allow/protect exact values. Treat it as a local development/admin tool, not a public-facing service.

Connect/settings mutation routes are POST-only and use the same local Host and Origin/Referer guardrails as consent mutation routes.

## Branding

The UI follows `RPBLC.Design`:

- Inlined RPBLC design tokens for color, type, spacing, motion, and geometry.
- Primary CTAs consume `--cta-*`: gold in dark theme, ink in light theme, and gold on light hover.
- Theme defaults to the system preference and supports persisted System, Light, and Dark choices.
- Warm gold accent.
- `[R:]` brand mark.
- Product stamp: `DAM`.
- React-owned app shell with Connect, Wallet, and Allowed as primary nav; Settings, diagnostic, and activity views live under an icon-only chevron menu.
- Person-facing wallet surfaces mirror `WalletCard`: the protected value is the hero, state/source metadata stays on one muted row, and the row action is visible. Advanced/debug routes may still use dense tables.
- Wallet sort mirrors `CycleButton`; advanced log tables mirror `SortHeader`.
- Settings mirrors compact `Section`, `SegmentedControl`, and `AppIntegrationCard`.
- Quiet operator surfaces: rectangular buttons, bordered panels, compact lists, and no offset shadow treatment.
- `/favicon.svg` served from the same SVG as `RPBLC.public/public/favicon.svg`.
- External link to `https://rpblc.com`.

## React Shell

Source lives in:

```text
crates/dam-web/ui
```

Build the embedded asset with:

```bash
cd crates/dam-web/ui
npm install
npm run build
```

The build writes `crates/dam-web/assets/dam-web-ui.js`, which is embedded into `dam-web` with `include_str!` and served locally. Runtime does not fetch React, fonts, or scripts from a CDN.

## Tests

```bash
npm run build --prefix crates/dam-web/ui
cargo test -p dam-web
```
