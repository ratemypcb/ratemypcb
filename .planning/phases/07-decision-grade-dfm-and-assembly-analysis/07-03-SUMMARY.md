---
phase: 07-decision-grade-dfm-and-assembly-analysis
plan: 03
subsystem: dfm-assembly
tags: [dfm, fixed-point, provenance, qualification, release-actions]
requires:
  - phase: 07-decision-grade-dfm-and-assembly-analysis
    plan: 02
    provides: Format-independent typed population tracer and report validation.
  - phase: 06-intelligent-interchange-decision-gate
    plan: 08
    provides: No-go closure for ODB++ and IPC-2581 this release.
provides:
  - One bounded CLI-to-core declaration seam normalized into existing fixed-point manufacturing and construction contracts.
  - One exact static family/version GateImpact policy with report-side recomputation; every shipped family remains EvidenceOnly.
  - One score-independent smallest-unblock evidence set bound to non-approve assessment priority 1.
affects: [07-04, 07-05, 07-06, 07-07, 07-08, 07-09, 07-10, 07-11]
actuals:
  tokens: 22000
  tasks: 3
  commits: 0
tech-stack:
  added: []
  patterns: [bounded local authority normalization, static fail-closed qualification, core-ranked evidence references]
key-files:
  created:
    - tests/fixtures/dfm/declarations.json
    - .planning/phases/07-decision-grade-dfm-and-assembly-analysis/07-03-SUMMARY.md
  modified:
    - crates/ratemypcb-core/src/dfm.rs
    - crates/ratemypcb-core/src/lib.rs
    - crates/ratemypcb-core/tests/dfm_release.rs
    - crates/ratemypcb-core/tests/fabrication_release.rs
    - crates/ratemypcb-core/tests/schematic_release.rs
    - crates/ratemypcb-cli/src/main.rs
    - crates/ratemypcb-cli/tests/decision_report.rs
    - .planning/phases/07-decision-grade-dfm-and-assembly-analysis/07-VALIDATION.md
key-decisions:
  - "Declaration identity is semantic `(group, id, applicability)`; distinct resolved layers may carry the same fact ID, while duplicate or overlapping authority fails closed."
  - "No family has reviewed promotion metadata or inference approval, so every current exact family/version remains EvidenceOnly."
  - "Assessment prose stays human-authored; core returns only the deterministic top evidence-reference set and requires non-approve P1 to intersect it."
patterns-established:
  - "Local declaration bytes are bounded before allocation, hashed exactly, and represented by safe project-relative or external logical source paths."
  - "Required evidence ranks before qualified blockers, which rank before EvidenceOnly attention; score is never an input."
requirements-completed: [DFM-01, DFM-02, DFM-05, DFM-06]
coverage:
  - id: D1
    description: "Source/version/digest/location/applicability-bound threshold declarations convert exact mm/in decimals to existing Picometres constraints and reject stale, duplicate, conflicting, dangling, unknown, or over-limit authority."
    requirement: DFM-01
    verification:
      - kind: integration
        ref: "crates/ratemypcb-core/tests/dfm_release.rs#authority_*"
        status: pass
      - kind: e2e
        ref: "crates/ratemypcb-cli/tests/decision_report.rs#dfm_authority_cli_normalizes_bounded_source_linked_declarations"
        status: pass
    human_judgment: false
  - id: D2
    description: "Represented customer order/profile facts normalize only into existing constraints/construction, while drill-span/plating, castellation, edge-plating, stackup-order, and profile facts remain confirmation-gap evidence."
    requirement: DFM-02
    verification:
      - kind: integration
        ref: "crates/ratemypcb-core/tests/dfm_release.rs#authority_normalizes_exact_rules_and_represented_order_facts"
        status: pass
    human_judgment: false
  - id: D3
    description: "Exact family/version qualification is unique, bounded, fail-closed, and recomputed during report validation; no deterministic or inference family is promoted."
    requirement: DFM-05
    verification:
      - kind: integration
        ref: "crates/ratemypcb-core/tests/dfm_release.rs#qualification_validation_recomputes_unknown_and_inference_family_impact"
        status: pass
      - kind: unit
        ref: "crates/ratemypcb-core/src/dfm.rs#qualification_*"
        status: pass
    human_judgment: false
  - id: D4
    description: "Core selects the deterministic score-independent smallest release unblock and non-approve assessment P1 must reference its nonempty top evidence set."
    requirement: DFM-06
    verification:
      - kind: integration
        ref: "crates/ratemypcb-core/tests/dfm_release.rs#unblock_*"
        status: pass
      - kind: unit
        ref: "crates/ratemypcb-core/src/dfm.rs#unblock_tiers_ties_and_corrective_action_dedupe_are_exact"
        status: pass
    human_judgment: false
duration: not-measured
completed: 2026-08-31
status: complete
---

# Phase 7 Plan 03: Authority, Qualification, and Release-Unblock Policy Summary

**Bounded source-linked declarations now feed existing fixed-point facts, while exact all-EvidenceOnly family qualification and score-independent P1 policy fail closed in core.**

## Performance

- **Duration:** Not measured — recovered from an interrupted prior-owner worktree.
- **Completed:** 2026-08-31T08:59:25Z
- **Tasks:** 3
- **Files modified:** 12

## Accomplishments

- Added one optional `--dfm-declarations` path through `ReviewOptions` into existing `FabricationReview.constraints` and representable construction records, with exact unit conversion, bounded collections, source provenance, semantic applicability, merge-conflict rejection, and explicit unrepresented-order gaps.
- Added one exact 26-family static qualification policy and report-side GateImpact recomputation. Undefined/out-of-range metrics, missing corpus classes, red mutations, missing/mismatched promotion metadata, unknown versions, and every unapproved inference family select only `EvidenceOnly`.
- Added deterministic required-evidence → qualified-blocker → EvidenceOnly ranking, exact tie breaks/corrective-action grouping, missing-coverage evidence repair, score independence, and non-approve P1 intersection without viewer policy or recommendation prose.

## Recovered vs New Work

- **Recovered and retained:** the interrupted Task 1 declaration DTO/parser, CLI flag, `ReviewOptions` seam, canonical merge, fixture, provenance tests, and the partially inserted Task 2 static policy/qualification unit tests.
- **Completed in this recovery:** declaration conflict/applicability/source-path hardening; GateImpact validator wiring; all remaining Task 2 integration mutations; the complete Task 3 ranking/P1 implementation and tests; existing assessment-test migration; review remediation; final verification and planning close-out.
- **Absent on takeover and newly added:** the top-unblock helper, required-coverage occurrence repair, assessment P1 enforcement, qualification integration test, and all unblock integration/unit mutations.

## Task Commits

No commits were created. The user explicitly prohibited staging, commits, and pushes.

## Verification

- Focused CLI authority: 1/1 passed.
- Focused core authority: 5/5 passed; compile-only exhaustive constructors passed.
- Qualification: 1/1 integration plus 3/3 unit checks passed.
- Unblock: 3/3 integration plus ranking unit checks passed; assessment filter 2/2 passed.
- Workspace Rust: 247 tests passed.
- Node report/viewer: 31 tests passed.
- `cargo fmt --all -- --check`, strict `cargo clippy --all-targets --locked -- -D warnings`, generated report-schema equality, JS syntax, Python compilation/dry-run installer, and `git diff --check` passed.
- Focused Rust LSP: five changed source/test files clean.

## Independent Review

Exactly one bounded fresh-context correctness/security review was run. It returned two valid findings:

1. **P1:** declaration identity ignored applicability, preventing repeated per-layer facts and layer-scoped rules. Fixed with `(group, id, applicability)` identity, unique layer resolution, overlap-aware authority conflict handling, and a two-layer regression.
2. **P2:** CLI provenance reduced every declaration path to its basename. Fixed with canonical bounded project-relative paths and distinct hashed logical paths for external namesakes; reads now use the same canonical file identity.

Both findings were remediated once. No repeat-review or zero-finding ceremony was added.

## Deviations from Plan

None — recovery and review fixes stayed inside the planned trust-boundary, qualification, and ranking contracts.

## Issues Encountered

- The interrupted state compiled and Task 1's original focused tests passed, but Task 2 validation was unwired and Task 3 was absent.
- Strict Clippy exposed two recovered redundant match guards and two test-only needless borrows; all were reduced without behavior change.
- The expensive lens runner reports repository-wide/pre-existing unwrap rules and generic-key false positives on fixture SHA/family strings. Focused LSP, strict Clippy, tests, and diff checks found no Plan 07-03 blocking error.

## Residual Risks

- Declaration issuance/expiry is enforced when original bytes enter production but is not represented as a first-class canonical report field; a serialized report alone cannot re-evaluate wall-clock freshness without the original declaration bytes.
- No family has promotion authority. All DFM and inference output remains `EvidenceOnly` until exact reviewed metadata and any required human approval are added by later plans.
- ODB++/IPC-2581 remain no-go; no adapter, parity fixture, private parser/corpus, or support claim entered this plan.

## User Setup Required

None.

## Next Phase Readiness

Plan 07-04 is ready to consume only the normalized source-bound constraints and central policy. It must keep finished-drill and outline families EvidenceOnly unless their own exact qualification and promotion gates are satisfied.

## Self-Check: PASSED

All declared artifacts exist, all planned checks pass, the one review's valid findings are remediated, HEAD remains unchanged, and no files are staged or committed.

---
*Phase: 07-decision-grade-dfm-and-assembly-analysis*
*Completed: 2026-08-31*
