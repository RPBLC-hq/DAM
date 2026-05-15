# dam-mcp

`dam-mcp` is the first local MCP server for agent-managed DAM operations.

It currently exposes local install/status tools, rescue preview/apply, and consent tools over stdio:

- `dam_status`
- `dam_setup_plan`
- `dam_setup_next_action`
- `dam_setup_rescue`
- `dam_consent_list`
- `dam_consent_grant`
- `dam_consent_revoke`

`dam_consent_request` is parked until `dam-notify` exists.

## Stable Handles

Grant uses `vault_key`, not bracket display references:

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

Claude/Codex MCP config can point at the installed binary:

```json
{
  "mcpServers": {
    "dam": {
      "command": "dam-mcp",
      "args": ["--config", "dam.toml"]
    }
  }
}
```

`dam_status`, `dam_setup_plan`, and `dam_setup_next_action` are always read-only. They let an agent inspect whether DAM is running and what idempotent setup action should happen next without mutating local network or trust state.

`dam_setup_rescue` previews by default. Passing `{"apply": true, "confirm": "remove_dam_network_setup"}` stops DAM and removes DAM-managed local network routing, matching `dam setup rescue --yes`; it leaves local CA trust and vault data intact.

Consent write tools are enabled by default through:

```toml
[consent]
mcp_write_enabled = true
```

Set it to `false` to expose list-only behavior.
