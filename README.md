<div align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/dam-wordmark-dark.svg">
    <img alt="[ RPBLC : DAM ]" src="assets/dam-wordmark-light.svg" width="320">
  </picture>
  <h3>Data Access Mediator</h3>
  <p><strong>A local privacy firewall for everything your machine sends out.</strong></p>
</div>

<p align="center">
  <a href="https://opensource.org/licenses/Apache-2.0"><img src="https://img.shields.io/badge/license-Apache_2.0-blue.svg" alt="License: Apache-2.0"></a>
  <a href="https://www.npmjs.com/package/@rpblc/dam"><img src="https://img.shields.io/badge/npm-%40rpblc%2Fdam-cb3837.svg" alt="npm: @rpblc/dam"></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/rust-1.94%2B-orange.svg" alt="Rust 1.94+"></a>
  <img src="https://img.shields.io/badge/checks-fmt%20%7C%20clippy%20%7C%20test-2ea44f.svg" alt="Checks: fmt, clippy, test">
</p>

<p align="center">
  <a href="#install">Install</a> &middot;
  <a href="#quick-start">Quick Start</a> &middot;
  <a href="#how-it-works">How It Works</a> &middot;
  <a href="docs/README.md">Docs</a> &middot;
  <a href="#status">Status</a>
</p>

---

## What it is

DAM runs on your machine, between what you send and where it goes. It detects sensitive
values before they leave, applies your local policy and consent, replaces protected values
with stable references, and keeps the originals in a local vault — so the other side only
sees what you meant to share.

It works on outbound traffic in general. **V1 starts with AI agents, assistants, and
harnesses**, because that is the most practical traffic to protect first — but the same
mediation applies to any client routed through DAM.

```text
You type:

  "Email banana@banana.com and include card 4111-1111-1111-1111"

DAM sends upstream:

  "Email [email:BhjEUc1EX1JHLbeT7JUS6g]
   and include card [cc:7j21sVjW3aN4xFqP9L6MRA]"

The other side can reason over:

  "there is an email"     "there is a card"

The other side never receives:

  banana@banana.com       4111-1111-1111-1111
```

## Install

The current MVP install path is staged around the macOS app and the npm/native shims.
Linux and Windows are design targets, but their routing, trust, tray, and packaging paths
are still landing in slices. On those platforms, use source builds and explicit proxy
experiments only; macOS-specific network mutation commands report `unsupported_platform`
with fallback guidance instead of changing host networking.

```bash
npm install -g @rpblc/dam
```

From a source checkout:

```bash
cargo build -p dam -p dam-web -p dam-tray
```

Per-platform native packages are staged — see [docs/build-release.md](docs/build-release.md).

## Quick Start

Start the tray. It puts a `[R:]` item in the menu bar and walks you through
setup, trust, and routing — no flags to memorize.

```bash
dam-tray
```

Click the `[R:]` menu-bar item and hit **Connect**. To inspect the full
idempotent setup checklist or the running daemon state:

```bash
dam setup status --json
dam status --json
```

From a source checkout instead:

```bash
cargo build -p dam -p dam-web -p dam-tray
cargo run -p dam-tray
```

Headless setup, app profiles (Claude, ChatGPT, …), and every routing/trust
flag are in [docs/dam.md](docs/dam.md),
[docs/dam-tray.md](docs/dam-tray.md), and
[docs/dam-integrations.md](docs/dam-integrations.md).

## How It Works

```text
              local machine                                      destination

  request ──► dam daemon ──────────► dam-proxy ─────────────────► upstream
              │                 │                                    │
              │                 ├─ detect sensitive values           │
              │                 ├─ apply policy                      │
              │                 ├─ apply active consents             │
              │                 ├─ write tokenized values to vault   │
              │                 ├─ redact outbound request           │
              │                 └─ write non-sensitive log events    │
              │                                                      │
              ◄──────────────── response with DAM references ◄────────┘

  vault.db       protected values for tokenized references, local SQLite
  consent.db     exact-value passthrough grants with TTL
  log.db         event metadata, not raw detected values
```

The outbound pipeline is one shape across the proxy and the `dam-filter` CLI:

```text
input -> dam-detect -> dam-policy -> dam-consent -> dam-core plan
      -> dam-vault -> dam-redact -> output
```

Full pipeline, resolve path, and control/diagnostics flows are in
[docs/README.md](docs/README.md).

## Highlights

- **Local-first.** Everything — detection, vault, log, consent — runs and stays on your machine.
- **Detect → redact → vault.** Sensitive values are swapped for stable references; originals never leave.
- **Consent.** Grant a specific value temporary passthrough with a TTL; revoke any time.
- **Web UI.** A local control surface for connecting, the vault, allowed values, and logs.
- **Tray app.** A native desktop shell hosting the Connect surface.
- **MCP server.** An agent can inspect DAM status and manage consent through standard tools.

## Docs

| You want to… | Read |
|---|---|
| Configure DAM | [docs/dam-config.md](docs/dam-config.md) |
| Full command reference | [docs/dam.md](docs/dam.md) · [docs/damctl.md](docs/damctl.md) |
| Connect apps / harnesses | [docs/dam-integrations.md](docs/dam-integrations.md) |
| Understand consent | [docs/dam-consent.md](docs/dam-consent.md) |
| Use the web UI / tray | [docs/dam-web.md](docs/dam-web.md) · [docs/dam-tray.md](docs/dam-tray.md) |
| Drive DAM from an agent | [docs/dam-mcp.md](docs/dam-mcp.md) · [docs/dam-api.md](docs/dam-api.md) |
| What's detected | [docs/dam-detect.md](docs/dam-detect.md) · [docs/dam-policy.md](docs/dam-policy.md) |
| Build & release | [docs/build-release.md](docs/build-release.md) |
| Module map & architecture | [docs/README.md](docs/README.md) |

## Status

- **V1 scope.** DAM protects clients routed through it as an HTTP(S) proxy, the macOS PAC
  fallback, and the macOS `tun`/Network Extension path. Unknown hosts pass through untouched.
- **Platforms.** Designed for macOS, Linux, and Windows; platform routing, trust, tray, and
  packaging land in staged slices. Partial or delayed behavior is tracked in
  [docs/parking-lot.md](docs/parking-lot.md).
- **Detection is intentionally narrow.** Email, NANP phone, US SSN, Luhn-validated
  cards, common API-key assignment labels, direct OpenAI/Anthropic/GitHub/Stripe/Slack-app-token/Discord/Microsoft Teams/Google/SendGrid/Mailgun/AWS
  API-key families, Stripe webhook signing secrets, Slack, Discord, and Microsoft Teams incoming webhook URLs, PEM private key blocks, database connection URLs, and `Bearer`-labeled JWTs today. Names, addresses, unlabeled
  bearer tokens, IBANs, and IPs are not covered yet.
- **Local stores.** Vault, log, and consent are local SQLite. The web UI shows vault
  values in clear text — treat it as a local control surface, not a public app.

## Build

```bash
cargo fmt --all --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

## License

Apache-2.0.
