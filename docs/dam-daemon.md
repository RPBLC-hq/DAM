# dam-daemon

`dam-daemon` owns the first background lifecycle slice for local DAM.

It is not a protection engine. The daemon opens the same `dam-proxy` app used by the launcher, writes local process state, waits for shutdown, and removes its state file when it exits cleanly.

## UX Surface

The intended user-facing commands live on `dam`:

```bash
dam connect
dam status
dam disconnect
```

`dam connect` starts a background daemon process by re-running the current `dam` executable through an internal `daemon-run` command. This keeps `cargo run -p dam -- connect` and installed `dam connect` on the same path.

The standalone service entry point also exists:

```bash
cargo run -p dam-daemon -- run
```

## Defaults

`dam connect` defaults to an OpenAI-compatible local endpoint:

```text
listen: 127.0.0.1:7828
target: openai
provider: openai-compatible
upstream: https://api.openai.com
local base URL for OpenAI-compatible harnesses: http://127.0.0.1:7828/v1
```

Use the Anthropic preset when the harness expects Anthropic-compatible traffic:

```bash
dam connect --anthropic
```

That starts the same daemon/proxy lifecycle with:

```text
target: anthropic
provider: anthropic
upstream: https://api.anthropic.com
local base URL for Anthropic-compatible harnesses: http://127.0.0.1:7828
```

Both presets use caller-owned provider auth headers by default. DAM does not store provider API keys for local daemon mode.

## State File

The daemon writes a JSON state file atomically at:

```text
$DAM_STATE_DIR/daemon.json
```

When `DAM_STATE_DIR` is unset, the fallback is:

```text
$HOME/.dam/daemon.json
```

The state file contains non-sensitive local lifecycle information:

- daemon PID;
- listen address and proxy URL;
- config path when one was supplied;
- local vault/log/consent SQLite paths;
- inbound reference resolution setting;
- target name, provider, and upstream URL;
- network mode (`explicit_proxy`, `system_proxy`, or `tun`);
- known transparent AI routes from `dam-net`;
- trust mode and non-sensitive `dam-trust` readiness metadata;
- daemon start time as a Unix timestamp.

It must not contain raw sensitive values, vault values, provider API keys, or auth headers.

## Commands

```bash
dam connect [--openai|--anthropic] [DAM_OPTIONS]
dam connect --profile <profile> [--apply]
dam connect --apply
dam status [--json]
dam disconnect
```

Daemon options:

```text
--openai             Use the OpenAI-compatible preset (default)
--anthropic          Use the Anthropic preset
--config <path>      Load DAM config before daemon overrides
--listen <addr>      Local proxy listen address
--network-mode <mode> Control-plane network mode: explicit_proxy, system_proxy, or tun
--trust-mode <mode>  Control-plane trust mode: disabled or local_ca
--target-name <name> Proxy target name
--provider <name>    Provider adapter: openai-compatible or anthropic
--upstream <url>     Provider upstream URL
--db <path>          Local SQLite vault path
--log <path>         Local SQLite log path
--consent-db <path>  Local SQLite consent path
--no-log             Disable log writes
--no-resolve-inbound Leave DAM references unresolved in inbound responses
--resolve-inbound    Restore DAM references in inbound responses
```

`--profile` and `--apply` are `dam connect` front-end options. They are resolved before daemon startup and are not accepted by the standalone `dam-daemon run` parser.

`dam status --json` emits a local status envelope containing daemon state and, when reachable, the `dam-api` `ProxyReport` returned by `/health`.

`network_mode` is currently a control-plane/status field. `explicit_proxy` is the implemented mode. `system_proxy` and `tun` are reserved for future OS routing modules and do not install routes or create a TUN device yet.

`trust_mode` is also currently a control-plane/status field. `disabled` is the only mode with current behavior. `local_ca` records the intended future TLS trust mode, but it does not install a local CA, change OS trust settings, or intercept TLS yet.

`damctl daemon inspect` is the read-only support/debug view over the same state file. It reports `connected`, `stale`, or `disconnected`, state file paths, process status, selected proxy target, local database paths, and inbound resolution settings without starting or stopping the daemon.

## Current Limits

- The daemon runs one proxy target at a time.
- It does not install system proxy settings, mutate harness configs, or start at login. The first tray/menu-bar shell lives in `dam-tray` and hosts `dam-web /connect`; it does not change daemon lifecycle behavior.
- It records future `system_proxy` and `tun` modes but does not activate OS routing for them yet.
- It records future `local_ca` trust mode but does not install certificates or intercept TLS yet.
- It does not add WebSocket handling.
- `dam disconnect` terminates the daemon process by PID, escalates if the process ignores the first termination signal, and removes stale state when the process is no longer running.
