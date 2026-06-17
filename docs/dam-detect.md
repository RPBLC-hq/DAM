# dam-detect

`dam-detect` is a pure detection module.

It receives text and returns sensitive spans. It does not redact, write to vault, log, or decide policy.

## Current Coverage

- Email, including whitespace around separators such as `alice@ example.com` or `alice @example.com`.
- Caller-supplied related domains, for context-aware response protection only.
- NANP phone numbers in dashed form, e.g. `415-555-2671`.
- SSN with basic area validation.
- Credit card numbers with Luhn validation.
- API-key/secret assignments using common key names such as `*_API_KEY`, `secret_key`, `secret_access_key`, `client_secret`, `webhook_secret`, `signing_secret`, or `access_token`; the detected span is the assigned secret value, not the variable name. Labeled assignment values may contain common base64 secret characters such as `+`, `/`, and `=`.
- Direct high-value secret families without assignment labels for the current MVP slice: OpenAI `sk-...` / `sk-proj-...` / `sk-svcacct-...`, Anthropic `sk-ant-...`, GitHub `ghp_` / `gho_` / `ghu_` / `ghs_` / `ghr_`, Stripe API keys (`sk_live_...` / `sk_test_...` / `rk_live_...` / `rk_test_...`) and webhook signing secrets (`whsec_...`), Slack app tokens beginning with `xoxb-` / `xoxa-` / `xoxp-` / `xoxr-` / `xoxs-`, Slack, Discord, and Microsoft Teams incoming webhook URLs, Google API keys beginning with `AIza`, SendGrid API keys beginning with `SG.`, Mailgun API keys beginning with `key-` followed by 32 lowercase alphanumeric characters, AWS access key IDs beginning with `AKIA` or `ASIA`, PEM private key blocks, database connection URLs with embedded passwords for common database schemes, and `Bearer`-labeled JWTs.

Domain-only values are not detected by the default `detect()` path. `detect_with_related_domains()` can emit exact `domain` detections for domains supplied by the caller, such as domains learned from outbound email detections, while still avoiding the same domain inside an email address or subdomain. Email addresses are still detected as whole values, but their domains are not emitted as separate `domain` detections unless passed back as related context.

Known current limitation: formats like `+1 (415) 555-2671`, zero-width-character obfuscation, and still-parked secret families such as unlabeled bearer tokens, webhook providers beyond Slack/Discord/Microsoft Teams, database URLs without embedded passwords, and other provider-specific formats not yet covered by the current regex set.

## Output

The module returns `Vec<Detection>`:

```rust
Detection {
    kind,
    span,
    value,
}
```

The raw `value` is required downstream for tokenization, but it must not be persisted outside the vault.

## Architecture Rules

- Detection modules only emit candidates.
- No vault calls.
- No redaction.
- No policy decisions.
- No persistent logging.

Future multiple-detector orchestration should happen through the spine/pipeline, not inside individual detectors.

## Tests

```bash
cargo test -p dam-detect
cargo test -p dam-detect-bench
scripts/dam-build.sh detector-bench
cargo run -q -p dam-detect-bench -- --format json
```

`dam-detect-bench` is the lightweight executable benchmark harness for the current DAM detector contract. It evaluates synthetic labeled spans against `dam-detect`, reports overall/per-kind precision/recall/F1 plus concrete false-positive/false-negative cases, and exits non-zero when the baseline suite regresses. Its implementation keeps benchmark fixtures in `crates/dam-detect-bench/src/cases.rs`, CLI parsing in `cli.rs`, evaluation in `evaluator.rs`, metric accounting in `metrics.rs`, and report rendering in `report.rs` so detector-contract growth does not turn the harness entrypoint into a mixed-purpose file.
