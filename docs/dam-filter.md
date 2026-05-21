# dam-filter

`dam-filter` is the current CLI pipeline.

It is the integration point for the local rebuild slice.

## Pipeline

```text
input
  -> dam-detect
  -> dam-policy
  -> dam-consent active canonical-value overrides
  -> dam-core replacement plan
  -> dam-vault for tokenize decisions
  -> dam-redact
  -> stdout

dam-core log events
  -> dam-log when enabled
```

## Usage

```bash
echo "email alice@example.com" \
  | cargo run -p dam-filter -- --config dam.example.toml --report
```

CLI overrides:

```text
--config <path>
--db <path>
--log <path>
--no-log
--report
--json-report
[FILE]
```

## Report Output

`--report` writes local diagnostic metadata to stderr:

```text
operation_id: ...
detections: 3
stored: 1
policy_redactions: 1
allowed: 1
blocked: 0
fallback_redactions: 0
```

Persisted Activity logs include detected values so the Activity UI can show what was detected independently from Wallet. Vault write failures use the stable `vault_write_failed` code instead of backend error text.

`--json-report` writes the standardized `dam-api` filter report JSON to stderr. Stdout remains the filtered payload. If both `--report` and `--json-report` are provided, JSON wins.

## Policy Behavior

- `tokenize`: vault write and token replacement.
- `redact`: no vault write and `[kind]` replacement.
- `allow`: no vault write and original text remains.
- `block`: no vault write, no stdout, non-zero exit.

Repeated equal values reuse one tokenized reference by default, and compatible vault writers reuse an existing canonical reference for the same stored value. Disable that with `policy.deduplicate_replacements = false` or `DAM_POLICY_DEDUPLICATE_REPLACEMENTS=false` when the repeated-reference equality signal is too revealing.

Active consent grants let canonical detected values pass through unredacted until expiry or revocation. Consent overrides `tokenize` and `redact`; it does not override `block`.

## Tests

```bash
cargo test -p dam-filter
```
