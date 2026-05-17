# Build And Release

`scripts/dam-build.sh` is the standard local and CI entry point for DAM builds.

## Commands

```bash
scripts/dam-build.sh check
scripts/dam-build.sh dev
scripts/dam-build.sh npm-native
scripts/dam-build.sh macos-app --mode developer-id
scripts/dam-build.sh notarize --app target/dam-build/macos/DAM.app --notary-profile DAM-notary
scripts/dam-build.sh release-macos --mode developer-id
scripts/dam-build.sh deploy-local --mode development
scripts/dam-build.sh agent-check
scripts/dam-build.sh agent-install --skip-checks
scripts/dam-build.sh agent-status --network-mode tun --trust-mode local_ca
```

`check` runs the repository verification suite: React/Vite UI dependency install and build for the embedded `dam-web` asset, npm package shim smoke tests, npm pack dry-run, Rust formatting, workspace clippy, workspace tests, and macOS Swift package tests when running on macOS.

`dev` builds the source-tree binaries used by local daemon/tray runs: `dam`, `damctl`, `dam-web`, and `dam-tray`.

`npm-native` builds current-platform release binaries for `dam`, `damctl`, `dam-web`, `dam-proxy`, `dam-mcp`, and `dam-tray`, then stages them under `npm/native/<platform>-<arch>/` for package smoke testing and release assembly.

`macos-app` delegates signed app assembly to `native/macos/scripts/package-dam-app.sh`, keeping entitlement and provisioning validation in the native macOS packaging script.

`notarize` zips an existing `DAM.app`, submits it with `xcrun notarytool`, staples the ticket, and validates the stapled app.

`release-macos` runs `check`, builds a signed Developer ID app by default, notarizes/staples it, and writes a release zip under `target/dam-build/macos`.

`deploy-local` builds or accepts an existing `DAM.app` and copies it to `/Applications` by default.

`agent-check` is the default verification command for local agents and maintainers. It runs `check` and adds `git diff --check` when the source tree is a git checkout.

`agent-install` is the idempotent local release-path install command for macOS. It optionally runs `agent-check`, builds the app, notarizes Developer ID builds unless notarization is disabled, stops the installed tray/web processes before replacing the app bundle, verifies the installed app, restarts the daemon with the persisted DAM configuration, opens the tray app, and prints `agent-status`.

`agent-status` inspects the installed app without mutating setup. It reports matching DAM processes, verifies code signing, validates notarization/Gatekeeper when notarization is enabled, and runs the installed `dam doctor --json`, `dam setup status --json`, `dam setup next-action --json`, and `dam status --json` probes. Setup probes default to the release-path `tun` + `local_ca` modes and can be overridden with `--network-mode` and `--trust-mode`. The npm wrapper package doctor remains part of `check`/`agent-check` through the npm smoke test; it is not an installed native app command.

## Environment

- `DAM_BUILD_OUT`: artifact root, default `target/dam-build`.
- `DAM_SIGN_MODE`: `development` or `developer-id`, default `developer-id`.
- `DAM_NOTARY_PROFILE`: notarytool keychain profile, default `DAM-notary`.
- `DAM_MACOS_TEAM_ID`: optional Team ID override passed through to macOS packaging.
- `DAM_MACOS_APP_GROUP_ID`: optional App Group override passed through to macOS packaging.
- `DAM_INSTALL_DIR`: local deploy destination, default `/Applications`.
- `DAM_SKIP_NOTARIZE`: set to `1` to skip notarization in `agent-install`.
- `DAM_RESTART_AFTER_INSTALL`: set to `0` to install without restarting daemon/tray.
- `DAM_AGENT_STATUS_STRICT`: set to `1` to make `agent-status` fail when any probe fails.
- `DAM_AGENT_NETWORK_MODE`: setup mode for `agent-status`, default `tun`.
- `DAM_AGENT_TRUST_MODE`: trust mode for `agent-status`, default `local_ca`.

The script intentionally keeps signing, provisioning, and notarization inputs in environment variables or keychain profiles. It must not require secrets in repository files.

For contributors without Developer ID/notary credentials, use a development build:

```bash
DAM_SIGN_MODE=development scripts/dam-build.sh agent-install --skip-notarize --no-restart
```

For maintainers validating the installed notarized path locally, use:

```bash
DAM_MACOS_APP_GROUP_ID=group.com.rpblc.dam scripts/dam-build.sh agent-install --skip-checks --require-notary
```
