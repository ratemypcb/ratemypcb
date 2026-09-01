---
phase: 07-decision-grade-dfm-and-assembly-analysis
plan: 04
subsystem: dfm-geometry
tags: [dfm, drill, xnc, outline, fixed-point, evidence-only]
requires:
  - phase: 07-decision-grade-dfm-and-assembly-analysis
    plan: 03
    provides: Source/version/digest declaration authority, central family qualification, canonical evidence validation, and score-independent release ranking.
  - phase: 05-manufacturing-evidence-model-and-gerber-baseline
    plan: 06
    provides: Bounded fixed-point Gerber/X2, XNC, native KiCad, profile, capability, and provenance contracts.
provides:
  - Source-authoritative exact minimum-finished-drill measurements that exclude routes, slots, presets, and direct constraints.
  - Exact drill/tool integrity across distinct drill, route, and slot objects with complete plating/span/source-resolution facts.
  - Bounded checked outline closure/intersection/classification measurements with conservative not_checked arc and unsupported-state handling.
  - Project-authored positive, hard-negative, mutation, determinism, and resource corpus rows for all three families.
affects: [07-05, 07-06, 07-07, 07-08, 07-09, 07-10, 07-11]
actuals:
  tokens: 29436
  tasks: 2
  commits: 0
tech-stack:
  added: []
  patterns: [declaration-bound fixed-point comparison, capability-dispatched semantic measurement, bounded checked topology, exact output-set revalidation]
key-files:
  created:
    - tests/fixtures/dfm/geometry-targets.json
    - .planning/phases/07-decision-grade-dfm-and-assembly-analysis/07-04-SUMMARY.md
  modified:
    - crates/ratemypcb-core/src/dfm.rs
    - crates/ratemypcb-core/src/lib.rs
    - crates/ratemypcb-core/tests/dfm_release.rs
    - tests/fixtures/dfm/manifest.json
    - .planning/phases/07-decision-grade-dfm-and-assembly-analysis/07-VALIDATION.md
    - .planning/ROADMAP.md
    - .planning/STATE.md
key-decisions:
  - "Minimum finished drill accepts exactly one Plan 07-03 declaration-backed MinimumDrill constraint and one complete round-hit/tool chain; every other threshold source remains not_checked."
  - "Drill, route, and slot geometries retain separate counts and tool checks; only Geometry::Drill with ToolKind::Drill can enter the round-hit minimum."
  - "Outline topology uses checked integer line predicates and only an exactly provable bounded arc subset; uncertain arc intersections, transforms, polarity, expansion, containment, or classification remain not_checked."
  - "Corpus metrics are recorded, but reviewed promotion metadata remains absent, so all three families stay EvidenceOnly."
patterns-established:
  - "Analyzer coverage and findings are recomputed from the canonical FabricationReview and the complete family coverage check-ID set must match exactly."
  - "Quadratic outline pairing is capped at 1,000,000 checked pairs and 1,414 retained segments with the inherited absolute manufacturing deadline checked inside loops."
requirements-completed: [DFM-01, DFM-05]
coverage:
  - id: D1
    description: "Minimum finished drill reports exact observed/threshold/delta/resolution plus hit, tool, declaration producer/version/path/record provenance only from complete source-authoritative facts."
    requirement: DFM-01
    verification:
      - kind: integration
        ref: "crates/ratemypcb-core/tests/dfm_release.rs#drill_families_use_exact_declaration_authority_and_keep_objects_distinct"
        status: pass
      - kind: integration
        ref: "crates/ratemypcb-core/tests/dfm_release.rs#drill_one_resolution_violation_is_measured_and_evidence_only"
        status: pass
      - kind: unit
        ref: "crates/ratemypcb-core/src/dfm.rs#drill_mutations_fail_closed_and_distinct_objects_are_deterministic"
        status: pass
    human_judgment: false
  - id: D2
    description: "Drill-tool integrity preserves drill/route/slot identity, requires exact tool diameter, plating, span, and source resolution, and rejects route tools as round drills."
    requirement: DFM-01
    verification:
      - kind: integration
        ref: "cargo test -p ratemypcb-core --test dfm_release --locked drill_"
        status: pass
      - kind: unit
        ref: "crates/ratemypcb-core/src/dfm.rs#drill_mutations_fail_closed_and_distinct_objects_are_deterministic"
        status: pass
    human_judgment: false
  - id: D3
    description: "Outline topology preserves contour/exterior/cutout/source identity, measures open and exact line intersections, and fails closed on unsupported arcs, arithmetic, transforms, polarity, expansion, ambiguity, deadline, or resource ceilings."
    requirement: DFM-01
    verification:
      - kind: integration
        ref: "crates/ratemypcb-core/tests/dfm_release.rs#outline_complete_profile_reports_exact_stable_topology"
        status: pass
      - kind: integration
        ref: "crates/ratemypcb-core/tests/dfm_release.rs#outline_forged_family_prefixed_coverage_is_rejected"
        status: pass
      - kind: unit
        ref: "crates/ratemypcb-core/src/dfm.rs#outline_*"
        status: pass
    human_judgment: false
  - id: D4
    description: "All three project-authored family corpora retain positive/hard-negative targets, exact TP/FP/FN/TN and precision/recall, fourteen fail-closed mutation classes, deterministic ordering, resource cases, and EvidenceOnly promotion state."
    requirement: DFM-05
    verification:
      - kind: integration
        ref: "crates/ratemypcb-core/tests/dfm_release.rs#drill_outline_geometry_corpus_is_exact_bounded_and_evidence_only"
        status: pass
      - kind: unit
        ref: "crates/ratemypcb-core/src/dfm.rs#qualification_shipped_policy_is_unique_and_evidence_only"
        status: pass
    human_judgment: false
duration: 1h 8m
completed: 2026-08-31
status: complete
---

# Phase 7 Plan 04: Exact Drill/Tool and Outline Topology Summary

**Declaration-bound finished-drill and exact tool integrity now share one bounded fixed-point outline slice, with every uncertain or unsupported state remaining not_checked and EvidenceOnly.**

## Performance

- **Duration:** 1h 8m
- **Started:** 2026-08-31T09:28:04Z
- **Completed:** 2026-08-31T10:36:40Z
- **Tasks:** 2
- **Files modified:** 9

## Accomplishments

- Added `dfm.minimum-finished-drill.v1` through the existing dispatch and Plan 07-03 declaration seam. Exact threshold and one-source-resolution cases retain dual tool/hit and declaration provenance; missing, stale, duplicate, direct, preset, ambiguous, unknown-plating/span, or unsupported authority cannot pass or emit a finding.
- Added `dfm.drill-tool-integrity.v1` with separate round-drill, route, and slot accounting, exact tool-diameter/source-resolution checks, and strict round-drill `ToolKind::Drill` enforcement.
- Added `dfm.outline-topology.v1` using checked fixed-point line predicates, exact closure/intersection and exterior/cutout containment for represented contours, bounded conservative arc handling, deterministic ordering, deadline checks, and exact segment/pair ceilings.
- Bound all three coverage/finding families to the fabrication model digest and validator-recomputed exact family output set; no extra family-prefixed pass can survive validation.

## TDD Evidence

- **Task 1 RED:** four `drill_` integration tests failed because no drill/tool families existed. **GREEN:** 4/4 integration plus the mutation/determinism unit passed.
- **Task 2 RED:** outline integration failed because no outline coverage existed. **GREEN:** 3/3 integration plus five topology, hard-negative, mutation, arithmetic, determinism, and exact-resource unit tests passed.
- **Review remediation RED/GREEN:** route-tool round drill, extreme arithmetic panic, and forged family-prefixed coverage each failed before the single remediation pass and passed afterward.

## Task Commits

No commits were created. The user explicitly prohibited staging, commits, and pushes.

## Files Created/Modified

- `crates/ratemypcb-core/src/dfm.rs` — three capability-dispatched families, qualification evidence, checked topology, exact family-output validation, and unit mutations.
- `crates/ratemypcb-core/src/lib.rs` — production analyzer insertion plus fabrication evidence digest binding and validation.
- `crates/ratemypcb-core/tests/dfm_release.rs` — production authority, boundary, corpus, forgery, and report validation tests.
- `tests/fixtures/dfm/manifest.json` — exact corpus references for the three implemented families.
- `tests/fixtures/dfm/geometry-targets.json` — project-authored target, unsupported, and mutation rows.
- `.planning/phases/07-decision-grade-dfm-and-assembly-analysis/07-VALIDATION.md` — Plan 07-04 execution evidence.
- `.planning/ROADMAP.md`, `.planning/STATE.md` — four-plan completion and Plan 07-05 readiness.

## Decisions Made

- A declaration adds the existing `Constraints` capability only after successful canonical normalization; no direct constraint or Preset can create production threshold authority.
- Minimum-drill uses only `Geometry::Drill`; routes and slots are integrity objects but never round hits.
- BBox data is reported only as retained profile extents or used for conservative arc pruning, never as final intersection/classification truth.
- Arc cases not exactly provable by the bounded integer subset return `not_checked`; no flattening, floating geometry, general toolkit, or dependency was added.

## Independent Review

Exactly one fresh bounded correctness/security review examined the complete Plan 07-04-only diff and returned `BLOCK` with three valid P1 findings:

1. Unchecked exact-geometry multiplication and radius addition could panic/wrap on extreme synthetic coordinates.
2. Report validation accepted an extra canonical family-prefixed coverage occurrence.
3. Integrity accepted a route-kind tool for a round drill.

One remediation pass replaced topology arithmetic with checked operations, added extreme-coordinate/radius regressions, required the exact three-family coverage ID set, and enforced `ToolKind::Drill` for round hits. Focused and workspace gates then passed. No second review was run.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Correctness/Security] Checked all outline predicate and radius arithmetic**

- **Found during:** Independent review after Task 2
- **Issue:** Extreme synthetic coordinates could panic or wrap before returning not_checked.
- **Fix:** Checked cross-product subtraction/multiplication, radius sums/differences/squares, and propagated overflow as not_checked.
- **Files modified:** `crates/ratemypcb-core/src/dfm.rs`
- **Verification:** `outline_extreme_arithmetic_fails_closed_without_panicking`
- **Committed in:** Uncommitted by user instruction.

**2. [Rule 1 - Validation] Rejected extra family-prefixed coverage**

- **Found during:** Independent review after Task 2
- **Issue:** The expected three coverage rows were verified, but a fourth prefixed row was not rejected.
- **Fix:** Validator now requires exact equality between expected and actual fabrication-family coverage check IDs.
- **Files modified:** `crates/ratemypcb-core/src/dfm.rs`, `crates/ratemypcb-core/tests/dfm_release.rs`
- **Verification:** `outline_forged_family_prefixed_coverage_is_rejected`
- **Committed in:** Uncommitted by user instruction.

**3. [Rule 1 - Semantic Integrity] Required drill-kind tools for round drills**

- **Found during:** Independent review after Task 2
- **Issue:** Matching diameter alone allowed a route-kind tool to qualify a `Geometry::Drill` integrity pass.
- **Fix:** Geometry-kind/tool-kind compatibility is explicit; minimum finished drill also requires `ToolKind::Drill`.
- **Files modified:** `crates/ratemypcb-core/src/dfm.rs`
- **Verification:** `drill_mutations_fail_closed_and_distinct_objects_are_deterministic`
- **Committed in:** Uncommitted by user instruction.

**Total deviations:** 3 auto-fixed correctness/security defects. **Impact:** Required fail-closed hardening only; no scope expansion.

## Issues Encountered

- The accepted canonical Profile capability currently proves a narrow represented subset; potentially intersecting non-adjacent arcs and arc-bearing cutout containment remain deliberately not_checked.
- Lens heavyweight runners still label intentional test `unwrap`s and static family/digest strings as blocking policy findings. Strict Clippy and primary LSP report no source error; the family strings are not credentials.

## Verification

- Focused drill: 4/4 integration plus one mutation/determinism unit passed.
- Focused outline: 3/3 integration plus five topology/resource units passed.
- Preserved Plan 07-03 authority, qualification, and unblock filters passed.
- Workspace Rust: 259 tests passed.
- Node report/viewer: 31 tests passed.
- `cargo fmt --all -- --check`, strict workspace Clippy, and `git diff --check` passed.
- Primary Rust LSP: three changed Rust files clean.

## User Setup Required

None.

## Residual Risks

- Arc intersection and arc-bearing containment outside the exactly provable bounded subset remain not_checked rather than approximated.
- Declaration freshness remains enforced when original bytes enter production; a serialized report alone cannot re-evaluate wall-clock freshness without those bytes.
- All three families have complete project-authored metrics but no reviewed promotion metadata, so they remain EvidenceOnly.

## Next Phase Readiness

Plan 07-05 may consume the same source-bound constraints and bounded exact primitives for copper-edge/clearance work. It must stop at unsupported geometry rather than generalize this slice or promote any family.

## Self-Check: PASSED

All five Plan 07-04 target artifacts exist, focused/workspace gates pass, exactly one review was remediated once, no viewer/dependency/intelligent-format/private-parser scope entered, and no files are staged or committed.

---
*Phase: 07-decision-grade-dfm-and-assembly-analysis*
*Completed: 2026-08-31*
