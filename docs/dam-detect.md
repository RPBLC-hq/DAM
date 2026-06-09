# dam-detect

`dam-detect` is a pure detection module.

It receives text and returns sensitive spans. It does not redact, write to vault, log, or decide policy.

## Current Coverage

- Email, including whitespace around separators such as `alice@ example.com` or `alice @example.com`.
- Caller-supplied related domains, for context-aware response protection only.
- NANP phone numbers in dashed form, e.g. `415-555-2671`.
- SSN with basic area validation.
- Credit card numbers with Luhn validation.
- API-key/secret assignments using common key names such as `*_API_KEY`, `secret_key`, or `access_token`; the detected span is the assigned secret value, not the variable name.

Domain-only values are not detected by the default `detect()` path. `detect_with_related_domains()` can emit exact `domain` detections for domains supplied by the caller, such as domains learned from outbound email detections, while still avoiding the same domain inside an email address or subdomain. Email addresses are still detected as whole values, but their domains are not emitted as separate `domain` detections unless passed back as related context.

Known current limitation: formats like `+1 (415) 555-2671`, zero-width-character obfuscation, and provider-specific token families without an assignment label are not detected yet.

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
```
