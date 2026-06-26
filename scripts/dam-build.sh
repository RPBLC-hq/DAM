#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${DAM_BUILD_OUT:-$ROOT/target/dam-build}"
MACOS_OUT="${DAM_MACOS_OUT:-$OUT_DIR/macos}"
SIGN_MODE="${DAM_SIGN_MODE:-developer-id}"
NOTARY_PROFILE="${DAM_NOTARY_PROFILE:-DAM-notary}"
INSTALL_DIR="${DAM_INSTALL_DIR:-/Applications}"
SKIP_NOTARIZE="${DAM_SKIP_NOTARIZE:-0}"
RESTART_AFTER_INSTALL="${DAM_RESTART_AFTER_INSTALL:-1}"
AGENT_STATUS_STRICT="${DAM_AGENT_STATUS_STRICT:-0}"
AGENT_NETWORK_MODE="${DAM_AGENT_NETWORK_MODE:-tun}"
AGENT_TRUST_MODE="${DAM_AGENT_TRUST_MODE:-local_ca}"
AGENT_STATE_DIR="${DAM_AGENT_STATE_DIR:-}"
AGENT_E2E_UPSTREAM="${DAM_AGENT_E2E_UPSTREAM:-http://127.0.0.1:8080}"
AGENT_E2E_LISTEN="${DAM_AGENT_E2E_LISTEN:-127.0.0.1:7831}"
AGENT_E2E_STARTUP_TIMEOUT="${DAM_AGENT_E2E_STARTUP_TIMEOUT:-30}"
AGENT_E2E_HTTP_TIMEOUT="${DAM_AGENT_E2E_HTTP_TIMEOUT:-60}"
AGENT_E2E_WEB_ADDR="${DAM_AGENT_E2E_WEB_ADDR:-}"
AGENT_E2E_SMOKE_SCRIPT="${DAM_AGENT_E2E_SMOKE_SCRIPT:-$ROOT/scripts/rpblc_dam_local_llm_e2e_smoke.py}"
AGENT_E2E_VERIFY_SCRIPT="${DAM_AGENT_E2E_VERIFY_SCRIPT:-$ROOT/scripts/dam_vps_dogfood_verify.py}"
AGENT_E2E_BINARY="${DAM_AGENT_E2E_BINARY:-$ROOT/target/debug/dam-proxy}"
AGENT_E2E_WEB_BINARY="${DAM_AGENT_E2E_WEB_BINARY:-$ROOT/target/debug/dam-web}"
AGENT_E2E_BUILD="${DAM_AGENT_E2E_BUILD:-1}"
AGENT_E2E_KEEP_TEMP="${DAM_AGENT_E2E_KEEP_TEMP:-0}"
AGENT_CONFIRM_MUTATION="${DAM_AGENT_CONFIRM_MUTATION:-0}"
MACOS_NE_BUNDLE_ID="${DAM_MACOS_NE_BUNDLE_ID:-com.rpblc.dam.network-extension}"

usage() {
  cat <<EOF
Usage: scripts/dam-build.sh <command> [options]

Commands:
  check          Run the standard local/CI verification suite
  dev           Build source-tree debug binaries used by local DAM runs
  npm-native    Build and stage current-platform binaries under npm/native
  macos-app     Build signed DAM.app through native/macos packaging
  notarize      Notarize and staple an existing DAM.app
  release-macos Run checks, build signed DAM.app, notarize, staple, and zip it
  deploy-local  Build signed DAM.app and copy it to /Applications or --install-dir
  agent-check   Run the standard verification suite plus repo whitespace checks
  agent-npm-readiness
               Stage npm native binaries, validate the package payload, and report publish blockers
  detector-bench
                Run the synthetic DAM detector benchmark harness
  agent-protection-smoke
                Run local API-through-DAM protection smoke against local upstream
  agent-websocket-smoke
                Run synthetic ChatGPT WebSocket route protection smoke against loopback upstream
  agent-dogfood-verify
                Run proxy, Activity, and pending-consent VPS dogfooding verification
  agent-recovery-smoke
                Run read-only installed rescue/repair/diagnostics recovery probes
  agent-repair-smoke
                Run mutating installed rescue/repair recovery probes with confirmation
  agent-install Build, notarize when enabled, install, verify, restart, and status DAM
  agent-status  Print installed app, process, package, doctor, and setup status

Options:
  --mode development|developer-id  Signing mode for macOS app packaging
  --out DIR                        Build artifact output directory
  --app PATH                       Existing DAM.app for notarize/deploy-local
  --notary-profile NAME            notarytool keychain profile name
  --install-dir DIR                Destination for deploy-local
  --skip-checks                    Skip check phase in release-macos
  --skip-notarize                  Skip notarization in agent-install
  --require-notary                 Fail agent-install unless notarization runs
  --restart                        Restart daemon/tray after agent-install
  --no-restart                     Do not restart daemon/tray after agent-install
  --strict-status                  Make agent-status fail when a status probe fails
  --network-mode MODE              Setup mode used by agent-status probes
  --trust-mode MODE                Trust mode used by agent-status probes
  --state-dir DIR                  State directory used by setup/doctor probes
  --confirm-mutation               Allow mutating agent repair smoke probes
  -h, --help                       Show this help

Environment:
  DAM_BUILD_OUT             Default artifact root, currently target/dam-build
  DAM_SIGN_MODE             development or developer-id, currently developer-id
  DAM_NOTARY_PROFILE        notarytool keychain profile, currently DAM-notary
  DAM_MACOS_TEAM_ID         Optional Team ID override for macOS packaging
  DAM_MACOS_APP_GROUP_ID    Optional App Group override for macOS packaging
  DAM_MACOS_NE_BUNDLE_ID    Network Extension bundle ID, currently $MACOS_NE_BUNDLE_ID
  DAM_INSTALL_DIR           Local install destination, currently /Applications
  DAM_SKIP_NOTARIZE         Set to 1 to skip notarization in agent-install
  DAM_RESTART_AFTER_INSTALL Set to 0 to skip daemon/tray restart in agent-install
  DAM_AGENT_STATUS_STRICT   Set to 1 to make agent-status fail on probe errors
  DAM_AGENT_NETWORK_MODE    Setup mode for agent-status, currently tun
  DAM_AGENT_TRUST_MODE      Trust mode for agent-status, currently local_ca
  DAM_AGENT_STATE_DIR       Optional state directory for setup/doctor probes
  DAM_AGENT_E2E_UPSTREAM    Local OpenAI-compatible smoke upstream, currently $AGENT_E2E_UPSTREAM
  DAM_AGENT_E2E_LISTEN      Loopback listen address for smoke proxy, currently $AGENT_E2E_LISTEN
  DAM_AGENT_E2E_WEB_ADDR    Loopback listen address for dogfood web proof, currently $AGENT_E2E_WEB_ADDR
  DAM_AGENT_E2E_STARTUP_TIMEOUT
                           Smoke proxy startup timeout, currently $AGENT_E2E_STARTUP_TIMEOUT
  DAM_AGENT_E2E_HTTP_TIMEOUT
                           Smoke request timeout, currently $AGENT_E2E_HTTP_TIMEOUT
  DAM_AGENT_E2E_SMOKE_SCRIPT
                           Smoke verifier script, currently $AGENT_E2E_SMOKE_SCRIPT
  DAM_AGENT_E2E_VERIFY_SCRIPT
                           VPS dogfood verifier script, currently $AGENT_E2E_VERIFY_SCRIPT
  DAM_AGENT_E2E_BINARY     dam-proxy binary path for smoke, currently $AGENT_E2E_BINARY
  DAM_AGENT_E2E_WEB_BINARY dam-web binary path for dogfood verify, currently $AGENT_E2E_WEB_BINARY
  DAM_AGENT_E2E_BUILD      Set to 0 to reuse the binary without cargo build, currently $AGENT_E2E_BUILD
  DAM_AGENT_E2E_KEEP_TEMP  Set to 1 to keep smoke temp vault/log files, currently $AGENT_E2E_KEEP_TEMP
  DAM_AGENT_CONFIRM_MUTATION
                           Set to 1 to allow mutating agent repair smoke probes
EOF
}

run() {
  printf '+'
  printf ' %q' "$@"
  printf '\n'
  "$@"
}

allocate_loopback_addr() {
  python3 - <<'PY'
import socket
with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
    sock.bind(("127.0.0.1", 0))
    host, port = sock.getsockname()
print(f"{host}:{port}")
PY
}

require_macos() {
  if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "macOS packaging/notarization requires Darwin" >&2
    exit 1
  fi
}

dam_app_path() {
  printf '%s/DAM.app\n' "$MACOS_OUT"
}

installed_app_path() {
  printf '%s/DAM.app\n' "$INSTALL_DIR"
}

zip_path_for_app() {
  local app="$1"
  local base
  base="$(basename "$app" .app)"
  printf '%s/%s-notary.zip\n' "$(dirname "$app")" "$base"
}

should_notarize() {
  [[ "$SIGN_MODE" == "developer-id" && "$SKIP_NOTARIZE" != "1" ]]
}

process_pids_for_path() {
  local path="$1"
  pgrep -f "$path" 2>/dev/null || true
}

stop_processes_for_path() {
  local label="$1"
  local path="$2"
  local pids
  pids="$(process_pids_for_path "$path")"
  if [[ -z "$pids" ]]; then
    return 0
  fi

  printf 'Stopping %s: %s\n' "$label" "$pids"
  kill $pids 2>/dev/null || true
  for _ in {1..20}; do
    pids="$(process_pids_for_path "$path")"
    if [[ -z "$pids" ]]; then
      return 0
    fi
    sleep 0.2
  done

  pids="$(process_pids_for_path "$path")"
  if [[ -n "$pids" ]]; then
    printf 'Force stopping %s: %s\n' "$label" "$pids"
    kill -9 $pids 2>/dev/null || true
  fi
}

stop_installed_ui() {
  local app="$1"
  stop_processes_for_path "dam-tray" "$app/Contents/MacOS/dam-tray"
  stop_processes_for_path "dam-web" "$app/Contents/MacOS/dam-web"
}

verify_installed_app() {
  require_macos
  local app="$1"
  if [[ ! -d "$app" ]]; then
    echo "missing installed app bundle: $app" >&2
    exit 1
  fi
  local dam_bin="$app/Contents/MacOS/dam"
  if [[ ! -x "$dam_bin" ]]; then
    echo "missing installed dam binary: $dam_bin" >&2
    exit 1
  fi

  run codesign --verify --deep --strict --verbose=2 "$app"
  if should_notarize; then
    run xcrun stapler validate "$app"
    run spctl -a -vvv -t exec "$app"
  else
    printf 'Skipped notarization ticket and Gatekeeper validation for %s mode.\n' "$SIGN_MODE"
  fi
}

restart_installed_app() {
  require_macos
  local app="$1"
  local dam_bin="$app/Contents/MacOS/dam"
  if [[ ! -x "$dam_bin" ]]; then
    echo "missing installed dam binary: $dam_bin" >&2
    exit 1
  fi
  run "$dam_bin" disconnect --stop --json
  run "$dam_bin" connect --network-mode "$AGENT_NETWORK_MODE" --trust-mode "$AGENT_TRUST_MODE" --json
  run open "$app"
}

refresh_installed_system_extension() {
  require_macos
  local app="$1"
  if [[ "$AGENT_NETWORK_MODE" != "tun" ]]; then
    return 0
  fi
  local tray_bin="$app/Contents/MacOS/dam-tray"
  local dam_bin="$app/Contents/MacOS/dam"
  if [[ ! -x "$tray_bin" || ! -x "$dam_bin" ]]; then
    echo "missing installed DAM tray or CLI binary under: $app" >&2
    exit 1
  fi
  run "$tray_bin" --activate-system-extension "$MACOS_NE_BUNDLE_ID"
  run "$dam_bin" network install-network-extension --yes --json
}

status_try() {
  local failures_name="$1"
  shift
  printf '+'
  printf ' %q' "$@"
  printf '\n'
  if "$@"; then
    return 0
  else
    local status=$?
    printf 'status probe failed (%s):' "$status"
    printf ' %q' "$@"
    printf '\n'
    printf -v "$failures_name" '%s' "$(( ${!failures_name} + 1 ))"
  fi
  return 0
}

cmd_check() {
  if [[ -f "$ROOT/crates/dam-web/ui/package.json" ]]; then
    run npm ci --prefix "$ROOT/crates/dam-web/ui"
    run npm run build --prefix "$ROOT/crates/dam-web/ui"
  fi
  run npm test --prefix "$ROOT"
  run npm pack --dry-run --ignore-scripts "$ROOT"
  run cargo fmt --all --check
  run cargo clippy --workspace -- -D warnings
  run cargo test --workspace
  if [[ -f "$ROOT/native/macos/Package.swift" && "$(uname -s)" == "Darwin" ]]; then
    run swift test --package-path "$ROOT/native/macos"
  fi
}

cmd_agent_check() {
  cmd_check
  if [[ -d "$ROOT/.git" ]]; then
    run git -C "$ROOT" diff --check
  else
    printf 'Skipped git diff --check because %s is not a git checkout.\n' "$ROOT"
  fi
}

package_manifest_field() {
  local field="$1"
  node -e "const fs = require('fs'); const manifest = JSON.parse(fs.readFileSync(process.argv[1], 'utf8')); const value = manifest[process.argv[2]]; if (value === undefined) process.exit(1); if (typeof value === 'string') { process.stdout.write(value); } else { process.stdout.write(JSON.stringify(value)); }" "$ROOT/package.json" "$field"
}

check_semver_greater() {
  python3 - "$1" "$2" <<'PY'
import sys


def parse(version: str) -> tuple[list[int], str]:
    normalized = version.strip().lstrip("v")
    core, _, prerelease = normalized.partition("-")
    parts = [int(part) if part.isdigit() else 0 for part in core.split(".") if part]
    return parts, prerelease


local_parts, local_pre = parse(sys.argv[1])
remote_parts, remote_pre = parse(sys.argv[2])
width = max(len(local_parts), len(remote_parts), 3)
local_parts.extend([0] * (width - len(local_parts)))
remote_parts.extend([0] * (width - len(remote_parts)))

if local_parts > remote_parts:
    raise SystemExit(0)
if local_parts < remote_parts:
    raise SystemExit(1)
if local_pre and not remote_pre:
    raise SystemExit(1)
if not local_pre and remote_pre:
    raise SystemExit(0)
if local_pre > remote_pre:
    raise SystemExit(0)
raise SystemExit(1)
PY
}

verify_npm_pack_payload() {
  local platform_dir="$1"
  python3 -c 'import json, sys
platform_dir = sys.argv[1]
payload = json.load(sys.stdin)
entries = payload if isinstance(payload, list) else [payload]
if not entries:
    print("npm pack output was empty", file=sys.stderr)
    raise SystemExit(1)
files = {entry.get("path") for entry in entries[0].get("files", [])}
expected = [
    f"npm/native/{platform_dir}/dam",
    f"npm/native/{platform_dir}/damctl",
    f"npm/native/{platform_dir}/dam-web",
    f"npm/native/{platform_dir}/dam-proxy",
    f"npm/native/{platform_dir}/dam-mcp",
    f"npm/native/{platform_dir}/dam-tray",
]
missing = [path for path in expected if path not in files]
if missing:
    print("missing staged native package files:", file=sys.stderr)
    for path in missing:
        print(path, file=sys.stderr)
    raise SystemExit(1)
print(entries[0].get("filename", ""))' "$platform_dir"
}

cmd_agent_npm_readiness() {
  local package_name local_version registry_url platform_dir doctor_output pack_output pack_filename
  local registry_version owners whoami_output blockers=()

  cmd_npm_native

  package_name="$(package_manifest_field name)"
  local_version="$(package_manifest_field version)"
  registry_url="$(npm config get registry)"
  platform_dir="$(node -p "process.platform + '-' + process.arch")"

  if ! doctor_output="$(node "$ROOT/npm/bin/dam.js" package-doctor --json)"; then
    printf '%s\n' "$doctor_output"
    echo "npm package-doctor failed" >&2
    exit 1
  fi

  if ! pack_output="$(npm pack --dry-run --ignore-scripts --json)"; then
    printf '%s\n' "$pack_output"
    echo "npm pack --dry-run failed" >&2
    exit 1
  fi

  if ! pack_filename="$(printf '%s' "$pack_output" | verify_npm_pack_payload "$platform_dir")"; then
    blockers+=("npm pack payload is missing one or more staged native binaries for $platform_dir")
  fi

  registry_version="unknown"
  if registry_version="$(npm view "$package_name" version --json 2>/dev/null | python3 -c 'import json, sys
payload = json.load(sys.stdin)
if isinstance(payload, str):
    print(payload)
else:
    print(json.dumps(payload))')"; then
    if [[ -n "$registry_version" ]] && ! check_semver_greater "$local_version" "$registry_version"; then
      blockers+=("local package version $local_version is not greater than published npm version $registry_version")
    fi
  else
    registry_version="unavailable"
    blockers+=("unable to read current npm registry version for $package_name")
  fi

  owners="$(npm owner ls "$package_name" 2>/dev/null || true)"
  if [[ -z "$owners" ]]; then
    owners="unavailable"
  fi

  if whoami_output="$(npm whoami 2>&1)"; then
    :
  else
    whoami_output="missing"
    blockers+=("npm publish auth is not configured on this machine; run npm adduser or configure a publish token for $package_name")
  fi

  printf 'DAM agent npm readiness\n'
  printf 'package: %s\n' "$package_name"
  printf 'local_version: %s\n' "$local_version"
  printf 'registry: %s\n' "$registry_url"
  printf 'registry_version: %s\n' "$registry_version"
  printf 'platform_dir: %s\n' "$platform_dir"
  printf 'package_doctor_state: ready\n'
  printf 'pack_file: %s\n' "$pack_filename"
  printf 'pack_native_files_present: %s\n' "$( [[ -n "$pack_filename" ]] && printf yes || printf no )"
  printf 'npm_owners: %s\n' "$owners"
  printf 'npm_auth: %s\n' "$whoami_output"

  if (( ${#blockers[@]} == 0 )); then
    printf 'blockers: none\n'
    return 0
  fi

  printf 'blockers:\n'
  local blocker
  for blocker in "${blockers[@]}"; do
    printf '  - %s\n' "$blocker"
  done
  return 1
}

cmd_detector_bench() {
  run cargo run -q -p dam-detect-bench --
}

cmd_agent_protection_smoke() {
  if [[ ! -f "$AGENT_E2E_SMOKE_SCRIPT" ]]; then
    echo "missing local protection smoke script: $AGENT_E2E_SMOKE_SCRIPT" >&2
    exit 1
  fi
  local smoke_args=(
    "$AGENT_E2E_SMOKE_SCRIPT"
    --upstream "$AGENT_E2E_UPSTREAM"
    --listen "$AGENT_E2E_LISTEN"
    --startup-timeout "$AGENT_E2E_STARTUP_TIMEOUT"
    --http-timeout "$AGENT_E2E_HTTP_TIMEOUT"
    --binary "$AGENT_E2E_BINARY"
  )
  if [[ "$AGENT_E2E_BUILD" == "0" ]]; then
    smoke_args+=(--no-build)
  fi
  if [[ "$AGENT_E2E_KEEP_TEMP" == "1" ]]; then
    smoke_args+=(--keep-temp)
  fi
  run python3 "${smoke_args[@]}"
}

cmd_agent_websocket_smoke() {
  run cargo test -q -p dam-proxy transparent_chatgpt_websocket_route_protects_outbound_text_frames -- --nocapture
}

cmd_agent_dogfood_verify() {
  if [[ ! -f "$AGENT_E2E_VERIFY_SCRIPT" ]]; then
    echo "missing VPS dogfood verifier script: $AGENT_E2E_VERIFY_SCRIPT" >&2
    exit 1
  fi
  local web_addr="$AGENT_E2E_WEB_ADDR"
  if [[ -z "$web_addr" ]]; then
    web_addr="$(allocate_loopback_addr)"
  fi
  local verify_args=(
    "$AGENT_E2E_VERIFY_SCRIPT"
    verify
    --upstream "$AGENT_E2E_UPSTREAM"
    --listen "$AGENT_E2E_LISTEN"
    --web-addr "$web_addr"
    --proxy-binary "$AGENT_E2E_BINARY"
    --web-binary "$AGENT_E2E_WEB_BINARY"
    --startup-timeout "$AGENT_E2E_STARTUP_TIMEOUT"
    --http-timeout "$AGENT_E2E_HTTP_TIMEOUT"
  )
  if [[ -n "$AGENT_STATE_DIR" ]]; then
    verify_args+=(--state-dir "$AGENT_STATE_DIR")
  fi
  if [[ "$AGENT_E2E_BUILD" == "0" ]]; then
    verify_args+=(--no-build)
  fi
  if [[ "$AGENT_E2E_KEEP_TEMP" == "1" ]]; then
    verify_args+=(--keep-state)
  fi
  run python3 "${verify_args[@]}"
}

cmd_dev() {
  run cargo build -p dam -p damctl -p dam-web -p dam-tray
}

cmd_npm_native() {
  run cargo build --release -p dam -p damctl -p dam-web -p dam-proxy -p dam-mcp -p dam-tray
  local platform_dir exe_suffix native_out bin source
  platform_dir="$(node -p "process.platform + '-' + process.arch")"
  exe_suffix=""
  if [[ "$(uname -s)" == MINGW* || "$(uname -s)" == MSYS* || "$(uname -s)" == CYGWIN* ]]; then
    exe_suffix=".exe"
  fi
  native_out="$ROOT/npm/native/$platform_dir"
  mkdir -p "$native_out"
  for bin in dam damctl dam-web dam-proxy dam-mcp dam-tray; do
    source="$ROOT/target/release/$bin$exe_suffix"
    if [[ ! -f "$source" ]]; then
      echo "missing release binary: $source" >&2
      exit 1
    fi
    run cp "$source" "$native_out/$bin$exe_suffix"
    chmod 755 "$native_out/$bin$exe_suffix"
  done
  printf 'Staged npm native binaries: %s\n' "$native_out"
}

cmd_macos_app() {
  require_macos
  run "$ROOT/native/macos/scripts/package-dam-app.sh" --mode "$SIGN_MODE" --out "$MACOS_OUT"
}

cmd_notarize() {
  require_macos
  local app="${APP_PATH:-$(dam_app_path)}"
  if [[ ! -d "$app" ]]; then
    echo "missing app bundle: $app" >&2
    exit 1
  fi
  local zip
  zip="$(zip_path_for_app "$app")"
  rm -f "$zip"
  run ditto -c -k --keepParent "$app" "$zip"
  run xcrun notarytool submit "$zip" --keychain-profile "$NOTARY_PROFILE" --wait
  run xcrun stapler staple "$app"
  run xcrun stapler validate "$app"
  printf 'Notarized app: %s\n' "$app"
  printf 'Notary zip: %s\n' "$zip"
}

cmd_release_macos() {
  require_macos
  if [[ "${SKIP_CHECKS:-0}" != "1" ]]; then
    cmd_check
  fi
  cmd_macos_app
  APP_PATH="$(dam_app_path)" cmd_notarize
  local app release_zip
  app="$(dam_app_path)"
  release_zip="$MACOS_OUT/DAM-macos-${SIGN_MODE}.zip"
  rm -f "$release_zip"
  run ditto -c -k --keepParent "$app" "$release_zip"
  printf 'Release app: %s\n' "$app"
  printf 'Release zip: %s\n' "$release_zip"
}

cmd_deploy_local() {
  require_macos
  local app="${APP_PATH:-}"
  if [[ -z "$app" ]]; then
    cmd_macos_app
    app="$(dam_app_path)"
  fi
  if [[ ! -d "$app" ]]; then
    echo "missing app bundle: $app" >&2
    exit 1
  fi
  local destination="$INSTALL_DIR/DAM.app"
  rm -rf "$destination"
  run ditto "$app" "$destination"
  printf 'Installed local app: %s\n' "$destination"
}

cmd_agent_status() {
  require_macos
  local app
  app="$(installed_app_path)"
  local failures=0

  printf 'DAM agent status\n'
  printf 'install_dir: %s\n' "$INSTALL_DIR"
  printf 'app: %s\n' "$app"
  printf 'setup_probe_network_mode: %s\n' "$AGENT_NETWORK_MODE"
  printf 'setup_probe_trust_mode: %s\n' "$AGENT_TRUST_MODE"
  if [[ -n "$AGENT_STATE_DIR" ]]; then
    printf 'setup_probe_state_dir: %s\n' "$AGENT_STATE_DIR"
  fi
  if [[ ! -d "$app" ]]; then
    echo "installed app not found"
    exit 1
  fi

  local dam_bin="$app/Contents/MacOS/dam"
  if [[ ! -x "$dam_bin" ]]; then
    echo "installed dam binary not found: $dam_bin" >&2
    exit 1
  fi

  printf 'processes:\n'
  pgrep -fl "$app/Contents/MacOS/dam" || true

  status_try failures codesign --verify --deep --strict --verbose=2 "$app"
  if should_notarize; then
    status_try failures xcrun stapler validate "$app"
    status_try failures spctl -a -vvv -t exec "$app"
  fi
  local state_args=()
  if [[ -n "$AGENT_STATE_DIR" ]]; then
    state_args=(--state-dir "$AGENT_STATE_DIR")
  fi
  status_try failures "$dam_bin" doctor --network-mode "$AGENT_NETWORK_MODE" --trust-mode "$AGENT_TRUST_MODE" "${state_args[@]}" --json
  status_try failures "$dam_bin" setup status --network-mode "$AGENT_NETWORK_MODE" --trust-mode "$AGENT_TRUST_MODE" "${state_args[@]}" --json
  status_try failures "$dam_bin" setup next-action --network-mode "$AGENT_NETWORK_MODE" --trust-mode "$AGENT_TRUST_MODE" "${state_args[@]}" --json
  status_try failures "$dam_bin" setup export-diagnostics --network-mode "$AGENT_NETWORK_MODE" --trust-mode "$AGENT_TRUST_MODE" "${state_args[@]}" --json
  status_try failures "$dam_bin" status --json

  printf 'status_probe_failures: %s\n' "$failures"
  if [[ "$AGENT_STATUS_STRICT" == "1" && "$failures" != "0" ]]; then
    exit 1
  fi
}

installed_dam_binary() {
  local app dam_bin
  app="$(installed_app_path)"
  if [[ ! -d "$app" ]]; then
    echo "installed app not found" >&2
    exit 1
  fi

  dam_bin="$app/Contents/MacOS/dam"
  if [[ ! -x "$dam_bin" ]]; then
    echo "installed dam binary not found: $dam_bin" >&2
    exit 1
  fi

  printf '%s\n' "$dam_bin"
}

print_agent_setup_probe_header() {
  local name app
  name="$1"
  app="$(installed_app_path)"
  printf 'DAM %s\n' "$name"
  printf 'install_dir: %s\n' "$INSTALL_DIR"
  printf 'app: %s\n' "$app"
  printf 'setup_probe_network_mode: %s\n' "$AGENT_NETWORK_MODE"
  printf 'setup_probe_trust_mode: %s\n' "$AGENT_TRUST_MODE"
  if [[ -n "$AGENT_STATE_DIR" ]]; then
    printf 'setup_probe_state_dir: %s\n' "$AGENT_STATE_DIR"
  fi
}

cmd_agent_recovery_smoke() {
  require_macos
  local dam_bin
  print_agent_setup_probe_header "agent recovery smoke"
  dam_bin="$(installed_dam_binary)"

  local state_args=()
  if [[ -n "$AGENT_STATE_DIR" ]]; then
    state_args=(--state-dir "$AGENT_STATE_DIR")
  fi
  run "$dam_bin" setup rescue --dry-run "${state_args[@]}" --json
  run "$dam_bin" setup repair --dry-run --network-mode "$AGENT_NETWORK_MODE" --trust-mode "$AGENT_TRUST_MODE" "${state_args[@]}" --json
  run "$dam_bin" setup export-diagnostics --network-mode "$AGENT_NETWORK_MODE" --trust-mode "$AGENT_TRUST_MODE" "${state_args[@]}" --json
}

cmd_agent_repair_smoke() {
  if [[ "$AGENT_CONFIRM_MUTATION" != "1" ]]; then
    echo "agent-repair-smoke mutates installed DAM setup; pass --confirm-mutation or set DAM_AGENT_CONFIRM_MUTATION=1" >&2
    exit 2
  fi
  require_macos

  local dam_bin
  print_agent_setup_probe_header "agent repair smoke"
  dam_bin="$(installed_dam_binary)"

  local state_args=()
  if [[ -n "$AGENT_STATE_DIR" ]]; then
    state_args=(--state-dir "$AGENT_STATE_DIR")
  fi
  run "$dam_bin" setup rescue --yes "${state_args[@]}" --json
  run "$dam_bin" setup repair --yes --network-mode "$AGENT_NETWORK_MODE" --trust-mode "$AGENT_TRUST_MODE" "${state_args[@]}" --json
  run "$dam_bin" setup status --network-mode "$AGENT_NETWORK_MODE" --trust-mode "$AGENT_TRUST_MODE" "${state_args[@]}" --json
}

cmd_agent_install() {
  require_macos
  if [[ "${SKIP_CHECKS:-0}" != "1" ]]; then
    cmd_agent_check
  fi

  cmd_macos_app
  local app
  app="$(dam_app_path)"
  if should_notarize; then
    APP_PATH="$app" cmd_notarize
  elif [[ "${REQUIRE_NOTARY:-0}" == "1" ]]; then
    echo "--require-notary requires --mode developer-id without --skip-notarize" >&2
    exit 2
  else
    printf 'Skipped notarization for %s because signing mode is %s or notarization is disabled.\n' "$app" "$SIGN_MODE"
  fi

  local destination
  destination="$(installed_app_path)"
  stop_installed_ui "$destination"
  APP_PATH="$app" cmd_deploy_local
  verify_installed_app "$destination"
  refresh_installed_system_extension "$destination"

  if [[ "$RESTART_AFTER_INSTALL" == "1" ]]; then
    restart_installed_app "$destination"
  else
    printf 'Skipped restart after install.\n'
  fi

  cmd_agent_status
}

COMMAND="${1:-}"
if [[ -z "$COMMAND" || "$COMMAND" == "-h" || "$COMMAND" == "--help" ]]; then
  usage
  exit 0
fi
shift

APP_PATH=""
SKIP_CHECKS=0
REQUIRE_NOTARY=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --mode)
      SIGN_MODE="${2:?--mode requires development or developer-id}"
      shift 2
      ;;
    --out)
      OUT_DIR="${2:?--out requires a directory}"
      MACOS_OUT="$OUT_DIR/macos"
      shift 2
      ;;
    --app)
      APP_PATH="${2:?--app requires a path}"
      shift 2
      ;;
    --notary-profile)
      NOTARY_PROFILE="${2:?--notary-profile requires a name}"
      shift 2
      ;;
    --install-dir)
      INSTALL_DIR="${2:?--install-dir requires a directory}"
      shift 2
      ;;
    --skip-checks)
      SKIP_CHECKS=1
      shift
      ;;
    --skip-notarize)
      SKIP_NOTARIZE=1
      shift
      ;;
    --require-notary)
      REQUIRE_NOTARY=1
      shift
      ;;
    --restart)
      RESTART_AFTER_INSTALL=1
      shift
      ;;
    --no-restart)
      RESTART_AFTER_INSTALL=0
      shift
      ;;
    --strict-status)
      AGENT_STATUS_STRICT=1
      shift
      ;;
    --network-mode)
      AGENT_NETWORK_MODE="${2:?--network-mode requires a value}"
      shift 2
      ;;
    --trust-mode)
      AGENT_TRUST_MODE="${2:?--trust-mode requires a value}"
      shift 2
      ;;
    --state-dir)
      AGENT_STATE_DIR="${2:?--state-dir requires a directory}"
      shift 2
      ;;
    --confirm-mutation)
      AGENT_CONFIRM_MUTATION=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

case "$SIGN_MODE" in
  development|developer-id) ;;
  *)
    echo "invalid signing mode: $SIGN_MODE" >&2
    exit 2
    ;;
esac

mode_errors=0
case "$AGENT_NETWORK_MODE" in
  explicit_proxy|system_proxy|tun) ;;
  *)
    echo "invalid agent network mode: $AGENT_NETWORK_MODE (expected explicit_proxy, system_proxy, or tun)" >&2
    mode_errors=1
    ;;
esac
case "$AGENT_TRUST_MODE" in
  disabled|local_ca) ;;
  *)
    echo "invalid agent trust mode: $AGENT_TRUST_MODE (expected disabled or local_ca)" >&2
    mode_errors=1
    ;;
esac
if [[ "$mode_errors" != "0" ]]; then
  exit 2
fi

mkdir -p "$OUT_DIR" "$MACOS_OUT"

case "$COMMAND" in
  check) cmd_check ;;
  dev) cmd_dev ;;
  npm-native) cmd_npm_native ;;
  macos-app) cmd_macos_app ;;
  notarize) cmd_notarize ;;
  release-macos) cmd_release_macos ;;
  deploy-local) cmd_deploy_local ;;
  agent-check) cmd_agent_check ;;
  agent-npm-readiness) cmd_agent_npm_readiness ;;
  detector-bench) cmd_detector_bench ;;
  agent-protection-smoke) cmd_agent_protection_smoke ;;
  agent-websocket-smoke) cmd_agent_websocket_smoke ;;
  agent-dogfood-verify) cmd_agent_dogfood_verify ;;
  agent-recovery-smoke) cmd_agent_recovery_smoke ;;
  agent-repair-smoke) cmd_agent_repair_smoke ;;
  agent-install) cmd_agent_install ;;
  agent-status) cmd_agent_status ;;
  *)
    echo "unknown command: $COMMAND" >&2
    usage >&2
    exit 2
    ;;
esac
