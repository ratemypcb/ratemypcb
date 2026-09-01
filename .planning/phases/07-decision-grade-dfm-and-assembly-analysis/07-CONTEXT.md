# Phase 7: Decision-Grade DFM and Assembly Analysis - Context

**Gathered:** 2026-08-30
**Status:** Plan 07-02 complete; Phase 6 no-go reconciled; Plan 07-03 unblocked

<domain>
## Phase Boundary

Deliver DFM-01 through DFM-06 by turning validated canonical fabrication, construction, connectivity, BOM, schematic-occurrence, and placement facts into measured source-linked findings and deterministic release-unblocking actions. Phase 7 owns analyzer and qualification policy in core; it does not own format parsing, a second board model, a second approval engine, or viewer-side policy.

Plan 07-02 is complete against the accepted Phase 5 baseline: existing typed schematic reconciliation plus native KiCad and Gerber/X2+Excellon. Exact external Phase 6 receipts `06-01-SUMMARY.md` and `06-08-SUMMARY.md` record no-go for both ODB++ and IPC-2581 this release, FMT-03/FMT-05 complete, FMT-04 not applicable, and Phase 6 complete at 8/8. Plan 07-03 is therefore unblocked without adding intelligent-format integration or parity. Any inference family still requires an explicit family-specific human promotion decision.

</domain>

<decisions>
## Implementation Decisions

### Canonical model and policy ownership

- **D-01:** Consume the existing `FabricationReview`, fixed-point geometry, `ManufacturingConstraint`, `ConstructionEvidence`, `AnalyzerRequirements`/`dispatch_analyzer`, `GateImpact`, typed schematic reconciliation output, evidence finalization, and report validator. One optional bounded local declaration seam may normalize explicit source/version-bound profile, project-rule, and customer order/profile authority into those existing fixed-point contracts with provenance; it must not create another board model, capability dispatcher, evidence contract, or approval engine.
- **D-02:** Adapters remain policy-free and the viewer remains a renderer. Input normalization, analyzer prerequisite checks, qualification, gate impact, and release-action ranking live in core.

### Dependency and honesty gates

- **D-03:** Phase 6 is complete at 8/8 with human no-go for both ODB++ and IPC-2581 this release. FMT-03 and FMT-05 are complete; FMT-04 is not applicable because no adapter was adopted. Phase 7 continues on native KiCad plus Gerber/X2+Excellon, adds no intelligent-format integration/parity/support claim, and treats PRIVATE parser SHA `a4216f6909754155555e9290c2ec84e0eb16d267` as research only.
- **D-04:** Deterministic and inference families stay separate. Missing, duplicate, partial, stale, failed, omitted, unsupported, unknown, or otherwise unqualified prerequisites yield `not_checked`/evidence-only output, never pass or blocking impact. Anonymous preset `f64` values are not Phase 7 threshold authority; a production threshold must come through the bounded source/version-bound seam. Order/profile facts not representable by existing `ConstructionEvidence`/`ManufacturingConstraint` fields produce confirmation gaps only, with their match/conflict subfamily deferred.
- **D-05:** Do not infer physical package compatibility from distributor order packaging, component paste coverage from paste-layer presence, construction defaults from common practice, or electrical intent from net names.

### Minimal analyzer and fixture shape

- **D-06:** Use one small plain core module for the format-independent tracer plus focused project-authored fixtures. Start with one production-quality deterministic family by mapping the existing typed occurrence-first schematic reconciliation output; do not duplicate population/fitted/DNP/quantity/placement/footprint/revision comparisons or their identity fallback semantics in `dfm.rs`. Intelligent-format expansion is omitted under D-03's no-go; do not build a plugin framework or all-analyzers-at-once task.
- **D-07:** Every family receives stable identity, explicit capability prerequisites, positive/hard-negative/mutation cases, TP/FP/FN/TN, precision, and recall. A deterministic family may block only with non-undefined precision at or above the existing 95% policy, green fail-closed mutations, and a reviewed promotion entry. Recall is reported; no threshold is invented.

### Promotion and release actions

- **D-08:** Inference families default to `EvidenceOnly` and cannot become blocking without the explicit family-specific human checkpoint in `07-PROMOTION-CHECKPOINT.md`. No inference family is currently approved.
- **D-09:** Core ranks release unblocks without score: incomplete required evidence first, then qualified blocking findings, then evidence-only/inference attention. Keep assessment prose and the existing three-action cap; validate that P1 references the top core-ranked unblock set instead of adding a second recommendation engine.

### the agent's Discretion

- Exact family ordering inside deterministic ties, fixture filenames, and whether the single plain module is named `dfm.rs` or `assembly.rs`, provided the final plan minimizes files and preserves the boundaries above.

</decisions>

<canonical_refs>

## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Product scope and dependency state

- `.planning/PROJECT.md` — milestone value, local-first boundary, and no-plugin-framework preference.
- `.planning/ROADMAP.md` § Phase 7 — goal, dependencies, success criteria, and inference checkpoint.
- `.planning/REQUIREMENTS.md` § Advanced DFM and Assembly Decisions — DFM-01 through DFM-06 acceptance scope.
- `.planning/STATE.md` — reconciled Phase 6 no-go and current Phase 7 execution state.
- `/Users/mattiafiumara/.paseo/worktrees/3s4r2ob6/phase6-interchange-decision/.planning/phases/06-intelligent-interchange-decision-gate/06-01-SUMMARY.md` — exact human FMT-03 no-go receipt.
- `/Users/mattiafiumara/.paseo/worktrees/3s4r2ob6/phase6-interchange-decision/.planning/phases/06-intelligent-interchange-decision-gate/06-08-SUMMARY.md` — exact FMT-04 N/A, FMT-05 complete, Phase 6 8/8 closure receipt.

### Phase 5 contracts

- `.planning/phases/05-manufacturing-evidence-model-and-gerber-baseline/05-CONTEXT.md` — canonical model, capability, fixed-point, reconciliation, and policy boundaries.
- `.planning/phases/05-manufacturing-evidence-model-and-gerber-baseline/05-RESEARCH.md` — adapter/model architecture, resource limits, and validation patterns.
- `.planning/phases/05-manufacturing-evidence-model-and-gerber-baseline/05-VERIFICATION.md` — accepted Phase 5 product evidence and residual boundaries.
- `crates/ratemypcb-core/src/fabrication.rs` — `FabricationReview`, canonical facts, `CapabilityLedger`, `AnalyzerRequirements`, and `dispatch_analyzer`.
- `crates/ratemypcb-core/src/fabrication/native.rs` — native KiCad facts and symmetric package reconciliation.
- `crates/ratemypcb-core/src/schematic.rs` — occurrence-first BOM/placement reconciliation and evidence-only precedent.
- `crates/ratemypcb-core/src/lib.rs` — BOM/placement review, `GateImpact`, evidence finalization, approval, report/assessment validation, and production review integration.
- `crates/ratemypcb-core/tests/fabrication_release.rs` — canonical capability, mutation, provenance, and deterministic fixture patterns.
- `crates/ratemypcb-core/tests/schematic_release.rs` — cross-artifact occurrence and mutation patterns.

### Prior research inputs (verify against current code)

- `/tmp/ratemypcb-parallel-prep.phase7-bom-assembly.md` — stale-status assembly seam inventory.
- `/tmp/ratemypcb-parallel-prep.phase7-dfm.md` — stale-status geometry/construction/inference seam inventory.

</canonical_refs>

<code_context>

## Existing Code Insights

### Reusable Assets

- `FabricationReview` already holds fixed-point features, layer roles/order, tools, profile, connectivity, construction, constraints, assembly evidence, provenance, omissions, conflicts, and reconciliation.
- `dispatch_analyzer` already fails closed when any prerequisite is missing, non-complete, or duplicated, and cannot invent a pass without a semantic result.
- `finalize_evidence`, `required_evidence_summary`, `approval_eligible`, `validate_report`, and `validate_assessment` already own stable evidence and approval integrity.
- Schematic reconciliation already compares occurrence-first population, fitted/DNP, value, footprint, quantity, placement, and revision facts and exposes typed `SchematicReview.mismatches` without promoting the family. Phase 7 maps that output to family qualification/evidence; it does not rerun those comparisons.

### Established Patterns

- Production manufacturing parsing and native/package reconciliation are active at current HEAD; the prior prep notes describing a legacy-only runtime are stale.
- Canonical `AssemblyEvidence` exists, but production currently populates mask/paste layer IDs only; `placements` remains empty and no `CapabilityId::Assembly` record is emitted.
- BOM and placement reviews are still broad-source checks; placement recognizes columns but does not validate side/rotation values or retain row/field provenance.
- Existing report policy separates score, required evidence, blocking impact, and approval. Phase 7 extends that contract rather than replacing it.

### Integration Points

- One optional local CLI input is parsed once, bounded and source-linked, then normalized at the core convergence point into existing `FabricationReview.constraints` and representable `FabricationReview.construction` facts before any threshold- or order-dependent family runs. Missing authority remains `not_checked`; unrepresented order facts remain confirmation gaps.
- Pure analyzers consume validated `FabricationReview` plus the existing typed `SchematicReview` reconciliation output after manufacturing/native reconciliation.
- Analyzer outputs join findings/coverage before `finalize_evidence`; report validation remains the final authority.
- CLI/viewer changes, if any, render core-provided state and never recompute qualification, promotion, or action order.

</code_context>

<specifics>
## Specific Ideas

- Preserve measured values and thresholds in fixed-point units with provenance to both compared facts; production geometry thresholds must originate from the bounded source/version-bound profile/project-rule input rather than fixture injection or anonymous presets.
- Normalize only customer order/profile facts already representable by `ConstructionEvidence`/`ManufacturingConstraint`; treat absent or unrepresented facts as confirmation gaps, not fabricated matches, conflicts, violations, or passes.
- Prefer the smaller DFM-06 contract: core computes acceptable top-unblock evidence references and assessment P1 must address that set.
- The first executable slice after dependency clearance should prove one complete deterministic family end to end before expanding analyzer breadth.

</specifics>

<deferred>
## Deferred Ideas

- Physical package-to-land-pattern compatibility remains `not_checked` until an authoritative package/qualified-footprint source is approved; distributor tape/reel/tray fields are never that authority.
- ODB++/IPC-2581 integration and parity fixtures are omitted this release under the Phase 6 no-go. Reopening requires separate authorization and new evidence; no private parser or corpus is copied into Phase 7.
- Inference-family blocking promotion waits for per-family evidence and explicit human approval; no blanket promotion is allowed.
- Release publication, broad fuzz/performance certification, and final skill adoption remain Phase 8.

</deferred>

---

*Phase: 07-decision-grade-dfm-and-assembly-analysis*
*Context gathered: 2026-08-30*
