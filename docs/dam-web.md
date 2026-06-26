# dam-web

`dam-web` is the local web UI.

It is being rebuilt from the architecture specs and `RPBLC.Design`. The current React slice includes the shared web/tray app frame, the pinned brand/navigation bar, Connect, Wallet, Activity, Insights, System, Health, and Settings.

The app frame is a React shell served from embedded `/assets/bundle.js`, `/assets/bundle.css`, and `/assets/index.html` build output.

## Current Routes

```text
/                 smart landing: web redirects to /insights when protected, /connect otherwise; tray stays on /connect
/connect          Connect surface
/insights         web privacy-dividend dashboard
/wallet           local Wallet and allowed-consent roster
/allowed          compatibility redirect to /wallet?state=allowed
/activity         dam-log derived activity feed
/settings         local preferences, integrations, and daemon controls
/system           web-only operator system log
/health           web-only doctor/diagnostics health surface
/*                frame fallback
```

The backend `/api/v1/*` routes remain available for upcoming page slices. Connect fetches `/api/v1/connect`, posts setup/action requests to `/api/v1/connect/action`, reads `/api/v1/requests/pending` while protected, and can use the local QA-only `/api/v1/requests/trigger` endpoint to simulate an inbound consent request. Agent callers can fetch `/api/v1/setup/plan` for the full setup checklist, `/api/v1/setup/next-action` for the next idempotent setup action, post `/api/v1/setup/rescue` for local setup rescue, post `/api/v1/setup/repair` for rescue plus a fresh setup plan, or fetch `/api/v1/setup/diagnostics` for an offline doctor/setup/rescue-preview bundle without depending on Connect page copy. Rescue and repair preview by default; mutating calls require `{"apply": true, "confirm": "remove_dam_network_setup"}` and leave local CA trust and vault data intact. The protected-state view reads `protected_since_unix` from `/api/v1/connect` and renders a live elapsed timer from that backend timestamp rather than keeping a client-side checkpoint. The Connect counts row is backed by live local stores: distinct active wallet values allowed by `dam-consent`, denied Activity rows for the current UTC day derived from `dam-log` through the web activity taxonomy, a separate redaction dividend count kept on the wire for local clients that still use it, and enabled integration profiles from `dam-integrations`. The shared `/api/v1/connect` query now re-fetches through a single visibility-aware observer while visible, and the Connect page, tray footer, and browser/tray status chrome read the same cached result because proxy-written `dam-log` rows are outside `dam-web`'s in-process event bus; SSE still invalidates in-process connect state changes immediately. The tiles link to `/wallet?state=allowed`, `/activity?decision=denied&since=today`, and `/settings#apps`.

When `DAM_WEB_SHELL=tray`, `dam-web` renders the tray brand bar and the same Connect page inside the hosted WebView. Browser mode renders the same app navbar. Both surfaces show `[R:] DAM`, the divider line, and the connection status mark.

The `[R:]` brand mark uses `data-tray-external="rpblc"` in tray mode so `dam-tray` can open `https://rpblc.com` through the native shell. The tray `DAM` product stamp uses `data-tray-external="dam-web-tab"` and posts `dam-tray:open-dam-web` to the native shell. If `DAM_WEB_TRAY_POST_TOKEN` is set, React API calls include it as `x-dam-web-tray-token`.

When Connect is not fully protected, the shared brand/status bar now mirrors the backend setup summary instead of collapsing every non-protected state to `off`. `waiting_for_approval` renders `approval needed`; `requested`, `configured`, `enabled`, `connected`, and `waiting_for_reboot` render `connecting`; `rolled_back` and blocked `failed` render `repair needed`; generic `needs_setup` stays `setup needed`; generic degraded fallback stays `attention`.

Connect action wiring is intentionally narrow in this slice: browser-hosted `connect`, `resume`, and `pause` toggle the local protected state for the current process, while tray-hosted Connect posts native IPC so `dam-tray` can own privileged setup. The setup checklist distinguishes macOS System Extension approval (`ne_install`), reboot (`ne_reboot`), Network Extension manager configuration (`ne_config`), manager enablement (`ne_enable`), manager start/connection verification (`ne_start`), local CA trust, optional system-proxy fallback setup (`system_proxy`), and daemon start. Checklist rows carry diagnostics `detail` values so the frontend can branch on stable reason codes rather than English messages. The page-level status line uses those same stable `detail` codes to surface explicit requested / waiting-for-approval / waiting-for-reboot / configured / enabled / connected / rolled-back / failed copy instead of a generic setup spinner. Its current step comes from the diagnostics `next_action`, so a blocked setup inspection is not hidden behind a later todo item. Explicit-proxy profile apply is available from integrations/settings but is not part of Connect onboarding. Linux and Windows use separate stable setup ids (`linux_capture`, `windows_capture`) so their future onboarding can diverge without reusing macOS Network Extension copy. Unknown setup/recovery step ids still return `not_implemented`, and the frontend maps stable error codes to localized English and French copy instead of showing raw backend text.

## Activity

`GET /api/v1/activity?since=&after_id=&limit=&decision=&q=` reads `dam-log` through its bounded indexed query API and maps person-facing events into the CTZN activity feed. When `since` is omitted, the API defaults to the last hour so the Activity view does not open as a full log dump; use `since=0` for all time. The API accepts an id cursor for future incremental callers and caps returned Activity rows so repeated refreshes stay cheap on local machines. The React Activity duration selector is available on web and tray and defaults to `1h`; its `today` shortcut resolves to the current UTC midnight so Connect’s blocked-today tile and the denied Activity filter share the same day window. The mapper currently includes:

- `policy_decision.allow` → `granted`
- `redaction.*` → `sealed`
- `policy_decision.block` → `denied`
- `proxy_failure.provider_down` → `denied`

Sealed `policy_decision.tokenize` and `policy_decision.redact` rows are diagnostic pipeline steps, not separate Activity rows; the paired `redaction.*` event is the user-facing sealed event. Request protection summary rows are diagnostic transport logs, not Activity rows. When a proxy protection event does not carry profile context itself, `dam-web` derives the traffic target from another log row with the same operation id, such as `route_decision target=...` or `provider_forward_start provider=...`, then resolves that target through the integration profile catalog. Target and provider ids are transport details and should not be shown as the user-facing profile label when a configured profile owns that route.

The Activity page polls this endpoint only while mounted and visible, refetches on window focus, and is invalidated by the existing local SSE stream when connect, wallet, or request state changes. Rows are detection/outcome log entries, not Wallet rows: ordinary Activity copy shows a safe detected-type placeholder such as `[email]`, a compact outcome protection mark, the profile, and the token reference such as `[email:<reference-id>]` when the log row carries one. The list does not reveal raw protected values; intentional clear-value reveal stays in local Wallet/value-detail flows. Rows with a stored Activity value can still post that value to `POST /api/v1/wallet` as a one-way convenience action, but the Wallet remains a separate user-maintained list of preferred values for quick access and provider-scoped allows. Activity does not read Wallet values or expose Wallet ids. The Activity allow-once action is parked and is not rendered in this slice.

## Wallet

`GET /api/v1/wallet?q=&state=&sort=&dir=` reads user-maintained `dam-vault` Wallet entries, joins consent state from `dam-consent`, and returns protected, allowed, revoked, and expired value rows. The React Wallet surface owns stored-value management: users can add a value directly to the wallet, filter to allowed values, inspect the compact Allowed roster, allow a value for all profiles or selected integration profiles, revoke access from a roster row, or remove the value from the wallet. Automatic proxy token-vault entries do not appear here. `shared_with` contains active grants only; revoked or expired grants do not render as selected profiles and can be allowed again. Profile choices are populated from the Settings profile list, not hardcoded into the component. The multi-select dropdown applies each selection change immediately, uses an opaque panel background, and is allowed to render outside the inline detail border after the detail open animation settles. Profile-level allows expand to target-scoped consent grants derived from the selected profile's traffic apps. Removing a value revokes active grants for that wallet key before deleting the Wallet row; the reversible token-vault mapping may remain so historical references can still resolve locally.

`POST /api/v1/wallet` adds a stored value with `{ kind, value }`. It ensures a token-vault reference exists, then records that reference in the Wallet table. After a successful add, the React Wallet clears any active search/state filter, scrolls the returned row near the top of the view, opens its detail after the scroll starts, and runs a second reveal pass after the inline detail expands so the panel stays visible. The Wallet search/filter header is sticky so long wallet lists stay controllable while scrolling. `POST /api/v1/wallet/:key/allow` accepts a legacy global `{ party }`, an explicit all-profile `{ party, scope: "global" }`, or a profile-level `{ party, profile_id }`. `POST /api/v1/wallet/:key/remove` removes that Wallet row. Mutating wallet routes notify the Wallet and Connect event topics so the list and Connect counts refresh.

`GET /api/v1/allowed?q=&sort=&dir=` remains as a compatibility/headless API that reads `dam-consent`, groups grants into active, expired, and revoked buckets, and joins each grant to `dam-vault` when the grant has a vault key. The old Allowed React page has been removed; `/allowed` redirects to `/wallet?state=allowed`.

## Settings

`GET /api/v1/settings` builds a live view from `dam-daemon`, `dam-config`, and `dam-integrations`. The Apps section is wired for the MVP-visible app catalog: Claude and ChatGPT, where ChatGPT covers OpenAI API, ChatGPT-login, and Codex traffic under the hood. Enabling an app records its profile as enabled; disabling clears that enabled state. Settings app-toggle routes reject ids outside this MVP-visible catalog with `invalid_request`, so hidden imported/custom profile JSON cannot be enabled accidentally through the Settings API. The custom profile creator is parked, and imported/custom profile JSON files remain headless CLI data rather than visible Settings rows until the profile-builder/import UX is designed end to end. Each visible app row also exposes detector toggles for the supported MVP kinds (`email`, `phone`, `ssn`, `credit_card`, `api_key`). These toggles are profile-scoped, default to DAM's current protection posture, persist in the local integrations state, and only relax the selected kind for that visible app/profile; hidden profile ids and unknown detector keys are rejected with `invalid_request`.

While an app toggle mutation is pending, the row keeps the optimistic switch position, disables the switch, and shows the compact redaction loader because connected installs may need a daemon reconnect. When a running daemon exists, app toggles reconcile the platform capture scope from the enabled profiles and invoke `dam connect` without explicit target arguments so the CLI expands the same enabled-profile JSON state used by Connect. Reconnect uses the web process' absolute vault/log/consent paths when the existing daemon state only has relative paths, then verifies the restarted daemon's route and target scope. If runtime reconciliation fails for a hard error, the enabled-state change is rolled back and the API returns `setup_step_failed`. If macOS or the `dam connect` preflight reports that Network Extension approval or setup is still pending, Settings keeps the enabled profile state and lets Connect/setup show the next required action; it must not present the route as protected until setup and daemon scope match. Turning every app off leaves an explicit empty enabled state, reconfigures macOS Network Extension capture with no protected hosts, and keeps unrelated traffic outside DAM.

The Network section is read-only and reflects the latest daemon state on disk. `ready` is true only when protection is enabled and every transparent interception route reports `ready`. Connect reports `degraded`, not `protected`, when enabled profile state expects routes that the running daemon does not expose.

Defaults are shown as disabled controls in this slice. `POST /api/v1/settings/defaults`, reset, and uninstall still return `not_implemented` until the runtime settings store and destructive flows are designed end to end.

## Local Request Trigger

`POST /api/v1/requests/trigger` creates an in-memory pending consent request and marks the current `dam-web` process protected. It is for local QA and screenshots until request delivery moves to `dam-notify`.

```bash
curl -sS -X POST http://127.0.0.1:2896/api/v1/requests/trigger \
  -H 'Content-Type: application/json' \
  -H 'Origin: http://127.0.0.1:2896' \
  -d '{
    "actor": "sample-profile",
    "value_label": "mobile phone",
    "value_preview": "+1 415 555 0142",
    "purpose": "send the verification code from your bank to confirm the wire",
    "expires_in_sec": 18000
  }'
```

Each `dam-web` process has its own request store. When web and tray are running on separate local ports, trigger the request on both ports to test both surfaces.

## Usage

```bash
dam web --config dam.example.toml
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

## Localization

All visible text in the current React slice is catalog-driven in English and French. Runtime locale defaults to the system language and can be overridden with `localStorage["rpblc.dam.locale"] = "en"` or `"fr"`.

The full Lingui catalog flow in the architecture is not wired yet. The current UI keeps a small local typed catalog in `ui/src/lib/i18n.ts` so no visible text is hardcoded in page components.

## Security Posture

This UI displays vault values in clear text and can add, remove, allow, and protect canonical values. Treat it as a local development/admin tool, not a public-facing service.

Connect/settings mutation routes are POST-only and use the same local Host and Origin/Referer guardrails as consent mutation routes.

## Branding

The UI follows `RPBLC.Design`:

- Inlined RPBLC design tokens for color, type, spacing, motion, and geometry.
- Theme defaults to the system preference. Persisted System, Light, and Dark settings return with the Settings page.
- Warm gold accent.
- `[R:]` brand mark.
- Product stamp: `DAM`.
- Web frame: pinned top app navbar with `[R:] DAM`.
- Tray frame: same pinned app navbar with `[R:] DAM`; `DAM` opens the hosted browser view.
- App chrome uses the reversed bar treatment from `RPBLC.Design` so logged-in/local product surfaces are distinct from the public website while preserving the same mark size and glyph behavior.
- `@rpblc/design/fonts.css` is imported so the app uses the design-system faces everywhere: Manrope for reading text and JetBrains Mono for marks, labels, counters, and controls.
- `/favicon.svg` served from the same SVG as `RPBLC.public/public/favicon.svg`.
- External link to `https://rpblc.com`.

The local UI vendors the current `RPBLC.Design/src` under `ui/src/design-system` and aliases `@rpblc/design` to that copy. This keeps call sites aligned with the future package import while the generated design-system library is not available yet.

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

The build writes `crates/dam-web/assets/index.html`, `crates/dam-web/assets/bundle.js`, and `crates/dam-web/assets/bundle.css`, which are embedded into `dam-web` with `include_str!` and served locally. Runtime does not fetch React or app scripts from a CDN. Font loading follows `@rpblc/design/fonts.css` so DAM matches the public-site typography.

## Tests

```bash
npm run build --prefix crates/dam-web/ui
cargo test -p dam-web
```
