---
phase: 01-decision-first-evidence-contract
plan: 01
subsystem: core-contract
tags: [rust, serde, sha256, provenance, json-schema, fail-closed]
requires: []
provides:
  - Authoritative report and assessment 2.0 schema paths
  - Independent observed-risk, required-evidence execution/result/freshness, confidence, and approval semantics
  - SHA-256 global evidence IDs with bounded provenance and duplicate validation
  - Report validation at the CLI render deserialization boundary
affects: [01-02, 01-03, report-ux, supply, schematic, fabrication]
actuals:
  tokens: 14000
  tasks: 2
  commits: 0
tech-stack:
  added: []
  patterns: [global evidence namespace, deterministic canonical identity, fail-closed report validation]
key-files:
  created: [schemas/report-2.0.json, schemas/assessment-2.0.json]
  modified: [crates/ratemypcb-core/src/lib.rs, crates/ratemypcb-core/Cargo.toml, crates/ratemypcb-cli/src/main.rs, .planning/phases/01-decision-first-evidence-contract/01-CONTEXT.md, .planning/phases/01-decision-first-evidence-contract/01-01-PLAN.md]
key-decisions:
  - "Assessment disposition is authoritative; report score and assessment rating remain secondary."
  - "Evidence identity is SHA-256 over artifact digest, check ID, and canonical structured location only."
  - "Attention and every missing, failed, stale, unsupported, or unknown required-evidence state close approval without changing observed risk."
patterns-established:
  - "Validate report structure, provenance, global uniqueness, and gate consistency before consuming assessment data."
  - "Keep stable checkId in evidence records and expose occurrence IDs through one global ev-* namespace."
requirements-completed: [EVID-01, EVID-02, EVID-03, EVID-04, EVID-05, EVID-06, EVID-07]
coverage:
  - id: D1
    description: "Decision-grade report semantics and fail-closed required-evidence gate"
    requirement: EVID-01
    verification:
      - kind: unit
        ref: "crates/ratemypcb-core/src/lib.rs#decision_contract_tracer_carries_a_blocked_release"
        status: pass
      - kind: unit
        ref: "crates/ratemypcb-core/src/lib.rs#decision_contract_required_states_fail_closed_without_changing_risk"
        status: pass
    human_judgment: false
  - id: D2
    description: "Stable SHA-256 evidence identity, provenance validation, and schema authority"
    requirement: EVID-03
    verification:
      - kind: unit
        ref: "cargo test -p ratemypcb-core --locked decision_contract"
        status: pass
    human_judgment: false
  - id: D3
    description: "CLI render rejects invalid reports before assessment validation or rendering"
    requirement: EVID-05
    verification:
      - kind: integration
        ref: "crates/ratemypcb-cli/src/main.rs#render_snapshot_rejects_invalid_report_before_rendering"
        status: pass
    human_judgment: false
duration: 25min
completed: 2026-08-26
status: complete
---

# Phase 1 Plan 01: Decision-Grade Contract Summary

**Report/assessment 2.0 now separates observed risk from evidence completeness and release approval, with deterministic provenance-backed IDs and fail-closed validation.**

## Performance

- **Duration:** 25 min
- **Completed:** 2026-08-26T12:19:38Z
- **Tasks:** 2
- **Files modified/created:** 8
- **Commits:** 0 (explicitly prohibited)

## Accomplishments

- Added report 2.0 decision fields, independent required-evidence execution/result/freshness semantics, and approval truth-table coverage including attention and unknown states.
- Added deterministic SHA-256 evidence IDs, stable check IDs, bounded provenance, global duplicate rejection, and structured assessment-question references.
- Added authoritative report/assessment schema generators and checked-in 2.0 schemas, plus CLI report validation before assessment/render handling.

## Files Created/Modified

- `crates/ratemypcb-core/src/lib.rs` — 2.0 DTOs, evidence identity/provenance, gate policy, validation, schemas, and tests.
- `crates/ratemypcb-core/Cargo.toml` — wires the existing workspace `sha2` dependency.
- `crates/ratemypcb-cli/src/main.rs` — removes assessment-version hardcoding and validates reports at `render_snapshot` deserialization.
- `schemas/report-2.0.json` — generated active report contract.
- `schemas/assessment-2.0.json` — generated active assessment contract.
- `.planning/phases/01-decision-first-evidence-contract/01-CONTEXT.md` — records superseding spike decisions.
- `.planning/phases/01-decision-first-evidence-contract/01-01-PLAN.md` — aligns execution record with accepted corrections.
- `.planning/phases/01-decision-first-evidence-contract/01-01-SUMMARY.md` — this execution record.

## Decisions Made

- Historical `report-1.2.json` and `assessment-1.0.json` remain unchanged.
- No parallel report model or new package was added.
- Canonical identity excludes prose, severity, ordering, array positions, and machine-specific paths.

## Deviations from Plan

The completed contract spike superseded stale plan details: core `sha2` wiring, CLI `validate_report` integration, and planning-record corrections were added exactly as required by the execution objective. No unauthorized files were touched.

## Issues Encountered

- Existing viewer tests deserialize historical fixtures; serde defaults preserve that baseline compatibility while active validation still requires a complete 2.0 report.

## Verification

- `cargo test -p ratemypcb-core --locked decision_contract_tracer` — pass (1 test).
- `cargo test -p ratemypcb-core --locked decision_contract` — pass (5 tests).
- `cargo test -p ratemypcb-core --locked` — pass (47 tests; 0 failed).
- `cargo test -p ratemypcb-cli --locked render_snapshot_rejects_invalid_report_before_rendering` — pass (1 test).
- `cargo test -p ratemypcb-cli --locked` — pass (11 tests; 0 failed).
- `cargo test --all --locked` — pass (58 tests; 0 failed).
- `cargo fmt --all -- --check` and `git diff --check` — pass.
- Rust primary LSP diagnostics — no errors/warnings; auxiliary informational suggestions only.

## Next Phase Readiness

Plan `01-01` is complete. Plans `01-02` and `01-03` were not started; the repository manager retains wave progression ownership.

---
*Phase: 01-decision-first-evidence-contract*
*Completed: 2026-08-26*
