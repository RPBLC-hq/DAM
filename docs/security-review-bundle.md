# DAM Security Review Bundle

This guide gives a reviewer one local, synthetic-data-only path to gather DAM trust evidence without implying compliance certification, enterprise DLP replacement, or universal leak prevention.

## What this bundle is

Use this bundle when you want a small reproducible answer to:

- does DAM replace supported sensitive values before upstream egress?
- can contributors inspect detector quality without real secrets?
- do current smoke checks fail closed on raw-value leakage in non-vault surfaces?
- can the installed app explain setup and recovery state without immediately mutating the host?
- what current build/release posture exists today?

## What this bundle is not

This bundle supports local technical review. It is **not**:

- a SOC 2 report;
- a GDPR/compliance certification claim;
- proof that DAM prevents all data leakage;
- proof for unsupported traffic types or unsupported sensitive-value kinds;
- a substitute for your own security review, threat model, or deployment controls.

## Entry point

Start here from a DAM checkout:

```bash
git diff --check
scripts/dam-build.sh detector-bench
python3 scripts/dam_fake_openai_upstream.py --port 18080
DAM_AGENT_E2E_UPSTREAM=http://127.0.0.1:18080 scripts/dam-build.sh agent-protection-smoke
```

Keep the fake upstream running only for the protection smoke, then stop it.

## Bundle sections

### 1. Protection smoke

Goal: prove supported synthetic values are replaced before a loopback upstream receives them, while the trusted local client still gets the resolved response.

Use a local OpenAI-compatible endpoint if you already have one:

```bash
scripts/dam-build.sh agent-protection-smoke
```

If no local model is listening, use the deterministic loopback fake upstream:

```bash
python3 scripts/dam_fake_openai_upstream.py --port 18080
DAM_AGENT_E2E_UPSTREAM=http://127.0.0.1:18080 scripts/dam-build.sh agent-protection-smoke
```

Current evidence boundary:

- loopback only;
- synthetic email + SSN fixtures only;
- no paid provider calls;
- verifies upstream tokenization, trusted-side exact echo, adversarial token transformation, and non-vault activity-log raw-value absence.

### 2. Detector benchmark

Goal: show current supported detector coverage and false-positive/false-negative behavior on synthetic fixtures.

```bash
scripts/dam-build.sh detector-bench
cargo run -q -p dam-detect-bench -- --format json
```

Current evidence boundary:

- benchmark results are only for the detector families and fixtures currently shipped;
- strong benchmark results do not imply protection for unsupported kinds.

### 3. Raw-value leakage boundary

Goal: show that current smoke coverage fails closed if synthetic raw values appear outside intentional local reveal surfaces.

Current executable proof path:

```bash
python3 scripts/dam_fake_openai_upstream.py --port 18080
DAM_AGENT_E2E_UPSTREAM=http://127.0.0.1:18080 scripts/dam-build.sh agent-protection-smoke
```

What that proves today:

- the upstream did not receive the raw synthetic values;
- the smoke's activity-log scan fails if the temporary non-vault log store contains the raw synthetic values.

Current limitation:

- richer Connect / Activity surface verification is still tracked separately and should not be described as shipped here unless the corresponding visible-evidence proof path is merged and rerun on the reviewed head.

### 4. Setup and recovery posture

Goal: show that installed-app setup state and recovery guidance are inspectable before running mutating repair flows.

Installed-app inspection commands:

```bash
scripts/dam-build.sh agent-status --network-mode tun --trust-mode local_ca
scripts/dam-build.sh agent-recovery-smoke --network-mode tun --trust-mode local_ca
```

Optional retained/fixture state inspection:

```bash
scripts/dam-build.sh agent-status --network-mode tun --trust-mode local_ca --state-dir /path/to/state
scripts/dam-build.sh agent-recovery-smoke --network-mode tun --trust-mode local_ca --state-dir /path/to/state
```

Current evidence boundary:

- these are installed-app release-path checks;
- `agent-recovery-smoke` is read-only;
- mutating recovery remains explicitly gated behind `agent-repair-smoke --confirm-mutation` and should only run on disposable or explicitly approved state.

Platform boundary:

- the installed app flow is currently the macOS release path; on Linux/Windows, do not claim equivalent packaged recovery posture until those release paths exist.

### 5. Build / release posture

Goal: show what local maintainers can verify today about build hygiene and release preparation.

```bash
scripts/dam-build.sh agent-check
scripts/dam-build.sh release-macos --mode developer-id
```

Current evidence boundary:

- `agent-check` covers repository verification and whitespace checks;
- `release-macos` covers the macOS signed/notarized artifact path when the required credentials are configured;
- this is build/release evidence, not a supply-chain attestation, SBOM, or certification claim.

### 6. Claim boundary text

Safe summary wording:

> DAM provides local technical evidence that supported synthetic sensitive values were replaced before supported AI/agent traffic reached a configured loopback or local upstream, and that the current smoke path checks non-vault temporary log storage for raw synthetic-value leakage.

Avoid wording like:

- “DAM is SOC 2 compliant”;
- “DAM makes AI usage GDPR compliant”;
- “DAM prevents all leakage”;
- “DAM replaces enterprise DLP”; or
- “DAM completes vendor risk review.”

## Suggested reviewer note template

```text
Reviewed with local synthetic-data-only DAM evidence.
Ran: git diff --check, detector-bench, agent-protection-smoke via loopback upstream, and the relevant installed-app status/recovery probes.
Observed: DAM proof is bounded to supported traffic/kinds and current local smoke checks; no compliance/certification claim implied.
```

## Related docs

- [build-release.md](build-release.md)
- [dam-detect.md](dam-detect.md)
- [dam-e2e.md](dam-e2e.md)
