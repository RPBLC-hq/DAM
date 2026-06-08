# dam-net-macos

`dam-net-macos` is the macOS platform routing implementation for DAM's local traffic mediation roadmap.

It manages macOS Auto Proxy Configuration through `networksetup` and a DAM-generated PAC file. The PAC routes proxy-capable HTTP and HTTPS traffic to the local DAM proxy, while bypassing localhost, plain hostnames, `.local`, loopback, IPv4 and IPv6 link-local, IPv4 private LAN, and IPv6 ULA ranges.

It also owns the macOS Network Extension control-plane used by `tun` mode. `dam-tray` owns the app-process System Extension activation request for packaged builds, because macOS keeps user approval pending only while the requesting app is alive. The tray refreshes `systemextensionsctl` before activation so `activated waiting for user` maps to the System Settings > General > Login Items & Extensions approval action and `activated enabled` proceeds to setup only when the installed build is at least the bundled build. Missing, disabled, or stale System Extension state returns onboarding to the System Extension step. System Extension approval, `NETransparentProxyManager` network configuration consent, manager enablement, and manager connection are separate setup states: after app-process activation succeeds, DAM records `system_extension_ready_needs_network_configuration` and the setup checklist advances to the network-configuration step before invoking the helper. `dam network install-network-extension` then plans configuration through the native Swift helper/provider package under `native/macos` (`DAM_MACOS_NE_HELPER` in source builds, or the signed helper app bundle in release builds). The helper configures `NETransparentProxyManager` with on-demand enabled; it does not submit System Extension activation. If macOS saves the Network Extension configuration but leaves it disabled or waiting for user approval, DAM records an inactive manager-enable state. On retry, once macOS reports the manager enabled but disconnected, the helper attempts to start the manager and waits for `connected`; a timeout disables the manager again to preserve normal networking and records a `rolled_back` setup detail rather than false active capture. After any non-empty helper install reports success, DAM also runs a local recovery gate: it asks the helper for live manager status and requires `configured`, `enabled`, and `connected` before recording active capture. If that verification fails, DAM asks the helper to remove the DAM Network Extension manager, records `network_extension_recovery_gate_rolled_back`, and returns setup to the repair/retry state. If that automatic removal fails, DAM records `network_extension_recovery_gate_failed` so setup blocks on `dam setup repair --yes` instead of advancing into trust or daemon startup. The app/helper process that writes Network Extension preferences must also carry the Network Extension entitlement, not only `system-extension.install`, and the embedded provisioning profiles must authorize the signed Network Extension and App Group entitlements. In packaged builds the helper is wrapped as an app bundle for AMFI entitlement validation and must embed its own copy of the provider system extension so NetworkExtension can persist the provider designated requirement. Without that helper, install fails closed instead of recording false active capture.

## Commands

The user-facing commands live under `dam network`:

```bash
dam network install-system-proxy [--config PATH] [--dry-run|--yes] [--json]
dam network remove-system-proxy [--dry-run|--yes] [--json]
dam network install-network-extension [--config PATH] [--dry-run|--yes] [--json]
dam network remove-network-extension [--dry-run|--yes] [--json]
dam network status [--json]
dam setup rescue [--dry-run|--yes] [--json]
```

Network mutation commands preview by default. `--yes` is required before DAM changes macOS network settings.

`dam setup rescue --yes` is the broad local recovery path for a broken setup. It stops the DAM daemon if needed and removes DAM-managed macOS system proxy plus Network Extension routing state. It intentionally leaves local CA trust and vault data untouched.

Without `--config`, the protected-host comments in the generated PAC and the Network Extension provider configuration are derived from the effective default app profile selection: `api.openai.com`, `platform.openai.com`, `api.anthropic.com`, `claude.ai`, `console.anthropic.com`, `mcp-proxy.anthropic.com`, `platform.claude.com`, `chatgpt.com`, `ab.chatgpt.com`, and `chat.openai.com`. With `--config`, those comments and provider configuration use the effective `[traffic]` profile from that config file. Runtime enabled app profiles override that default scope for `dam network install-*` and `dam connect`: if the enabled-profile state exists but contains zero profiles, DAM still runs setup for the macOS Network Extension and local CA, then configures an explicit empty protected-host list and leaves the Network Extension manager disabled so no traffic is mediated until an app profile is enabled. The helper's install defaults still provide the MVP hosts when no host flags are supplied, but a provider that starts with missing or incomplete provider configuration treats the protected-host scope as empty so stale manager state cannot unexpectedly capture bundled targets. The architecture is not AI-only: future traffic profiles can configure other destinations and protocol adapters. PAC routing scope is still all proxy-capable HTTP/HTTPS traffic. Network Extension routing installs broad outbound TCP rules for HTTP/HTTPS plus configured-host UDP/443 rules. Configured UDP/443 is not inspected yet; when DAM is protected, the provider closes those flows to force client fallback to TCP/TLS, and when DAM is paused or unhealthy the routing failure policy decides whether the UDP flow passes through or is blocked.

## State

DAM writes routing state under:

```text
$DAM_STATE_DIR/network/macos-system-proxy/latest.json
$DAM_STATE_DIR/network/macos-system-proxy/dam-ai-proxy.pac
$DAM_STATE_DIR/network/macos-network-extension/latest.json
```

The rollback record is written only after all `networksetup` mutations succeed. It stores the previous Auto Proxy URL and enabled state for each active macOS network service. Removal restores those values and deletes the DAM rollback/PAC files after successful restoration.

The Network Extension state record stores the activated bundle identifier, optional team identifier, active traffic profile hosts, and activation/configuration method. Active records are written only after app-process activation is approved, the installed System Extension build is at least the bundled build, and the native helper confirms the manager is connected. Inactive records are explicit: `system_extension_ready_needs_network_configuration`, `network_extension_configured_needs_enable`, `network_extension_enabled_needs_start`, `network_extension_recovery_gate_rolled_back`, and `network_extension_recovery_gate_failed` each map to one onboarding or repair action. Pending-reboot records represent a macOS System Extension transition only and are scoped to the boot that produced them; after reboot, DAM clears that transition by re-checking the live System Extension state and still requires the helper to configure and verify `NETransparentProxyManager` before capture becomes active. Status checks ask the native helper for live manager presence, enabled state, and `NETransparentProxyManager.connection.status`, then reconcile the local `active` flag before reporting, so removed, disabled, stale, or disconnected manager/provider state returns the setup checklist to the exact activation, configuration, enable, start, rolled-back, or repair step.

Removal is idempotent for deleted local state: when a signed helper is available, `dam network remove-network-extension --yes` can ask macOS to remove any DAM Network Extension configuration for the configured bundle identifier even if `$DAM_STATE_DIR/network/macos-network-extension/latest.json` is missing. This keeps `dam setup rescue --yes` useful after manual state cleanup.

The provider must apply DAM's routing failure policy for configured targets. In `fail_open` mode, DAM off, paused, unhealthy, unreachable, or not ready means the provider passes traffic outside DAM and reports the route as unprotected. In `fail_closed` mode, configured targets are blocked when DAM cannot verify protection. The provider polls the local proxy health while running, caches the current flow action, and uses that cached decision in `handleNewFlow` instead of issuing a synchronous `/health` check for every connection. It closes active configured flows as soon as the cached action stops being `handle`; under `fail_open`, the client's next connection bypasses DAM instead of staying pinned through an unhealthy proxy path. Named unknown/non-configured hosts, empty protected-host scopes, and DAM-owned processes continue to pass through without body inspection. Hostless TCP/443 flows whose Network Extension endpoint is only an IP address are accepted long enough to read the first TLS ClientHello. If SNI matches a configured protected host, the provider forwards the buffered TLS stream through DAM using that hostname; if SNI is absent, unrelated, or the bytes are not TLS, the provider direct-pipes the buffered stream to the original endpoint without decryption. This keeps shared-IP traffic from being over-captured while covering clients that omit `remoteHostname` on protected TLS flows. Updating the protected-host list on an already connected manager restarts the tunnel so the provider reloads the new runtime configuration.

The provider resolves `sourceAppAuditToken` directly to PID and process path, combines that with `sourceAppSigningIdentifier`, and bypasses DAM-owned traffic only when a configured DAM signing identifier is paired with a packaged DAM app path under `/Applications/DAM.app/Contents/MacOS/` or `/Applications/DAM.app/Contents/Helpers/`. Bare basename or path-only matches do not bypass interception. Packaged builds sign nested DAM executables with explicit identifiers such as `com.rpblc.dam.proxy`, `com.rpblc.dam.web`, `com.rpblc.dam.daemon`, `com.rpblc.dam.cli`, `com.rpblc.dam.mcp`, and `com.rpblc.dam.tray`. For configured flows handed to DAM, the synthesized CONNECT preface includes sanitized `X-DAM-Source-*` metadata headers for internal diagnostics and future profile decisions.

The provider system extension is an executable system extension, not an app-extension entrypoint. Its `main.swift` calls `NEProvider.startSystemExtensionMode()` and the generated Info.plist declares the `NetworkExtension:NEProviderClasses` mapping. Release builds must bump the provider `CFBundleVersion` whenever the provider binary changes so macOS replaces an already-approved extension instead of continuing to run the old build.

## Safety Boundary

This module installs routing only. The native provider forwards configured TCP flows to the local transparent proxy and closes configured UDP/443 flows only to force TCP/TLS fallback while protection is healthy. CONNECT handling, TLS interception, detection, redaction, and protocol-adapter policy remain in `dam-proxy` and the shared DAM pipeline.

Unknown/non-configured traffic routed by the PAC file or Network Extension is passed through without TLS decryption, body inspection, or redaction. Configured HTTPS traffic is protected only when daemon routing mode, local CA trust, explicit consent, and the transparent TLS adapter are all ready. When protection is paused, the configured routing failure policy decides whether matched traffic passes outside DAM (`fail_open`) or stops (`fail_closed`).

## Tests

```bash
cargo test -p dam-net-macos
swift build --package-path native/macos
```
