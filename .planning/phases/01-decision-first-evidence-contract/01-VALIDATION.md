---
phase: 1
slug: decision-first-evidence-contract
status: validated
nyquist_compliant: true
wave_0_complete: true
created: 2026-08-26
---

# Phase 1 — Validation Strategy

> Per-phase validation contract for fast feedback during execution.

## Test Infrastructure

| Property | Value |
| ---------- | ------- |
| **Framework** | Rust built-in test harness + Node built-in `node:test` |
| **Config file** | Workspace `Cargo.toml`; no Node config |
| **Quick run command** | `cargo test -p ratemypcb-core --locked decision_contract` |
| **Full suite command** | `cargo test --all --locked && node --test tests/board-view.test.mjs tests/report-contract.test.mjs` |
| **Feedback target** | Each task has a focused command; full suite runs after each wave |

## Sampling Rate

- **After every task:** Run its focused `<verify><automated>` command.
- **After every plan wave:** Run the full suite.
- **Before phase verification:** Full suite, plan structure checks, and schema equality must be green.
- **No watch mode:** All commands terminate.

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
| --------- | ------ | ------ | ------------- | ------------ | ----------------- | ----------- | ------------------- | ------------- | -------- |
| 01-01-01 | 01 | 1 | EVID-01, EVID-02, EVID-07 | T-01-01, T-01-04 | Missing evidence cannot approve | contract tracer | `cargo test -p ratemypcb-core --locked decision_contract_tracer` | Existing harness | ✅ green |
| 01-01-02 | 01 | 1 | EVID-03, EVID-04, EVID-06 | T-01-02 | IDs/provenance/schema are deterministic | unit/schema | `cargo test -p ratemypcb-core --locked decision_contract` | Existing harness | ✅ green |
| 01-02-01 | 02 | 2 | EVID-07 | T-01-01, T-01-03 | Digest-bound decision reaches escaped offline HTML | integration | `cargo test -p ratemypcb-cli --test decision_report --locked tracer` | Created by task | ✅ green |
| 01-02-02 | 02 | 2 | EVID-05 | T-01-01 | All assessment claims/questions resolve | unit/integration | `cargo test -p ratemypcb-cli --test decision_report --locked assessment` | Created by prior task | ✅ green |
| 01-03-01 | 03 | 3 | EVID-08 | T-01-04 | Ambiguity/false pass/load fail closed | mutation | `node --test tests/report-contract.test.mjs` | Created by task | ✅ green |
| 01-03-02 | 03 | 3 | EVID-03, EVID-06, EVID-08 | T-01-02 | Reordering/prose/schema drift do not break identity silently | regression | `cargo test --all --locked && node --test tests/board-view.test.mjs tests/report-contract.test.mjs` | Existing + created | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

## Wave 0 Requirements

Existing Rust and Node built-in infrastructure covers the phase. Each new integration/evaluation file is created by the first task that uses it before its automated command runs; there are no `MISSING` verification placeholders and no package installs.

## Manual-Only Verifications

None. The Phase 1 contract is structurally machine-verifiable. Human visual/usability validation belongs to Phase 2.

## Validation Sign-Off

- [x] All tasks have `<automated>` verification.
- [x] Sampling continuity has no unverified implementation run.
- [x] No Wave 0 dependency is missing.
- [x] No watch-mode flags.
- [x] No package install or live service.
- [x] `nyquist_compliant: true` set in frontmatter.

**Approval:** strategy approved and execution verified 2026-08-26; all Phase 1 automated gates green
