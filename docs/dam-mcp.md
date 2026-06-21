# dam-mcp

`dam-mcp` is the first local MCP server for agent-managed DAM operations.

It currently exposes local install/status tools, rescue/repair preview/apply, offline diagnostics export, passthrough-consent tools, and the first bounded direct value-access slice over stdio:

- `dam_status`
- `dam_setup_plan`
- `dam_setup_next_action`
- `dam_setup_rescue`
- `dam_setup_repair`
- `dam_setup_export_diagnostics`
- `dam_consent_list`
- `dam_consent_grant`
- `dam_consent_revoke`
- `dam_consent_request`
- `dam_consent_request_status`
- `dam_resolve_if_consented`

## Stable Handles

Grant and request tools use `vault_key`, not bracket display references:

```json
{
  "vault_key": "email:ANJFsZtLfEA9WeP3bZS8Nw",
  "ttl_seconds": 3600,
  "reason": "user approved sending this support address"
}
```

This avoids friction when `[email:...]` has been resolved inbound before the agent can call MCP.

## Usage

```bash
dam-mcp --config dam.toml
dam-mcp --db vault.db --consent-db consent.db
```

Claude/ChatGPT MCP config can point at the installed binary:

```json
{
  "mcpServers": {
    "dam": {
      "command": "dam-mcp",
      "args": ["--config", "dam.toml"],
      "env": {
        "DAM_MCP_ACTOR_LABEL": "Codex"
      }
    }
  }
}
```

`dam_status`, `dam_setup_plan`, `dam_setup_next_action`, and `dam_setup_export_diagnostics` are always read-only. They let an agent inspect whether DAM is running, collect an offline doctor/setup/rescue-preview bundle, and determine what idempotent setup action should happen next without mutating local network or trust state.

`dam_setup_rescue` previews by default. Passing `{"apply": true, "confirm": "remove_dam_network_setup"}` stops DAM and removes DAM-managed local network routing, matching `dam setup rescue --yes`; it leaves local CA trust and vault data intact.

`dam_setup_repair` uses the same confirmation rule. Without `apply`, it previews local rescue and returns the current setup plan. With confirmed `apply`, it applies rescue first and returns a fresh setup plan so an autonomous installer can continue from `setup_plan.next_action`.

Consent write tools are enabled by default through:

```toml
[consent]
mcp_write_enabled = true
pending_timeout_seconds = 60
max_request_duration_seconds = 86400
```

Set `mcp_write_enabled = false` to restrict to read-only behavior: `dam_consent_list` and `dam_consent_request_status` remain available; `dam_consent_grant`, `dam_consent_revoke`, `dam_consent_request`, and `dam_resolve_if_consented` are hidden and disabled.

## Direct value-access flow

`dam_consent_request` creates a pending request bound to the local MCP actor. The caller must supply:

- `vault_key`
- `purpose`
- `duration_seconds` (minimum 30 seconds, capped by `consent.max_request_duration_seconds`)
- optional `reason`
- optional `correlation_id`

The request stays metadata-only until approved by a local control surface or API harness. The MCP server itself does **not** auto-approve requests.

`dam_consent_request_status` returns the current non-sensitive state for a `request_id` or `grant_id`.

`dam_resolve_if_consented` returns a raw value only when all of these are true:

- the request is approved and not expired;
- the current MCP actor binding matches the approved actor;
- the backing vault row is still readable; and
- the single-use grant has not already been consumed.

Otherwise it returns request metadata plus a stable denial/expiry reason and no raw value.

`dam_consent_revoke` now accepts passthrough `consent_id` values and direct-access `request_id`/`grant_id` values.
