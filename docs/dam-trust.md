# dam-trust

`dam-trust` defines the first TLS trust contracts for DAM's future transparent protection path.

It does not generate certificates, install a local CA, intercept TLS, decrypt traffic, or change system trust settings. It is a small control-plane crate that lets daemon, CLI, tray/web status, and future platform trust installers use the same vocabulary.

## Current Contracts

Trust modes:

```text
disabled   current default; no TLS trust changes
local_ca   planned local DAM CA mode for future transparent HTTPS/WSS protection
```

Platform trust stores:

```text
macos_keychain
windows_root_store
linux_nss_or_system_store
unknown
```

Trust actions:

```text
inspect           implemented; reports trust metadata without system changes
install_local_ca  planned; must require explicit user consent and rollback
remove_local_ca   planned; must remove DAM trust material cleanly
```

`TrustActionPlan` reports whether an action is implemented, requires admin rights, changes system trust, needs user consent, and requires rollback support.

## TLS Readiness

`dam-trust` combines a `dam-net` transparent route decision with local trust state:

```text
non-AI traffic              -> not in scope
HTTP/WS known AI traffic    -> TLS trust not required
HTTPS/WSS known AI traffic  -> needs trust checks
```

For encrypted AI traffic, readiness is explicit:

```text
disabled            TLS interception is disabled
host_not_allowed    host is outside the trusted AI host scope
needs_user_consent  user has not approved interception for this scope
needs_local_ca      local DAM CA is not installed
ready               host is allowed, user consented, local CA installed
```

The default trusted AI host scope comes from `dam-net`:

```text
api.openai.com
api.anthropic.com
api.x.ai
chatgpt.com
```

This list is a transparent-protection scope, not an egress policy allowlist.

## Current Consumers

- `dam-daemon` stores `trust.mode`, platform store metadata, and trusted AI host scope in `daemon.json`.
- `dam connect --trust-mode disabled|local_ca` records the selected trust mode for future UI/status flows.
- `dam status` prints `trust_mode` when daemon state exists.
- `damctl trust inspect` prints read-only trust readiness and trust action plans.
- `damctl daemon inspect` prints trust mode, platform store, local CA installed state, and trusted AI host count.

## Boundaries

`dam-trust` owns:

- trust-mode vocabulary;
- local CA metadata shape;
- platform trust-store tags;
- trusted AI host scope;
- TLS interception readiness decisions.

`dam-trust` does not own:

- OS trust-store mutation;
- certificate generation;
- TLS interception;
- packet or proxy routing;
- provider request/response handling;
- detection, policy, consent, vault, logging, or redaction.

Those stay in future platform trust installers, `dam-net`, `dam-proxy`, provider adapters, and the spine modules.

## Tests

```bash
cargo test -p dam-trust
```
