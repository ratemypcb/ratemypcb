---
phase: 07-decision-grade-dfm-and-assembly-analysis
plan: 06
subsystem: dfm-mask-paste
status: complete
tags: [dfm, solder-mask, paste, x2, fitted-state, fixed-point, evidence-only]
requires:
  - phase: 07-decision-grade-dfm-and-assembly-analysis
    plan: 03
    provides: Source/version-bound normalized mask and paste relationship thresholds.
  - phase: 07-decision-grade-dfm-and-assembly-analysis
    plan: 05
    provides: Checked exact represented-shape distance, deterministic bounds, and central fabrication-family revalidation.
  - phase: 06-intelligent-interchange-decision-gate
    plan: 08
    provides: No-go closure for ODB++ and IPC-2581 this release.
provides:
  - Exact bounded negative-mask opening sliver with source-linked opening and intent evidence.
  - Exact per-fitted-SMD-pad concentric round paste/mask set relation, radial expansion/reduction, side, and dual provenance.
  - Complete eight-family DFM-01 corpus, mutation, metric, qualification, and non-required-coverage audit.
  - Policy-free retention of standards-valid multi-value X2 aperture-function facts needed for positive SMD authority.
affects: [07-07, 07-08, 07-09, 07-10, 07-11]
actuals:
  tokens: 38000
  tasks: 2
  commits: 0
tech-stack:
  added: []
  patterns: [negative-layer resolved openings, explicit fitted-state join, positive SMD aperture authority, exact concentric set relation]
key-files:
  created:
    - .planning/phases/07-decision-grade-dfm-and-assembly-analysis/07-06-SUMMARY.md
  modified:
    - crates/ratemypcb-core/src/dfm.rs
    - crates/ratemypcb-core/src/fabrication.rs
    - crates/ratemypcb-core/src/lib.rs
    - crates/ratemypcb-core/tests/dfm_release.rs
    - tests/fixtures/dfm/manifest.json
    - tests/fixtures/dfm/geometry-targets.json
    - .planning/phases/07-decision-grade-dfm-and-assembly-analysis/07-VALIDATION.md
    - .planning/ROADMAP.md
    - .planning/STATE.md
key-decisions:
  - "Mask sliver accepts only explicit negative board mask layers, exact dark top-level line/round-flash openings, and one component/pad intent per opening; overlap or multi-pad association without deliberate merge authority is not_checked."
  - "Paste/mask accepts only exact concentric round openings with standards-valid X2 SMDPad aperture authority, one matching component/pad association, explicit schematic dnp=false, and one matching placement side."
  - "A placement is side evidence, never fitted-state authority; absence from PadHoleAssociation is never proof that a pad is SMD."
  - "All eight DFM-01 families have complete corpora and remain EvidenceOnly; none entered required coverage."
patterns-established:
  - "Positive exception authority is required: absence of a through-hole association cannot establish a surface-mount pad."
  - "Production DFM families run after typed schematic normalization when a geometry claim requires fitted-state evidence."
requirements-completed: [DFM-01, DFM-05]
coverage:
  - id: D1
    description: "Mask sliver measures only actual exact negative-layer openings with represented single-pad intent and production declaration authority."
    requirement: DFM-01
    verification:
      - kind: unit
        ref: "crates/ratemypcb-core/src/dfm.rs#mask_sliver_exact_boundaries_intent_order_and_resources_fail_closed"
        status: pass
      - kind: integration
        ref: "crates/ratemypcb-core/tests/dfm_release.rs#mask_sliver_layer_presence_without_resolved_polarity_or_intent_is_not_checked"
        status: pass
    human_judgment: false
  - id: D2
    description: "Paste/mask reports exact per-fitted-SMD-pad set relation and radial expansion/reduction with geometry, intent, placement, fitted-state, and threshold provenance."
    requirement: DFM-01
    verification:
      - kind: unit
        ref: "crates/ratemypcb-core/src/dfm.rs#paste_mask_exact_set_relationship_boundaries_and_mutations_fail_closed"
        status: pass
      - kind: integration
        ref: "crates/ratemypcb-core/tests/dfm_release.rs#paste_mask_layer_presence_without_fitted_pad_authority_is_not_checked"
        status: pass
    human_judgment: false
  - id: D3
    description: "All eight DFM-01 families retain positive, hard-negative, unsupported, fourteen-mutation, exact-metric, EvidenceOnly, and non-required-coverage records."
    requirement: DFM-05
    verification:
      - kind: integration
        ref: "crates/ratemypcb-core/tests/dfm_release.rs#dfm01_family_matrix_is_complete_qualified_and_not_required"
        status: pass
      - kind: other
        ref: "cargo test --workspace --locked"
        status: pass
    human_judgment: false
duration: not-measured
completed: 2026-08-31
---

# Phase 7 Plan 06: Exact Mask Sliver and Fitted-Pad Paste/Mask Summary

**Mask sliver now uses only actual resolved negative-mask openings, while paste/mask comparison requires positive SMD, fitted-state, placement-side, and per-pad geometry authority.**

## Performance

- **Duration:** Not measured.
- **Completed:** 2026-08-31T14:43:03Z
- **Tasks:** 2
- **Production/fixture files modified:** 6 plus planning receipts

## Accomplishments

- Added `dfm.mask-sliver.v1` over exact same-physical-layer axis-line/round-flash openings on explicitly negative board solder-mask layers. Exact threshold, one-resolution violation/safe, deterministic ties, single-opening no-pair, overlap, unsupported shape/polarity/intent, deadline, arithmetic, and resource cases are explicit.
- Added `dfm.paste-mask-relationship.v1` over exact concentric round mask/paste geometry joined by component/pad identity, standards-valid X2 `SMDPad` aperture authority, explicit schematic `dnp=false`, and exact placement side. Output names equal/subset/superset set relation, radial expansion/reduction, both geometry and intent locations, placement provenance, fitted-state occurrence, and threshold provenance.
- Retained standards-valid multi-value X2 aperture-function facts in the existing policy-free adapter instead of inferring SMD from absent holes, names, or layer presence.
- Closed the geometry corpus at exactly eight DFM-01 family/versions. Mask reports TP=2/TN=3; paste/mask reports TP=2/TN=3; both have zero FP/FN, precision/recall 1.000, fourteen fail-closed mutations, and no reviewed promotion. The report required-evidence set remains the prior exact ten checks and contains no DFM/assembly/inference family.

## TDD Evidence

- **Task 1 RED:** `mask_sliver_` integration coverage did not exist and the eight-family corpus expected two absent rows. **GREEN:** exact sliver/unit boundary and mutation test, production layer-presence negative, and corpus/matrix checks pass.
- **Task 2 RED:** `paste_mask_` integration coverage did not exist. **GREEN:** exact equal/expansion/reduction boundaries, direct/preset/missing/duplicate/off-grid authority, unknown fitted/DNP/side, missing association, omission, windowpane, pin-in-paste, unsupported shape, ordering, deadline, and EvidenceOnly checks pass.
- Every threshold-bearing positive and exact-boundary fixture is created through the real `DfmDeclarations::from_json` normalizer. Directly mutated or anonymous constraints are used only as fail-closed negative cases.
- **Review remediation RED/GREEN:** placement-with-DNP/unknown-fitted, multi-pad mask opening without merge authority, and absent positive SMD authority regressions fail closed after the one remediation pass.

## Task Commits

No commits were created. The user prohibited staging, commits, and pushes.

## Files Created/Modified

- `crates/ratemypcb-core/src/dfm.rs` — two bounded exact families, negative-layer primitive/index logic, explicit fitted/SMD authority, central dispatch/revalidation, qualification receipts, and focused unit mutations.
- `crates/ratemypcb-core/src/fabrication.rs` — policy-free retention of standards-valid multi-value X2 aperture-function facts.
- `crates/ratemypcb-core/src/lib.rs` — runs fabrication DFM after typed schematic normalization and revalidates against both canonical models.
- `crates/ratemypcb-core/tests/dfm_release.rs` — production authority/layer-presence controls and exact eight-family required-coverage audit.
- `tests/fixtures/dfm/{manifest.json,geometry-targets.json}` — mask/paste source contracts, adjudicated cases, mutations, and qualification digests.
- Planning receipts — Plan 07-06 completion and Plan 07-07 readiness.

## Decisions Made

- A physical opening may carry one component/pad association. Multiple associations do not prove deliberate merge/override intent, so the family returns `not_checked`. A single actual opening can cleanly have no pair; separate overlapping openings cannot.
- Paste/mask comparison supports only concentric round flashes. General lines, offsets, polygons, regions, arcs, transforms, and windowpanes remain `not_checked` rather than gaining approximate set geometry.
- Placement establishes side only. Explicit high-confidence typed schematic `dnp=false`, with no fitted-state reconciliation conflict, establishes fitted state.
- Positive X2 `SMDPad` aperture-function authority establishes non-through-hole applicability. Missing/partial SMD authority and explicit pad-hole ownership both keep pin-in-paste cases `not_checked`.

## Independent Review

Exactly one fresh bounded correctness/security review ran as subagent `85a70298-8ba8-46d6-8b08-14ef58917389` under mission `05e881e2-052f-4097-9d75-c3c83b54f817`. It returned `BLOCK` with three valid P1 findings:

1. `AssemblyPlacement` had been treated as fitted-state proof despite carrying no fitted/DNP field.
2. Multiple component/pin associations on one mask feature had been treated as represented deliberate merge intent.
3. Absence from the supported `PadHoleAssociation` subset had been treated as proof that a pad was not pin-in-paste.

One remediation pass required typed explicit `dnp=false`, rejected unrepresented multi-pad merge intent, and required positive X2 `SMDPad` authority while retaining explicit through-hole contradiction checks. Focused regressions pass. No second review ran.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Correctness] Separated placement side from fitted state**

- **Found during:** The one independent review
- **Issue:** A placement could belong to a DNP or unknown-fitted component.
- **Fix:** DFM execution now receives validated typed schematic evidence; each compared component requires explicit high-confidence `dnp=false` and no fitted-state reconciliation conflict.
- **Files:** `crates/ratemypcb-core/src/{dfm.rs,lib.rs}`
- **Verification:** DNP and missing-DNP regressions in the paste/mask unit test.

**2. [Rule 1 - Correctness] Rejected inferred merged-opening intent**

- **Found during:** The one independent review
- **Issue:** Component/pin association describes ownership, not deliberate mask merge/override policy.
- **Fix:** Every physical mask opening must have one pad intent; multi-pad and overlapping separate openings are `not_checked`.
- **Files:** `crates/ratemypcb-core/src/dfm.rs`, geometry corpus
- **Verification:** multi-pad opening regression returns `NotRun`, while a true single-opening no-pair case remains clean.

**3. [Rule 1 - Authority] Required positive SMD authority**

- **Found during:** The one independent review
- **Issue:** An absent supported hole association cannot prove a pad is surface mount.
- **Fix:** Retained standards-valid multi-value X2 aperture-function facts and required exact `SMDPad` authority for both compared openings; explicit pad-hole identity remains a pin-in-paste uncertainty.
- **Files:** `crates/ratemypcb-core/src/{fabrication.rs,dfm.rs}`, tests and corpus
- **Verification:** no-SMD/empty-association and explicit pin-in-paste regressions return `NotRun`.

**Total deviations:** 3 review-driven fail-closed remediations. **Impact:** stricter authority and one minimal policy-free X2 normalization correction; no dependency, DTO, schema, viewer, or format expansion.

## Verification

- Focused: mask unit 1/1, paste/mask unit 1/1, mask integration 1/1, paste/mask integration 1/1, DFM-01 matrix 1/1.
- Full DFM release: 28/28; fabrication release: 106/106.
- Full workspace: 273 Rust tests passed; Node report/viewer: 31/31 passed.
- `cargo fmt --all -- --check`, strict workspace Clippy, and `git diff --check` passed.
- Primary Rust LSP completed on all four changed Rust files with no error/warning; only pre-existing informational let-chain/typo suggestions appeared.
- Lens found no Plan 07-06 production correctness issue; its static family-string secret matches were marked false-positive, while broad accepted test-only `unwrap` and pre-existing duplication policy findings remain scanner noise. Strict Clippy and executable security/resource gates are clean.
- No public DTO changed, and the existing generated-schema equality test passed within the fabrication/workspace suites.

## User Setup Required

None.

## Residual Risks

- Current package adapters do not infer negative mask polarity from a solder-mask layer role. Role/layer presence alone therefore stays `not_checked` until a source explicitly represents negative polarity.
- Production package assembly placement facts remain absent today; paste/mask stays `not_checked` until an authoritative placement side and typed fitted state coexist with exact SMD pad geometry.
- Mask sliver supports exact axis-aligned lines and round flashes; paste/mask supports exact concentric round flashes only. Other shapes and deliberate merge/tent/override/omission/windowpane/pin-in-paste policies remain `not_checked`.
- All eight DFM-01 families have complete project-authored metrics but no reviewed promotion metadata, so every finding remains `EvidenceOnly`.

## Next Phase Readiness

Plan 07-07 may begin construction/order confirmation using the existing source-bound declaration seam. It must not treat mask/paste residual unsupported states as construction defaults or promote any DFM family.

## Handoff

- **Workspace:** `wks_4c340b66223c27bd`
- **Worktree:** `/Users/mattiafiumara/.paseo/worktrees/3s4r2ob6/phase7-dfm-assembly`
- **Paseo agent:** `661dd0c4-3f97-4b21-af04-5e5c5691e568`
- **Pi session:** `01a05784-5126-777b-8efa-9ba234139bd4`
- **Review run:** `85a70298-8ba8-46d6-8b08-14ef58917389`
- **Base/HEAD:** `5e0fa62a5865cdea1a7755c6bedcedab3a64ba07` unchanged
- **Git boundary:** no files staged; no commit or push created
- **Next action:** start a fresh sole Plan 07-07 lead; Plan 07-07 was not started here

## Self-Check: PASSED

All Plan 07-06 artifacts exist, all requested gates pass, exactly one fresh review was remediated once, accepted Plans 07-01..07-05 remain preserved, and no files are staged or committed.

---
*Phase: 07-decision-grade-dfm-and-assembly-analysis*
*Completed: 2026-08-31*
