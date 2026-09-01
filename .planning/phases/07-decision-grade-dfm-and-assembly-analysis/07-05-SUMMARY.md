---
phase: 07-decision-grade-dfm-and-assembly-analysis
plan: 05
subsystem: dfm-geometry
status: complete
tags: [dfm, copper, clearance, annular-ring, kicad, fixed-point, evidence-only]
requires:
  - phase: 07-decision-grade-dfm-and-assembly-analysis
    plan: 03
    provides: Source/version-bound manufacturing constraints and central all-EvidenceOnly qualification.
  - phase: 07-decision-grade-dfm-and-assembly-analysis
    plan: 04
    provides: Bounded checked outline topology and exact drill/tool semantics.
  - phase: 06-intelligent-interchange-decision-gate
    plan: 08
    provides: No-go closure for ODB++ and IPC-2581 this release.
provides:
  - Exact source-authoritative copper-to-exterior/cutout/non-plated-route distance.
  - Bounded deterministic same-physical-layer proven-different-net copper clearance.
  - Canonical same-object native KiCad PadHoleAssociation and per-layer annular-ring measurement.
  - Project-authored positive, hard-negative, mutation, tie, arithmetic, deadline, and resource evidence for all three families.
affects: [07-06, 07-07, 07-08, 07-09, 07-10, 07-11]
actuals:
  tokens: 32000
  tasks: 3
  commits: 0
tech-stack:
  added: []
  patterns: [exact represented-shape distance, layer-net bucket pruning, same-object native association, adapter-version-bound authority]
key-files:
  created:
    - .planning/phases/07-decision-grade-dfm-and-assembly-analysis/07-05-SUMMARY.md
  modified:
    - crates/ratemypcb-core/src/dfm.rs
    - crates/ratemypcb-core/src/fabrication.rs
    - crates/ratemypcb-core/src/fabrication/native.rs
    - crates/ratemypcb-core/tests/dfm_release.rs
    - crates/ratemypcb-core/tests/fabrication_release.rs
    - tests/fixtures/dfm/manifest.json
    - tests/fixtures/dfm/geometry-targets.json
    - schemas/report-2.0.json
    - .planning/phases/07-decision-grade-dfm-and-assembly-analysis/07-VALIDATION.md
    - .planning/ROADMAP.md
    - .planning/STATE.md
key-decisions:
  - "Copper distance supports exact axis-line capsules and round flashes; unsupported or potentially-nearer inexact geometry remains not_checked rather than approximated."
  - "Clearance candidates are bucketed by physical layer and explicit net identity, then bounded by deterministic spatial pruning and one inherited deadline."
  - "PadHoleAssociation exists only for one plated round-hole native KiCad pad object and carries exact materialized pad/hole geometry, span/layers, stable pad/hole/association/tool IDs, and dual provenance."
  - "Declaration and native authority require exact adapter/version contracts; no package/Gerber/XNC proximity, name, or dimension association is accepted."
patterns-established:
  - "Bounding intervals may prune exact candidates but never serve as final distance truth."
  - "Every no-pair/overload return checks the absolute deadline before it can become Passed."
requirements-completed: [DFM-01, DFM-05]
coverage:
  - id: D1
    description: "Copper-edge reports exact nearest exterior, cutout, or non-plated routed-boundary distance with both source locations and only production declaration authority."
    requirement: DFM-01
    verification:
      - kind: integration
        ref: "crates/ratemypcb-core/tests/dfm_release.rs#copper_edge_*"
        status: pass
      - kind: unit
        ref: "crates/ratemypcb-core/src/dfm.rs#copper_edge_cutout_mutations_order_arithmetic_and_resources_fail_closed"
        status: pass
    human_judgment: false
  - id: D2
    description: "Copper clearance compares only explicit different nets on one physical layer through deterministic bounded layer/net buckets."
    requirement: DFM-01
    verification:
      - kind: integration
        ref: "crates/ratemypcb-core/tests/dfm_release.rs#copper_clearance_*"
        status: pass
      - kind: unit
        ref: "crates/ratemypcb-core/src/dfm.rs#copper_clearance_mutations_order_and_layer_local_resource_bound_fail_closed"
        status: pass
    human_judgment: false
  - id: D3
    description: "Native KiCad same-pad ownership produces canonical PadHoleAssociation facts and exact annular-ring measurements on every applicable layer."
    requirement: DFM-01
    verification:
      - kind: integration
        ref: "crates/ratemypcb-core/tests/dfm_release.rs#annular_ring_*"
        status: pass
      - kind: integration
        ref: "crates/ratemypcb-core/tests/fabrication_release.rs#package_reconciliation_native_parser_is_bounded_explicit_and_quote_safe"
        status: pass
      - kind: unit
        ref: "crates/ratemypcb-core/src/dfm.rs#annular_association_mutations_order_and_deadline_fail_closed"
        status: pass
    human_judgment: false
  - id: D4
    description: "All three families retain positive/hard-negative/mutation corpora, exact metrics, deterministic ties, resource bounds, and EvidenceOnly qualification."
    requirement: DFM-05
    verification:
      - kind: integration
        ref: "crates/ratemypcb-core/tests/dfm_release.rs#drill_outline_geometry_corpus_is_exact_bounded_and_evidence_only"
        status: pass
      - kind: other
        ref: "cargo test --all --locked"
        status: pass
    human_judgment: false
duration: not-measured
completed: 2026-08-31
---

# Phase 7 Plan 05: Exact Copper Distance and Native Annular Ring Summary

**Exact source-bound copper-edge and different-net clearance now share bounded fixed-point geometry, while annular ring uses only authoritative same-object native KiCad pad ownership.**

## Performance

- **Duration:** Not measured — the fresh handoff did not record a start timestamp.
- **Completed:** 2026-08-31T13:01:18Z
- **Tasks:** 3
- **Files modified:** 11 plus this summary

## Accomplishments

- Added `dfm.copper-edge.v1` over exact represented axis-line/round-flash copper and distinct exterior, cutout, and non-plated routed boundaries. Extents are pruning evidence only, never final truth.
- Added `dfm.copper-clearance.v1` over explicit unequal nets on one physical layer, with deterministic layer/net buckets, spatial pruning, checked candidate ceilings, and cooperative deadlines instead of a global pair scan.
- Added the smallest policy-free `PadHoleAssociation` to `FabricationReview`, emitted only from one authoritative plated round-hole native KiCad pad object. `dfm.annular-ring.v1` measures its actual pad and hole boundaries on every applicable layer.
- Extended the project-authored geometry corpus and central output revalidation for all three all-EvidenceOnly families; no dependency, viewer, ODB++/IPC-2581, private parser, or second model entered the plan.

## TDD Evidence

- **Task 1 RED:** both `copper_edge_` integration tests failed because the family coverage did not exist. **GREEN:** exterior exact-threshold/one-resolution cases, routed positive, missing/preset/off-grid authority, cutout, round-flash, mutation, arithmetic, determinism, deadline, and resource checks pass.
- **Task 2 RED:** both `copper_clearance_` tests failed because the family coverage did not exist. **GREEN:** exact threshold/one-resolution violation, same-net, adjacent-layer, missing/off-grid/direct authority, duplicate/partial connectivity, deterministic ties, dense layer/net resource, and no-pair deadline checks pass.
- **Task 3 RED:** `annular_ring_` tests failed because `padHoleAssociations` and family coverage were absent. **GREEN:** native same-pad positive, per-layer findings, NPTH, slot, unsupported pad, missing/off-grid/direct authority, duplicate/dangling association, span/plating, order, and deadline checks pass.
- **Review remediation RED/GREEN:** forged adapter-version tests and an expired single-net no-pair test failed before the one remediation pass and passed afterward.

## Task Commits

No commits were created. The user explicitly prohibited staging, commits, and pushes.

## Files Created/Modified

- `crates/ratemypcb-core/src/dfm.rs` — three exact families, bounded primitives/buckets, source-authority checks, central dispatch/output validation, and focused unit mutations.
- `crates/ratemypcb-core/src/fabrication.rs` — canonical `PadHoleAssociation`, digest/reference/resource/runtime validation, allocation accounting, and generated schema definition.
- `crates/ratemypcb-core/src/fabrication/native.rs` — same-object native round-pad association, exact all-copper-layer expansion, and native/package retention validation.
- `crates/ratemypcb-core/tests/dfm_release.rs` — production authority, boundaries, hard negatives, schema-visible facts, and one-resolution tests.
- `crates/ratemypcb-core/tests/fabrication_release.rs` — association mutation/forgery and exhaustive schema checks.
- `tests/fixtures/dfm/{manifest.json,geometry-targets.json}` — three family corpora and promotion metadata references.
- `schemas/report-2.0.json` — authoritative generated additive association schema.
- `.planning/phases/07-decision-grade-dfm-and-assembly-analysis/07-VALIDATION.md`, `.planning/ROADMAP.md`, `.planning/STATE.md` — Plan 07-05 evidence and Plan 07-06 readiness.

## Decisions Made

- Exact line/circle operations use checked integer picometres. A non-integral candidate may be deferred during pruning, but if it can still beat the exact minimum the family returns `not_checked`.
- Round flashes reuse the existing aperture model; regions, general arcs, non-circular flashes, unresolved transforms/polarity/expansion, and uncertain routes stay `not_checked` rather than gaining a geometry framework.
- Clearance compares opaque explicit net identities only. Names are retained as source facts but never interpreted as electrical intent.
- Association provenance is intentionally dual but identical because pad and hole must originate from the same native object. Package/XNC/Gerber facts never create or repair an association.

## Independent Review

Exactly one bounded fresh-context correctness/security review examined the complete Plan 07-05 implementation and returned `BLOCK` with two valid P1 findings:

1. Layer/net bucket construction and sorting lacked deadline checks, so an expired one-net/no-pair scan could return a false clean result.
2. Declaration/native authority checked adapter names but not exact adapter versions; native association provenance was not explicitly bound to the native adapter/version.

One remediation pass added bucket-build, pre/post-sort, and final-return deadline checks; exact declaration/native adapter-version contracts; native provenance producer/version validation; and regressions for both defects. No second review was run.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Correctness/Availability] Prevented expired no-pair clearance from passing**

- **Found during:** Independent review after Task 3
- **Issue:** Deadline expiry during layer/net bucket construction or sorting could reach the clean no-pair return.
- **Fix:** Added cooperative checks during bucket construction, before/after every sort, and before all return paths.
- **Files modified:** `crates/ratemypcb-core/src/dfm.rs`
- **Verification:** `copper_clearance_mutations_order_and_layer_local_resource_bound_fail_closed`
- **Committed in:** Uncommitted by user instruction.

**2. [Rule 1 - Security/Authority] Bound declaration and native adapter versions**

- **Found during:** Independent review after Task 3
- **Issue:** A self-consistent canonical model could reuse an expected adapter name with a forged version/provenance.
- **Fix:** Required exact declaration/native adapter versions, native document shape, and native association producer/version provenance in runtime and model validation.
- **Files modified:** `crates/ratemypcb-core/src/dfm.rs`, `crates/ratemypcb-core/src/fabrication.rs`, `crates/ratemypcb-core/src/fabrication/native.rs`
- **Verification:** focused forged-adapter unit and native fabrication tests
- **Committed in:** Uncommitted by user instruction.

**Total deviations:** 2 auto-fixed correctness/security defects. **Impact:** fail-closed hardening only; no scope expansion.

## Issues Encountered

- Strict Clippy rejected a nine-argument single-use native association helper; it was reduced to seven arguments by deriving document/provenance identity from the authoritative feature itself.
- Post-handoff independent verification found rustfmt-only differences in `crates/ratemypcb-core/tests/dfm_release.rs`. `cargo fmt --all` changed that file only: import ordering and line wrapping, with no behavior change.
- Lens reports policy false positives for public SHA/family strings and test-only `unwrap`s. Those exact secret findings were marked false-positive; strict Clippy, primary Rust LSP, executable tests, and `git diff --check` are clean.

## Verification

- Focused Plan 07-05 integration: edge 2/2, clearance 2/2, annular 2/2.
- Native fabrication filter: 5/5; full fabrication release: 106/106.
- Full DFM release: 25/25; workspace Rust: 268 tests passed.
- Node report/viewer: 31/31 passed.
- Generated/checked-in report schema is semantic and byte-equal.
- Post-handoff corrective rerun passed edge 3/3, clearance 3/3, annular 3/3 (one unit plus two integration tests each), and native pad-hole association 1/1.
- After the formatting-only correction, `cargo fmt --all -- --check`, strict workspace Clippy, and `git diff --check` passed.
- Primary Rust LSP: five changed Rust files had no primary diagnostic; one pre-existing informational let-chain suggestion remains.
- GSD `query init.execute-phase 07` recognized Plan 07-05 as the next incomplete plan before this summary was written.

## User Setup Required

None.

## Residual Risks

- General copper/profile arcs, regions/zones, non-circular flashes, and other shapes outside the exact bounded subset remain `not_checked`; no approximation or bounding-box answer is substituted.
- Native annular-ring authority currently supports plated round-hole circular pads only. NPTH, slots, unknown plating/span, unsupported pad geometry, or weaker cross-document facts remain `not_checked`.
- All three families have complete project-authored metrics but no reviewed promotion metadata, so every finding remains `EvidenceOnly`.

## Next Phase Readiness

Plan 07-06 may consume the same exact source-bound geometry and authority patterns for mask sliver and paste/mask work. It must not broaden unsupported shapes by approximation or promote any family.

## Handoff

- **Workspace:** `wks_4c340b66223c27bd`
- **Worktree:** `/Users/mattiafiumara/.paseo/worktrees/3s4r2ob6/phase7-dfm-assembly`
- **Paseo agent:** `661dd0c4-3f97-4b21-af04-5e5c5691e568`
- **Pi session:** `01a05784-5126-777b-8efa-9ba234139bd4`
- **Base/HEAD:** `5e0fa62a5865cdea1a7755c6bedcedab3a64ba07` unchanged
- **Git boundary:** no files staged; no commit or push created
- **Next action:** start a fresh sole Plan 07-06 lead; Plan 07-06 was not started here

## Self-Check: PASSED

All planned artifacts exist, the post-handoff formatting correction and focused/workspace/schema/Node/fmt/Clippy/LSP/diff gates pass, exactly one fresh review was remediated once, accepted Plans 07-01..07-04 remain preserved, and no files are staged or committed.

---
*Phase: 07-decision-grade-dfm-and-assembly-analysis*
*Completed: 2026-08-31*
