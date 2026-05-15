# DAM Agent Instructions

This workspace is being rebuilt one module at a time. Keep changes small, explicit, and contract-driven.

## Open Source Guidelines

This project is intended to be open source.

- Do not add proprietary business logic, private customer assumptions, or environment-specific behavior to core modules.
- Do not commit secrets, real credentials, real personal data, private URLs, or internal-only deployment details.
- Prefer small public contracts and replaceable implementations over closed, tightly coupled integrations.
- Keep public APIs, config keys, CLI behavior, and failure modes documented.
- Use synthetic data in examples, tests, fixtures, docs, and manual test commands.
- Keep dependencies minimal and compatible with the project license. Avoid adding a dependency unless it is clearly justified.
- Do not introduce telemetry, network calls, or external services without explicit config and documentation.
- Treat local SQLite implementations as reference implementations, not as the only possible backends.
- Keep error messages useful, but avoid leaking raw sensitive values or secret material.
- Preserve a clean contributor path: format, clippy, tests, docs, and clear module boundaries.

## Module Changes

When editing a module under `crates/<module>`:

- Update the matching module doc in `docs/<module>.md` in the same change.
- Update `docs/README.md` if module responsibilities, pipeline position, or public commands change.
- Update `dam.example.toml` if configuration keys, defaults, or supported values change.
- Update `../RPBLC.Architecture` when an agreed design decision, implemented behavior, public interface, config key, failure mode, pipeline shape, or module boundary changes.
- Do not introduce direct cross-module calls that bypass `dam-core` contracts.

## File Division

Do not grow single-file libraries for non-trivial modules. When a crate adds meaningful behavior, split it into focused files with explicit responsibilities, for example `config`, `state`, `platform`, `routing`, `server`, `errors`, and `tests` where those boundaries fit the crate.

- Keep `src/lib.rs` as the public contract/re-export surface, not as the implementation dumping ground.
- Keep `src/main.rs` as command wiring and process startup, not business logic.
- Prefer small module files with narrow ownership over broad "manager" files.
- Keep structs, enums, traits, and impl blocks focused on one purpose. If a type starts coordinating unrelated concerns, split the concern behind a smaller module or helper type.
- Avoid catch-all helpers, generic "util" modules, and mixed-purpose files. Name modules by the domain responsibility they own.
- Keep reusable behavior in the crate that owns the contract, not copied into command, web, tray, or platform glue.
- Update the matching docs when module boundaries change so future agents can find the implementation quickly.
- Break large files into manageable pieces before they become difficult to review or reuse.

## Test Organization

Keep tests outside production module bodies.

- Unit tests that need private access should live in sibling `*_tests.rs` files and be included from the production file with `#[cfg(test)]` plus `#[path = "..."] mod tests;`.
- Integration and CLI tests should live under each crate's `tests/` directory.
- Do not add new inline `mod tests { ... }` blocks to production files.
- Before removing or materially changing a test, stop and identify what else it may protect: edge cases, migration behavior, privacy guarantees, security boundaries, failure modes, platform behavior, or a secondary contract not obvious from the test name.
- Prefer preserving test intent when refactoring. If a test is obsolete, update or replace it with coverage for the current contract instead of deleting it casually.
- When test fixtures contain sensitive-looking values, keep them synthetic and verify logs, errors, and reports do not persist raw sensitive values unless the test is explicitly about vault storage.

## Architecture Sync

`../RPBLC.Architecture` is the authoritative contract repo. Keep it current when design decisions are agreed in discussion and when behavior is implemented in code.

- Update the relevant architecture files in the same change as the DAM implementation whenever possible.
- If an architecture update is needed but cannot be completed in the same change, call it out explicitly before considering the work done.
- Do not let local DAM docs, README claims, config examples, or code behavior drift away from the architecture contracts.

## Tests

Every module change should include relevant tests.

- Keep existing unit tests passing.
- Add or update unit tests for changed behavior.
- Add edge-case coverage for boundary conditions, failure paths, invalid config, and privacy-sensitive behavior.
- For CLI or integration behavior, update tests under the consuming module, e.g. `crates/dam-filter/tests/`.
- For policy, vault, log, and redaction behavior, test that raw sensitive values are not persisted outside the vault.

## Verification

Before considering work complete, run:

```bash
cargo fmt --all --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

If a command cannot be run, document why in the final response.

## Documentation Rule

Docs are part of the implementation. A module edit without corresponding docs and tests is incomplete unless the change is purely mechanical and does not affect behavior, config, contracts, or usage.
