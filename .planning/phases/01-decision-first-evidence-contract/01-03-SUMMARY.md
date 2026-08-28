---
phase: 01-decision-first-evidence-contract
plan: 03
subsystem: adversarial-contract
status: complete
completed: 2026-08-26
requirements-completed: [EVID-03, EVID-05, EVID-06, EVID-08]
tech-stack:
  added: []
  patterns: [deep-cloned contract mutations, authoritative action budget, explicit active-version anchors]
key-files:
  created:
    - tests/report-contract.test.mjs
    - tests/fixtures/decision-report/blocked-report.json
    - tests/fixtures/decision-report/blocked-assessment.json
  modified:
    - crates/ratemypcb-core/src/lib.rs
    - crates/ratemypcb-cli/src/viewer.rs
    - crates/ratemypcb-cli/tests/decision_report.rs
    - skills/review-pcb-dfm/SKILL.md
    - skills/review-pcb-dfm/references/report-contract.md
    - .planning/phases/01-decision-first-evidence-contract/01-03-PLAN.md
commits: 0
---

# Phase 1 Plan 03: Adversarial Contract Regressions Summary

Added dependency-free sanitized contract goldens and isolated Node mutations for ambiguous/missing disposition, fail-open evidence, broken references, duplicate public IDs, absent completeness/freshness, four actions, score-first ordering, shared anchor wiring, and active 2.0 version drift.

## Behaviors implemented

- `validate_assessment` now rejects more than three actions at the authoritative core boundary; the viewer does not truncate or repair input.
- The evaluator validates reference integrity and supplied public-ID uniqueness without claiming canonical-ID recomputation or runtime DOM execution.
- Active core constants/generated schema IDs, parsed checked-in schemas, viewer fixture, skill, and report-contract reference are anchored to report 2.0 and assessment 2.0 while historical schemas remain untouched.
- Skill/reference ordering is disposition, actions, completeness/freshness, then secondary scores, with separate risk/coverage/confidence/freshness/approval and fail-closed unknown states.
- Goldens are purpose-built and contain no customer, provider, credential, or restricted data; no overload fixture was created.

## Verification

- `cargo test -p ratemypcb-core --locked decision_contract_rejects_more_than_three_actions` — pass (1).
- `node --test tests/report-contract.test.mjs` — pass (13 assertions/subtests).
- `cargo test --all --locked` — final pass (61 tests; 0 failed).
- `node --test tests/board-view.test.mjs tests/report-contract.test.mjs` — pass (14 assertions/subtests).
- `cargo test -p ratemypcb-cli --test decision_report --locked` — pass (2).
- `node --check tests/report-contract.test.mjs` and `node --check crates/ratemypcb-cli/assets/local-viewer.js` — pass.
- `cargo fmt --all -- --check` — pass.
- `git diff --check` — pass.
- Primary LSP diagnostics on touched Rust/JavaScript — clean.

## Deviation

One complete-suite run exposed a parallel temp-directory name collision in the pre-existing Phase 1 CLI tracer. As a strictly necessary adjacent Phase 1 correction, `crates/ratemypcb-cli/tests/decision_report.rs` now adds an atomic sequence to temp paths. The final full suite and focused tracer both pass.

The repository-manager formatting gate additionally authorized rustfmt-only changes in `crates/ratemypcb-cli/src/main.rs`, `crates/ratemypcb-cli/src/viewer.rs`, `crates/ratemypcb-cli/tests/decision_report.rs`, `crates/ratemypcb-core/src/lib.rs`, and adjacent `crates/ratemypcb-core/src/stackup.rs`. Before/after inspection confirmed only import ordering and rustfmt line wrapping; behavior is unchanged.

## Residual risks

Runtime browser DOM/keyboard behavior remains intentionally outside dependency-free Phase 1 automation and belongs to Phase 2. No known formatting drift remains in the Rust workspace.

No files were staged or committed.
