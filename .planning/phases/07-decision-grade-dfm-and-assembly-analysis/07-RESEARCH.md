# Phase 7: Decision-Grade DFM and Assembly Analysis - Research

<user_constraints>

## User Constraints (from CONTEXT.md)

### Locked Decisions

- **D-01:** Consume the existing `FabricationReview`, fixed-point geometry, `AnalyzerRequirements`/`dispatch_analyzer`, `GateImpact`, BOM/schematic occurrence facts, evidence finalization, and report validator. Do not create another board model, capability dispatcher, evidence contract, or approval engine.
- **D-02:** Adapters remain policy-free and the viewer remains a renderer. Analyzer prerequisite checks, qualification, gate impact, and release-action ranking live in core.
- **D-03:** Exact external Phase 6 receipts record no-go for both ODB++ and IPC-2581 this release, FMT-03/FMT-05 complete, FMT-04 N/A, and 8/8 closure. Phase 7 continues on native KiCad plus Gerber/X2+Excellon; no intelligent-format integration, parity, private parser/corpus use, or support claim is authorized.
- **D-04:** Deterministic and inference families stay separate. Missing, duplicate, partial, stale, failed, omitted, unsupported, unknown, or otherwise unqualified prerequisites yield `not_checked`/evidence-only output, never pass or blocking impact.
- **D-05:** Do not infer physical package compatibility from distributor order packaging, component paste coverage from paste-layer presence, construction defaults from common practice, or electrical intent from net names.
- **D-06:** Use one small plain core module plus focused project-authored fixtures. The completed Phase 6 no-go clears the native KiCad plus Gerber/X2+Excellon execution path; start with one production-quality deterministic tracer family, then expand without a plugin framework or all-analyzers-at-once task.
- **D-07:** Every family receives stable identity, explicit capability prerequisites, positive/hard-negative/mutation cases, TP/FP/FN/TN, precision, and recall. A deterministic family may block only with non-undefined precision at or above the existing 95% policy, green fail-closed mutations, and a reviewed promotion entry. Recall is reported; no threshold is invented.
- **D-08:** Inference families default to `EvidenceOnly` and cannot become blocking without the explicit family-specific human checkpoint in `07-PROMOTION-CHECKPOINT.md`. No inference family is currently approved.
- **D-09:** Core ranks release unblocks without score: incomplete required evidence first, then qualified blocking findings, then evidence-only/inference attention. Keep assessment prose and the existing three-action cap; validate that P1 references the top core-ranked unblock set instead of adding a second recommendation engine.

### the agent's Discretion

- Exact family ordering inside deterministic ties, fixture filenames, and whether the single plain module is named `dfm.rs` or `assembly.rs`, provided the final plan minimizes files and preserves the boundaries above.

### Deferred Ideas (OUT OF SCOPE)

- Physical package-to-land-pattern compatibility remains `not_checked` until an authoritative package/qualified-footprint source is approved; distributor tape/reel/tray fields are never that authority.
- Intelligent-format integration/parity fixtures are omitted this release under Phase 6 no-go; reopening requires separate authorization and new evidence.
- Inference-family blocking promotion waits for per-family evidence and explicit human approval; no blanket promotion is allowed.
- Release publication, broad fuzz/performance certification, and final skill adoption remain Phase 8.
</user_constraints>

**Researched:** 2026-08-30
**Domain:** Capability-gated PCB fabrication geometry, construction reconciliation, assembly correlation, analyzer qualification, and release-action policy
**Confidence:** HIGH for HEAD seams and dependency state; MEDIUM for per-family engineering policy until exact profile/order authority and corpora are adjudicated

<phase_requirements>

## Phase Requirements

| ID | Description | Research Support |
| --- | --- | --- |
| DFM-01 | Shared geometry analyzers report measured clearance, annular ring, copper-to-edge, mask sliver, paste/mask relationship, outline, and drill/tool issues only when required capabilities are present. | Exact family prerequisites, measurement rules, unsupported-shape closure, and source-link requirements below. [VERIFIED: .planning/REQUIREMENTS.md:74] |
| DFM-02 | Stackup, thickness/material, drill-span, impedance/special-process, and profile/order requirements show evidence, conflicts, and explicit confirmation gaps without fabricated defaults. | Reconciliation-only construction taxonomy and explicit-gap behavior below. [VERIFIED: .planning/REQUIREMENTS.md:75] |
| DFM-03 | Assembly analyzers cover placement/BOM population, side/rotation, paste availability, courtyard/access/test-point risks, and package/footprint consistency with source-linked locations. | Current BOM/schematic seams, missing placement provenance, assembly prerequisites, and bounded deterministic/inference split below. [VERIFIED: .planning/REQUIREMENTS.md:76] |
| DFM-04 | Net-aware return-path, high-current, creepage, differential, thermal, and interface checks declare assumptions/capabilities and remain inference-labeled unless deterministic evidence is validated. | Intent-input inventory and default `EvidenceOnly`/`not_checked` taxonomy below. [VERIFIED: .planning/REQUIREMENTS.md:77] |
| DFM-05 | Each analyzer family has adjudicated positive/hard-negative/mutation fixtures and reports precision/recall; only families meeting policy can block release. | Target-level confusion math, mutation gate, manifest contract, and promotion sequence below. [VERIFIED: .planning/REQUIREMENTS.md:78] |
| DFM-06 | Category/disposition actions prioritize the smallest release-unblocking fix and never let an analyzer score override missing required evidence or the approval gate. | Core-ranked unblock-set algorithm and assessment-P1 validation below. [VERIFIED: .planning/REQUIREMENTS.md:79] |
</phase_requirements>

## Summary

HEAD has completed Phase 5 production Gerber/X2, XNC, package, native-KiCad, and symmetric reconciliation integration; the two prep notes are therefore useful seam inventories but stale where they describe a legacy-only runtime or incomplete Phase 5. Production now enters `analyze_manufacturing_inventory_with_deadline`, falls back to legacy only on semantic failure, parses native KiCad manufacturing facts, and reconciles native/package evidence before final validation. [VERIFIED: crates/ratemypcb-core/src/lib.rs:4346-4529] External receipts `06-01-SUMMARY.md` and `06-08-SUMMARY.md` now record Phase 6 no-go for both intelligent formats, FMT-03/FMT-05 complete, FMT-04 N/A, and 8/8 closure; Plan 07-03 is unblocked on the existing baseline.

The first production tracer, `assembly.population-parity.v1`, is complete. It maps the existing typed occurrence-first `SchematicReview.mismatches` output and preserves its population/fitted/DNP/quantity/placement/footprint/revision comparison and identity-fallback semantics; `dfm.rs` adds only family qualification and evidence mapping. It remains evidence-only until its family metrics and reviewed promotion entry qualify it. This is smaller and less error-prone than either duplicating reconciliation or beginning with polygon offset/intersection code, while still proving the complete capability → evidence → qualification → gate-impact → action path. [VERIFIED: crates/ratemypcb-core/src/schematic.rs:1990-2340] [VERIFIED: crates/ratemypcb-core/tests/schematic_release.rs:397-489]

Geometry should then expand one family at a time. No universal PCB numeric limits should be invented: compare exact fixed-point measurements only to an explicit, source/version-bound project, fabricator-profile, or order requirement. KiCad itself resolves clearances from board minima, net classes, and custom rules, while fabricator guidance is process-specific. [CITED: https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html#clearance-and-constraint-resolution] [CITED: https://www.eurocircuits.com/pcb-classification-drill-class/]

**Primary recommendation:** Continue with Plan 07-03's bounded source/version-bound profile/project/order input seam on native KiCad plus Gerber/X2+Excellon. Keep every family EvidenceOnly unless its own qualification/promotion gate passes; omit ODB++/IPC-2581 integration and parity under the recorded no-go.

## Architectural Responsibility Map

| Capability | Primary tier | Secondary tier | Rationale |
| --- | --- | --- | --- |
| Format parsing and canonical facts | Existing fabrication/native adapters | Core model validation | Adapters already emit policy-free facts, capability states, omissions, conflicts, and provenance. [VERIFIED: crates/ratemypcb-core/src/fabrication.rs:1430-1457] |
| Profile/project/order authority input | One bounded local CLI→core normalizer | Existing `ManufacturingConstraint`/`ConstructionEvidence` | Parse once, retain source/version/digest/location, convert exact declared units to fixed point, and normalize only represented facts; missing or unrepresented authority fails closed. |
| Prerequisite gating | Core analyzer dispatch | Analyzer-local represented-shape guard | `dispatch_analyzer` already rejects absent, non-complete, or duplicate prerequisites and cannot invent a pass from `None`. [VERIFIED: crates/ratemypcb-core/src/fabrication.rs:1520-1554] |
| DFM/assembly measurement | One plain core `dfm.rs` module | Existing fixed-point canonical types | Policy and measurements must not enter adapters or JavaScript. [VERIFIED: .planning/phases/07-decision-grade-dfm-and-assembly-analysis/07-CONTEXT.md:15-35] |
| Qualification and promotion | Core family policy plus project-authored manifest | Existing `07-PROMOTION-CHECKPOINT.md` human record | Unknown/unreviewed families default evidence-only; no second approval engine. [VERIFIED: .planning/phases/07-decision-grade-dfm-and-assembly-analysis/07-PROMOTION-CHECKPOINT.md:11-27] |
| Evidence identity and approval | Existing `finalize_evidence`, report validator, and approval recomputation | Assessment validator | Every occurrence already has one canonical evidence record and approval is recomputed independently of score. [VERIFIED: crates/ratemypcb-core/src/lib.rs:3484-3693] [VERIFIED: crates/ratemypcb-core/src/lib.rs:4066-4215] |
| Release-unblock ranking | Core report/assessment validation | CLI/viewer rendering only | P1 must intersect core’s top evidence-ref set; the viewer must not rank or infer. [VERIFIED: .planning/phases/07-decision-grade-dfm-and-assembly-analysis/07-CONTEXT.md:37-43] |
| Presentation | Existing CLI and viewer | Skill wording | Presentation consumes core-provided order and links without recomputation. [VERIFIED: skills/review-pcb-dfm/SKILL.md:49-66] |

## Prep Notes Verified Against HEAD

| Finding | Severity | HEAD verdict |
| --- | --- | --- |
| `/tmp/ratemypcb-parallel-prep.phase7-bom-assembly.md` says `manufacturing_review()` still runs legacy inventory. | medium (stale research input) | Refuted at HEAD: production semantic inventory runs first and legacy is only a fail-closed fallback. [VERIFIED: crates/ratemypcb-core/src/lib.rs:4346-4374] |
| The BOM/assembly note says XNC/Job/package/native-package closure fixtures are absent. | medium (stale research input) | Refuted at HEAD: checked-in `tests/fixtures/fabrication/{xnc,job,package}` manifests and mutations exist; Phase 5 verification records 226 locked Rust tests and accepted native/package reconciliation. [VERIFIED: .planning/phases/05-manufacturing-evidence-model-and-gerber-baseline/05-VERIFICATION.md:15-26] |
| The BOM/assembly note says placement recognizes columns but does not validate side/rotation values or retain rows. | high (current implementation gap) | Confirmed: `placement_review` finds the five headers but only collects references; no X/Y/rotation/side value is parsed. [VERIFIED: crates/ratemypcb-core/src/lib.rs:1801-1901] |
| The BOM/assembly note says occurrence-first reconciliation is richer but evidence-only. | high (promotion boundary) | Confirmed: BOM population/value/footprint/quantity/fitted, placement population/value, and revision mismatches are emitted with `GateImpact::EvidenceOnly`. [VERIFIED: crates/ratemypcb-core/src/schematic.rs:1962-1987] [VERIFIED: crates/ratemypcb-core/src/schematic.rs:2203-2340] |
| `/tmp/ratemypcb-parallel-prep.phase7-dfm.md` says production integration remains legacy and FAB-03..08 are incomplete. | medium (stale research input) | Refuted at HEAD: Phase 5 verification is `passed`, 8/8, and current production semantic integration is active. [VERIFIED: .planning/phases/05-manufacturing-evidence-model-and-gerber-baseline/05-VERIFICATION.md:1-9] [VERIFIED: crates/ratemypcb-core/src/lib.rs:4346-4529] |
| The DFM note says no shared measured Phase 7 analyzer or qualification registry exists. | blocker for DFM-05, expected phase gap | Confirmed: only three stable fabrication coverage dispatch contracts exist, and repository search finds no TP/FP/FN/TN or precision/recall evaluator. [VERIFIED: crates/ratemypcb-core/src/fabrication.rs:1465-1518] |
| Both notes identify empty production assembly placement evidence. | high for DFM-03 | Confirmed: the DTO exists, but current native/package code emits component semantics rather than `AssemblyPlacement`; no `CapabilityId::Assembly` record is emitted. [VERIFIED: crates/ratemypcb-core/src/fabrication.rs:1292-1308] [VERIFIED: crates/ratemypcb-core/src/fabrication/native.rs:2280-2500] [VERIFIED: crates/ratemypcb-core/src/fabrication/native.rs:2780-2828] |

## Current Contracts and Seams

### Discrete source-of-truth values

The analyzer plan must use the current capability spellings exactly. [VERIFIED: crates/ratemypcb-core/src/fabrication.rs:1356-1411]

DATA_Q7M4K2P9_START

```rust
pub enum CapabilityId {
    ProductIdentity, DocumentSyntax, UnitsAndFormat, LayerRoles, LayerOrder,
    GeometryPoints, GeometryLines, GeometryArcs, GeometryRegions, GeometryFlashes,
    GeometryExpanded, Polarity, Transforms, Repetition, Apertures, Macros,
    Profile, Extents, Drills, Routes, Slots, Tools, Plating, LayerSpans,
    X2FileAttributes, X2ApertureAttributes, X2ObjectAttributes, Connectivity,
    Components, Pins, Assembly, Construction, Constraints, NativeKicadFacts,
    PackageCompleteness, PackageReconciliation, LegacyFilenameScreening,
    LegacyTokenScreening,
}
pub enum CapabilityState {
    Complete, Partial, NotProvided, Unsupported, Failed, Stale, Omitted,
}
```

DATA_Q7M4K2P9_END

Public gate impact has exactly the following variants; note that its current serde/default behavior defaults a missing field to blocking, so Phase 7 must never let an analyzer construct final gate impact directly. [VERIFIED: crates/ratemypcb-core/src/lib.rs:90-110]

DATA_W8R2N6C4_START

```rust
pub enum GateImpact {
    Blocking,
    EvidenceOnly,
}
```

DATA_W8R2N6C4_END

Coverage uses the following exact states, which are already mapped to required-evidence execution/result states. [VERIFIED: crates/ratemypcb-core/src/lib.rs:113-130] [VERIFIED: crates/ratemypcb-core/src/lib.rs:3654-3669]

DATA_L3V9H5S1_START

```rust
pub enum CoverageStatus {
    Passed, Attention, NotRun, NotProvided, Failed, Unsupported, Stale, Unknown,
}
```

DATA_L3V9H5S1_END

### Reusable data

- `FabricationReview` carries documents, layers, tools, apertures/macros/blocks/repetitions, features, physical bounds, profile, connectivity, X2 and Job facts, assembly, construction, constraints, capabilities, omissions/conflicts, reconciliation, warnings, limits, and allocation accounting. [VERIFIED: crates/ratemypcb-core/src/fabrication.rs:1774-1808]
- Geometry is integer picometres and preserves points, lines, arcs, contours, regions, flashes, drills, routes, and slots; features retain layer/tool/polarity/transforms/membership/provenance. [VERIFIED: crates/ratemypcb-core/src/fabrication.rs:922-1108]
- `ManufacturingProvenance` already has document/artifact/producer/version and structural record/subrecord/byte range. [VERIFIED: crates/ratemypcb-core/src/fabrication.rs:770-788]
- `AssemblyEvidence` currently has placements plus mask/paste layer IDs, but layer-ID presence alone cannot prove per-component paste coverage. [VERIFIED: crates/ratemypcb-core/src/fabrication.rs:1292-1308]
- BOM lines retain a source `line_number`, references, quantity, value, footprint, manufacturer, and MPN, but findings produced by `bom_review` still use artifact-wide locations such as `BOM`. [VERIFIED: crates/ratemypcb-core/src/lib.rs:303-344] [VERIFIED: crates/ratemypcb-core/src/lib.rs:1691-1766]
- Schematic occurrences preserve project/root identity, sheet UUID path, item UUID, source path, reference/unit, and source facts. [VERIFIED: crates/ratemypcb-core/src/schematic.rs:72-108]
- Native KiCad manufacturing currently retains pad drill/slot geometry and object net/component/pin semantics, but not assembly placements, courtyards, paste apertures, or fitted-side placement capability. [VERIFIED: crates/ratemypcb-core/src/fabrication/native.rs:2280-2500] [VERIFIED: crates/ratemypcb-core/src/fabrication/native.rs:2878-2891]

### Integration seam

The correct insertion is after manufacturing/native reconciliation and schematic review, but before Phase 7 analyzers and `finalize_evidence`. At that convergence point, one optional bounded local declaration value is normalized into existing fixed-point `FabricationReview.constraints` and only those customer order/profile facts representable by existing `FabricationReview.construction`/constraint fields. Phase 7 then maps the already-produced typed `SchematicReview.mismatches`; it does not parse or recompare BOM/placement/occurrence facts in `dfm.rs`. At present `review()` gathers these inputs separately and calls `finalize_evidence` only near the end, so the plan should add one core normalization/analyzer call there rather than make adapters or the viewer aware of policy. [VERIFIED: crates/ratemypcb-core/src/lib.rs:4693-4802] [VERIFIED: crates/ratemypcb-core/src/lib.rs:5000-5100]

The validator already enforces capability legality for the three fabrication coverage families, canonical evidence IDs, one evidence record per finding/coverage occurrence, authoritative required evidence, and recomputed approval eligibility. Extend these checks for Phase 7 family qualification; do not fork them. [VERIFIED: crates/ratemypcb-core/src/lib.rs:4082-4215]

## Standard Stack

### Core

| Tool/library | Version | Use | Recommendation |
| --- | --- | --- | --- |
| Rust workspace | package `0.2.0`, edition 2024, minimum Rust 1.85 | Core analyzers and tests | Keep existing workspace; local `rustc 1.96.0` is available. [VERIFIED: Cargo.toml:1-15] |
| Rust standard library | toolchain-provided | Checked integer math, deterministic `BTreeMap`/`BTreeSet`, sorting | Use first; no geometry dependency. [VERIFIED: crates/ratemypcb-core/src/fabrication.rs:1-20] |
| Existing `serde` / `serde_json` | workspace major `1` | Existing report DTOs and one fixture manifest | Reuse; no new manifest framework. [VERIFIED: Cargo.toml:12-19] |
| Existing `sha2` | workspace `0.10` | Existing stable evidence/model identities | Reuse existing helpers; no alternate identity system. [VERIFIED: Cargo.toml:12-19] |
| Existing native `kicad-cli` path | locally 10.0.5; repository supports bounded native execution | Independent native DRC/courtyard evidence | Consume normalized native results; do not rebuild KiCad DRC semantics. [CITED: https://docs.kicad.org/9.0/en/cli/cli.html#pcb_drc] |

### No installation

No external package is required or recommended. Therefore package-legitimacy auditing is not applicable, and the implementation plan must contain no install task.

### Why no geometry crate

The first tracer is cross-artifact set/field correlation and needs no geometric library. Later families should add only the exact fixed-point primitive needed by that family (for example point-to-segment or circle boundary distance), only after existing canonical shape coverage and hard negatives prove the operation is sufficient. Pulling a floating-point polygon engine would weaken the existing exact-unit/arc/polarity contract and is not justified by the first slice. [VERIFIED: crates/ratemypcb-core/src/fabrication.rs:922-1108]

## Deterministic vs Inference Taxonomy

| Class | Definition | Default result/gate | Families |
| --- | --- | --- | --- |
| Deterministic comparison | Exact facts and an explicit source/version-bound rule produce a reproducible boolean and measurement. | `not_checked` if any prerequisite/represented shape is unqualified; otherwise `EvidenceOnly` until reviewed qualification may promote. | Population parity, declared side/rotation correlation, minimum finished drill, outline topology, copper clearance, ring, edge, mask sliver, paste/mask, construction conflict. |
| Deterministic native observation | A supported native tool reports a typed violation with exact tool/version/exclusion/location. | Preserve native channel semantics; Phase 7 may classify but must not manufacture a clean result if native execution did not complete. | Courtyard overlap/malformed courtyard, configured creepage, footprint-type mismatch. [CITED: https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html#design-rule-checking] |
| Confirmation gap | A required construction/order/profile declaration is absent, conflicting, partial, or stale. | `not_checked`/attention and release-unblocking evidence; never pass and never a fabricated violation. | Material/thickness/finish, drill span, impedance declaration, special process, order/profile confirmation. |
| Conservative inference | Geometry/connectivity plus explicit assumptions suggests risk but does not establish intent or compliance. | `EvidenceOnly`; family-specific human promotion is still required even after metrics. | Return path, high current, differential quality, thermal, interface, access/test point. |
| Unsupported inference | Required voltage/current/frequency/material/power/package authority is absent. | `not_checked`; emit the missing prerequisite, not a risk conclusion. | Creepage compliance without voltage/material/environment, ampacity without current/copper/process, thermal adequacy without power/boundaries, physical package compatibility without approved land-pattern authority. |

KiCad’s own definitions reinforce the distinction: copper clearance applies between different-net items under configured rules, and creepage likewise requires a configured creepage rule; dimensions or net names alone do not establish the rule. [CITED: https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html#design-rule-checking]

## Family Prerequisites and Measured Outputs

`Complete` capability records are necessary but not sufficient. Every family must also reject affected omissions/conflicts, duplicate prerequisite records, unhandled primitive shapes, missing threshold authority, and dangling provenance. [VERIFIED: crates/ratemypcb-core/src/fabrication.rs:1520-1549] [VERIFIED: crates/ratemypcb-core/tests/fabrication_release.rs:1323-1414]

### DFM-01 deterministic geometry

Production thresholds require the early bounded authority seam: an explicit source/version-bound fabrication profile or project rule is parsed from local input, hashed and location-linked, converted from exact declared units into existing fixed-point `ManufacturingConstraint` values, and merged without overwriting conflicts. Existing semantic `ConstraintKind` variants cover drill, clearance, and annular ring; copper-edge, mask-sliver, and paste/mask limits use `ConstraintKind::Other` only with a closed/versioned accepted rule ID and explicit applicability/provenance. Unknown `Other` IDs are unrepresented, never guessed. Anonymous `Preset` `f64` values and fixture-injected constraints do not qualify production behavior. Any absent, duplicate, stale, ambiguous, unrepresented, or inapplicable rule keeps its dependent family `not_checked`.

| Stable family ID | Minimum complete prerequisites | Exact output | Must become `not_checked` when |
| --- | --- | --- | --- |
| `dfm.outline-topology.v1` | `LayerRoles`, `Profile`, relevant `GeometryLines`/`GeometryArcs`/`GeometryRegions`, `GeometryExpanded`, `Transforms`, `Polarity` | Contour IDs, closed/open state, intersections, exterior/cutout classification, extents, source resolution | Multiple/ambiguous profile authorities, unexpanded constructs, unsupported polarity, non-materialized transforms, or ambiguous exterior/cutout topology. |
| `dfm.minimum-finished-drill.v1` | `UnitsAndFormat`, `Tools`, `Drills`, `Constraints`; add `Plating`/`LayerSpans` only to statements that use them | Minimum observed finished diameter, exact tool/hit IDs, explicit threshold and threshold source | Threshold absent/stale, tool diameter absent, drill references ambiguous, or plating/span is required but unknown. Ucamco XNC defines the declared tool diameter as the finished diameter after plating. [CITED: https://www.ucamco.com/files/downloads/file_en/452/xnc-format-specification-revision-2021-11_en.pdf] |
| `dfm.copper-clearance.v1` | `LayerRoles`, copper geometry families, `GeometryExpanded`, `Transforms`, `Polarity`, `Connectivity`, `Constraints` | Nearest boundary-to-boundary distance, two feature IDs/nets/layer, threshold, delta | Same-net identity unavailable, zone/region/arc/negative-polarity shape unsupported, threshold absent, or candidate pair cannot be proven same physical layer. KiCad likewise defines clearance violations for different-net copper under configured rules. [CITED: https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html#design-rule-checking] |
| `dfm.annular-ring.v1` | `LayerRoles`, `LayerOrder`, copper geometry, `Tools`, `Drills`, `Plating`, `LayerSpans`, `GeometryExpanded`, `Transforms`, `Polarity`, `Constraints`, plus an unambiguous hole↔pad association | Minimum radial copper remaining around the actual hole boundary on every applicable span layer, with pad/hole/tool IDs | Hole↔pad association is absent, NPTH/slot is misclassified, span unknown, pad geometry incomplete, or only nominal diameter arithmetic is possible. Eurocircuits explicitly distinguishes production tool size from finished hole size, so the source of the threshold/diameter must be retained rather than silently mixed. [CITED: https://www.eurocircuits.com/pcb-classification-drill-class/] |
| `dfm.copper-edge.v1` | `LayerRoles`, copper geometry, `Profile`, `GeometryExpanded`, `Transforms`, `Polarity`, `Constraints` | Minimum distance from copper boundary to exterior/cutout/routed boundary, both feature IDs, threshold | Profile/cutout authority incomplete, routed slots omitted, negative layers unsupported, or threshold absent. |
| `dfm.mask-sliver.v1` | Mask `LayerRoles`, mask geometry, `GeometryExpanded`, `Transforms`, `Polarity`, `Constraints` | Minimum remaining mask material between resolved openings, opening IDs, threshold | Negative-mask interpretation or merged opening intent is unresolved; tenting/override facts needed for the target are absent. KiCad notes solder-mask layers are negative and openings can be expanded per source rule. [CITED: https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html#working-with-graphic-shapes] |
| `dfm.paste-mask-relationship.v1` | Paste/mask `LayerRoles`, geometry, `GeometryExpanded`, `Transforms`, `Polarity`; `Components`, `Pins`, `Assembly` for per-component claims | Per-pad geometric set relationship and measured expansion/reduction, with both compared feature locations | Only layer IDs exist, component/pad association is partial, a deliberate paste omission/windowpane/pin-in-paste policy is unresolved, or placed side is unknown. KiCad permits deliberate per-pad paste removal, so absence alone is not a defect. [CITED: https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html#editing-footprints] |
| `dfm.drill-tool-integrity.v1` | `UnitsAndFormat`, `Tools`, and the relevant `Drills`/`Routes`/`Slots`; `Plating`/`LayerSpans` per subcheck | Tool diameter, object kind, span/plating state, duplicate/undefined/mismatch evidence | Routes/slots are being treated as round hits, selected tool is ambiguous, or required plating/span is unknown. Ucamco models drill holes and routed slots as distinct hole objects. [CITED: https://www.ucamco.com/files/downloads/file_en/452/xnc-format-specification-revision-2021-11_en.pdf] |

Do not flatten arcs, ignore clear polarity, approximate macro/block/step-repeat expansion, or use bounding boxes as a final clearance/ring answer. Ucamco defines draws/arcs as stroked geometry, flashes as aperture replications, and regions as closed contours; these semantics affect nearest-distance results. [CITED: https://www.ucamco.com/files/downloads/file_en/554/gerber-layer-format-specification-revision-2026-05_en.pdf]

### DFM-02 construction and order confirmation

| Family | Required facts | Output policy |
| --- | --- | --- |
| `dfm.stackup-order-confirmation.v1` | `Construction`, `LayerOrder`, explicit requirement source | Compare declared layer order/material/thickness facts to requirement; missing source is a confirmation gap. |
| `dfm.total-thickness-material.v1` | `Construction`, `Constraints` | Compare fixed-point total/per-layer thickness and exact material strings only where both authorities are explicit; preserve both sides on conflict. |
| `dfm.drill-span-plating.v1` | `Tools`, `Drills`, `Plating`, `LayerSpans`, `LayerOrder`; customer acknowledgement currently unrepresented | Report design evidence plus a source-linked confirmation gap only; defer match/conflict and never assume through-hole. |
| `dfm.finish-profile.v1` | `Construction`, `Profile`, `Constraints`, order/profile authority | Match/conflict represented finish only; profile/castellation/edge-plating acknowledgements currently produce confirmation gaps. |
| `dfm.impedance-special-process.v1` | `Constraints`, `Construction`, explicit order acknowledgement | Confirm declaration and agreement only; do not calculate impedance without an approved electrical model and complete stackup. |

The same bounded input seam accepts explicit customer order/profile acknowledgements and normalizes only facts already represented by `ConstructionEvidence` or a semantically matching `ConstraintKind`, retaining source/version/digest/location and exact units. At current HEAD, layer material/thickness/total thickness, finish, and explicit impedance/material/finish/special-process constraints are representable; an unrepresented drill-span/plating acknowledgement, castellation/edge-plating requirement, or other profile fact may produce only a confirmation gap. Its match/conflict subfamily is deferred rather than encoded as `Other` or forced into a second model.

The legacy `stackup.rs` uses `f64` and can infer Gerber layer count from filenames while explicitly labeling that result “not construction evidence”; it must not become Phase 7 authority. [VERIFIED: crates/ratemypcb-core/src/stackup.rs:28-47] [VERIFIED: crates/ratemypcb-core/src/stackup.rs:149-170]

### DFM-03 assembly

| Stable family ID | Prerequisites | Deterministic boundary |
| --- | --- | --- |
| `assembly.population-parity.v1` **first tracer** | Completed typed `SchematicReview` reconciliation output, required artifact identity/provenance, and its existing occurrence-first joins/fallbacks | Map the existing typed population/fitted/DNP/quantity/placement/revision mismatch subset and clean completion state into family evidence. Do not recompare rows or alter identity/fallback semantics; missing typed authority is `not_checked`. [VERIFIED: crates/ratemypcb-core/src/schematic.rs:1990-2340] |
| `assembly.side-rotation.v1` | `Assembly`, `NativeKicadFacts`, explicit placement coordinate/unit/origin/side/rotation convention, dual provenance | Normalize angles exactly to microdegrees and compare only after top/bottom convention and origin are explicit. Unknown bottom mirroring/rotation convention is `not_checked`. |
| `assembly.paste-availability.v1` | `Assembly`, `Components`, `Pins`, paste `LayerRoles`, paste geometry, `GeometryExpanded`, `Transforms`, `Polarity`, fitted/DNP state | A fitted paste-requiring pad/component has applicable paste geometry on its placed side; layer presence is not coverage. |
| `assembly.courtyard-native.v1` | Completed supported native DRC, exact tool/version, active/excluded/unknown exclusion state, typed source locations | Preserve native overlap/malformed/missing-courtyard observations. KiCad defines courtyard overlap and malformed non-closed courtyard violations. [CITED: https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html#design-rule-checking] |
| `assembly.footprint-string-parity.v1` | Occurrence/board/BOM footprint strings, exact or unique fallback identity, dual provenance | Exact source-string/library-suffix policy only; never physical package compatibility. Existing reconciliation currently accepts exact equality or matching suffix after `:` for board footprint comparison, which must be frozen and adjudicated rather than broadened silently. [VERIFIED: crates/ratemypcb-core/src/schematic.rs:2081-2094] |
| `assembly.access.v1` | Complete placement/profile/component geometry plus explicit assembly process/tool envelope | Inference-only until a named process/tool envelope and corpus exist. |
| `assembly.testpoint-access.v1` | Complete connectivity/component/pin/placement/profile geometry plus explicit probe envelope and target-net authority | Inference-only; missing connectivity or probe/process input is `not_checked`. Current code only excludes TP-like references from ordinary placement population. [VERIFIED: crates/ratemypcb-core/src/lib.rs:760-772] |

The first population tracer consumes the existing typed `SchematicReview.mismatches` and reconciliation capability/artifact metadata. It must not normalize rows or duplicate joins/comparisons in `dfm.rs`. Rich X/Y/unit/side/rotation normalization remains a later DFM-03 input task because the existing placement path cannot support source-linked side/rotation findings; that later normalization must not change the current reconciliation comparison or identity-fallback semantics. [VERIFIED: crates/ratemypcb-core/src/lib.rs:1801-1901] [VERIFIED: crates/ratemypcb-core/src/schematic.rs:1990-2340]

### DFM-04 inference prerequisites

| Family | Required explicit intent beyond geometry | Default |
| --- | --- | --- |
| `inference.return-path.v1` | Signal class/frequency or edge-rate, reference-plane intent, complete layer order/connectivity | `not_checked` without intent; otherwise `EvidenceOnly`. |
| `inference.high-current.v1` | Required current, allowed rise/drop, copper thickness/finish/process, complete geometry | `not_checked` without current/process; otherwise `EvidenceOnly`. |
| `inference.creepage.v1` | Voltage domains, required creepage rule, material/environment/coating policy, board-edge/cutout geometry | Prefer completed configured native DRC evidence; do not infer voltage from names. KiCad supports an explicit creepage constraint. [CITED: https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html#custom-design-rules] |
| `inference.differential.v1` | Explicit pair identity, impedance/skew target, stackup, complete route geometry | `not_checked` without pair/target; otherwise `EvidenceOnly`. |
| `inference.thermal.v1` | Dissipation/current, copper/stackup/material, boundary/airflow/heatsink assumptions | `not_checked` without power/boundary inputs; otherwise `EvidenceOnly`. |
| `inference.interface.v1` | Explicit interface protocol/connector/pin intent and electrical constraints | `not_checked` without declared interface; net-name pattern matching is prohibited. |

## Qualification Math and Promotion Contract

### Observation unit

Use one declared `targetKey` as the denominator unit, not prose and not a whole fixture. A target key is `(familyId, familyVersion, fixtureDigest, canonicalTargetIds)` and has one adjudicated expected label (`violation` or `clean`) plus one actual label (`finding` or `no_finding`). Unsupported/prerequisite-mutation observations have expected run status `not_checked` and are reported separately; counting them as true negatives would inflate precision/recall without exercising semantics.

| Expected | Actual | Count |
| --- | --- | --- |
| violation | finding with the same stable target key | TP |
| clean | finding | FP |
| violation | no finding | FN |
| clean | no finding | TN |

`precision = TP / (TP + FP)`. When `TP + FP == 0`, precision is undefined and the family cannot block. `recall = TP / (TP + FN)`. When `TP + FN == 0`, recall is undefined; the corpus is missing a positive and cannot qualify. Report TP, FP, FN, TN, precision, recall, executable target count, `not_checked` mutation count, and unsupported target count per family/version. The 95% threshold applies only to defined precision; recall has no invented threshold. [VERIFIED: .planning/phases/07-decision-grade-dfm-and-assembly-analysis/07-CONTEXT.md:28-34]

### Fixture classes

1. **Positive:** isolated violation, multiple violations, deterministic nearest/ordering, exact measurement and threshold provenance.
2. **Hard negative:** exact threshold, one source-resolution unit safe, intentional exceptions, same-net/NPTH/tenting/windowpane/DNP/cutout cases appropriate to the family.
3. **Mutation:** remove a prerequisite; change it to each non-complete state; duplicate its capability record; add an affected omission/conflict; dangle source/feature IDs; reorder facts; alter source resolution/units. Expected behavior is `not_checked` or validation failure, never pass/blocking.
4. **Parity:** Phase 6 selected no-go, so omit ODB++/IPC-2581 parity entirely this release; native KiCad plus Gerber/X2+XNC remains the baseline.

### Promotion eligibility

A family may select `Blocking` only when all are true: deterministic classification; exact family/version registry match; defined precision ≥ 0.95; defined recall reported; positive and hard-negative targets present; every fail-closed mutation green; fixture digests and adjudication record present; reviewed promotion entry present. Any missing, duplicate, stale, unknown, under-threshold, version-mismatched, or removed entry forces `EvidenceOnly` and never creates a pass. Inference adds the separate family-specific human approval in `07-PROMOTION-CHECKPOINT.md`; none is approved today. [VERIFIED: .planning/phases/07-decision-grade-dfm-and-assembly-analysis/07-PROMOTION-CHECKPOINT.md:11-27]

Keep this as a small manifest evaluator in the focused integration test, not a plugin framework or generic benchmarking service.

## Release-Unblock Ranking (DFM-06)

Core should compute a set of acceptable evidence references for the highest release-unblock tier; it should not generate assessment prose.

1. Collect required evidence whose execution is not completed, result is not pass, or freshness is neither current nor not-applicable. Rank by existing `required_coverage(scope)` order, then `check_id` for determinism. [VERIFIED: crates/ratemypcb-core/src/lib.rs:3361-3397] [VERIFIED: crates/ratemypcb-core/src/lib.rs:3682-3693]
2. If tier 1 is nonempty, its evidence refs are the top unblock set. Do not consider score or evidence-only findings.
3. Otherwise collect qualified blocking findings at medium-or-higher severity. Rank severity descending, then stable family/version, canonical location/target IDs, and evidence ID. Deduplicate only when the same declared corrective-action key resolves all grouped occurrences.
4. If tiers 1–2 are empty, collect evidence-only/inference attention by deterministic family/location order.
5. When disposition is not `approve`, assessment action priority 1 must exist and its evidence refs must intersect the top unblock set. Keep the existing at-most-three and unique-priority checks. [VERIFIED: crates/ratemypcb-core/src/lib.rs:4267-4312]
6. Score never enters ordering or approval. Existing approval already depends only on required evidence and blocking findings. [VERIFIED: crates/ratemypcb-core/src/lib.rs:3682-3693]

A missing evidence record currently can have an empty `evidence_id` in `RequiredEvidence`; the planner must handle this by ranking the canonical coverage occurrence that caused the gap, or by adding one evidence-bearing missing-coverage occurrence before assessment validation. Do not permit an empty string to become a valid P1 reference. [VERIFIED: crates/ratemypcb-core/src/lib.rs:3630-3653]

## Validation Architecture

### Test framework

| Property | Value |
| --- | --- |
| Framework | Built-in Rust test harness; Node built-in test runner for report/viewer regression. [VERIFIED: .github/workflows/ci.yml:12-29] |
| Existing config | Cargo workspace plus `.github/workflows/ci.yml`; no new framework config. [VERIFIED: .github/workflows/ci.yml:1-29] |
| Focused quick run | `cargo test -p ratemypcb-core --test dfm_release --locked` (Wave 0 file does not yet exist). |
| Full suite | `cargo fmt --all -- --check && cargo clippy --all-targets --locked -- -D warnings && cargo test --all --locked && node --test tests/board-view.test.mjs tests/report-contract.test.mjs tests/report-ux.test.mjs` [VERIFIED: .github/workflows/ci.yml:12-24] |

### Requirement-to-test map

| Requirement | Required checks | Test type | Existing? |
| --- | --- | --- | --- |
| DFM-01 | Per-family exact measurements, threshold boundary ± source resolution, geometry/polarity/transform hard negatives, provenance, dispatch mutation matrix | focused integration/property-style loops | ❌ Wave 0 |
| DFM-02 | explicit match/conflict/gap for construction/order; no-default mutations | focused integration | ❌ Wave 0 |
| DFM-03 | population tracer, duplicate/ambiguous/DNP/excluded cases, row/occurrence dual provenance, side/rotation convention, paste/courtyard bounded behavior | focused integration plus existing schematic regressions | partial: `schematic_release.rs` exists; Phase 7 lane absent [VERIFIED: crates/ratemypcb-core/tests/schematic_release.rs:242-509] |
| DFM-04 | each missing intent prerequisite → `not_checked`; assumptions visible; forged blocking rejected | focused integration/report mutation | ❌ Wave 0 |
| DFM-05 | manifest validation, TP/FP/FN/TN, undefined denominators, <95% precision, mutation gate, registry/version/removal downgrade | focused integration | ❌ Wave 0 |
| DFM-06 | missing evidence outranks blocker/inference/score, deterministic ties, dedupe key, P1 intersection, three-action cap, viewer no-recompute | Rust contract + existing Node report tests | partial: action cap/reference tests exist; rank contract absent [VERIFIED: crates/ratemypcb-core/src/lib.rs:4245-4322] |

### Wave 0 gaps

- `crates/ratemypcb-core/tests/dfm_release.rs` — one test lane for the first family, qualification math, report mutations, and later incremental families.
- `tests/fixtures/dfm/manifest.json` plus the minimum project-authored canonical/BOM/placement cases — one manifest, not one framework per family.
- Source-linked normalized placement fixture rows with side/rotation conventions.
- A report/assessment fixture proving missing required evidence outranks a severe-looking evidence-only finding.

Plan 07-01 created the inert contract/fixtures and Plan 07-02 created the first EvidenceOnly tracer. Plan 07-03 is unblocked by the recorded no-go; required coverage, Blocking, and inference promotion still require their own explicit qualification gates.

### Sampling

- Per task: focused `dfm_release` test plus the directly touched existing test.
- Per wave: all locked Rust tests and the three Node suites.
- Phase gate: full CI, schema regeneration/equality if public DTOs change, fail-closed mutation matrix, reported metrics, reviewed promotion entry, and `git diff --check`.

## Minimal Sequencing

### Gates 0-1 — resolved

1. Plan 07-01 froze exact family IDs/versions, prerequisites, target-key schema, metric formulas, and mutation accounting.
2. Plan 07-02 landed the first format-independent EvidenceOnly tracer; `07-PROMOTION-CHECKPOINT.md` remains closed.
3. Phase 6 selected no-go for both intelligent formats. Continue with native KiCad plus Gerber/X2+Excellon and omit intelligent-format parity. Plan 07-03 is unblocked.

### Slice 1 — one deterministic tracer

1. Add one plain `crates/ratemypcb-core/src/dfm.rs` module.
2. Consume existing typed occurrence-first `SchematicReview.mismatches`; do not reparse rows or duplicate population/fitted/DNP/quantity/placement/footprint/revision comparisons or identity fallback.
3. Implement only `assembly.population-parity.v1` qualification/evidence mapping and keep output evidence-only.
4. Add positive/hard-negative/mutation fixtures, compute metrics, and run report provenance/approval mutations.
5. Promote only this deterministic version if precision and review policy pass.

### Slice 2 — bounded authority seam and qualification-backed action contract

Add one optional local CLI→core declaration value. Normalize source/version-bound fabrication profile/project rules into fixed-point `ManufacturingConstraint` and representable customer order/profile acknowledgements into existing `ConstructionEvidence`/constraints. Missing threshold authority stays `not_checked`; unrepresented order facts yield confirmation gaps only. Then add central family/version gate-impact selection and top-unblock evidence refs; extend `validate_assessment` so non-approve P1 intersects that set. Test score independence before touching viewer code. Render only the core result.

### Slice 3 — measured geometry, one family per task

Recommended order: minimum finished drill → outline topology → copper-to-edge → copper clearance → annular ring → mask sliver → paste/mask. This orders simpler exact facts before pairwise/full-shape operations; every task brings its own corpus and may remain `not_checked` where current canonical associations are insufficient.

### Slice 4 — construction, then remaining assembly

Add confirmation-gap families before side/rotation and paste availability. Reuse native courtyard evidence rather than reimplementing it. Keep physical package compatibility deferred.

### Slice 5 — inference last

Perform an explicit intent/capability audit per family. Implement at most one bounded inference family at a time, always `EvidenceOnly`, with the family-specific human checkpoint still closed by default.

## Architecture Pattern

```text
bounded inputs
  -> existing adapters / BOM-placement-schematic normalizers (policy-free)
  -> FabricationReview + occurrence/BOM/placement facts + capability ledger
  -> existing dispatch_analyzer prerequisite guard
       -> missing/duplicate/non-complete/unhandled => not_checked
       -> complete => one pure family measurement
  -> qualification registry selects EvidenceOnly or reviewed Blocking
  -> existing finding/coverage finalization and provenance IDs
  -> existing report validation and approval recomputation
  -> core top-unblock evidence-ref set
  -> assessment P1 intersection validation
  -> CLI/viewer render only
```

## Don't Hand-Roll

| Problem | Do not build | Use instead |
| --- | --- | --- |
| Board/canonical model | A DFM-specific scene graph | Existing `FabricationReview`, fixed-point geometry, connectivity, assembly/construction facts. |
| Capability routing | Plugin registry or second dispatcher | Existing `AnalyzerRequirements` and `dispatch_analyzer`. |
| Gerber/XNC/KiCad semantics | Analyzer-local parser or browser geometry | Accepted Phase 5 adapters and native KiCad path. |
| Stable evidence | New family UUID/hash scheme | Existing check IDs, canonical target/location, and `finalize_evidence`. |
| Approval/promotion | Analyzer-selected `GateImpact` or second approval engine | Central qualification policy plus existing approval/report validators. |
| Recommendations | Generated prose model or viewer sorting | Core top-unblock evidence refs plus human assessment prose. |
| Physical package mapping | Distributor packaging lookup or name heuristic | `not_checked` until authoritative package↔qualified-footprint evidence is approved. |
| Full geometry toolkit | Speculative polygon framework or new dependency | One exact fixed-point operation only when a qualified family needs it. |

## Common Pitfalls

1. **A complete capability is mistaken for a semantic pass.** Dispatch still needs a real semantic result; `None` is `NotChecked`. Test this for every family. [VERIFIED: crates/ratemypcb-core/src/fabrication.rs:1540-1549]
2. **Current default-blocking finding construction bypasses qualification.** The generic `finding()` helper sets `GateImpact::Blocking`; Phase 7 output must pass through central policy, not this default. [VERIFIED: crates/ratemypcb-core/src/lib.rs:908-929]
3. **Bounding boxes replace shape distance.** This yields false positives/negatives around arcs, regions, cutouts, flashes, and negative polarity. Keep unsupported shapes `not_checked`.
4. **Same-net copper is treated as clearance failure.** Blocking clearance needs complete connectivity; otherwise no different-net conclusion is available. KiCad’s own clearance definition is different-net. [CITED: https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html#design-rule-checking]
5. **Finished hole and production tool size are mixed.** Preserve the declared semantics and threshold source; fabricator calculations can differ from XNC finished diameter. [CITED: https://www.eurocircuits.com/pcb-classification-drill-class/] [CITED: https://www.ucamco.com/files/downloads/file_en/452/xnc-format-specification-revision-2021-11_en.pdf]
6. **Paste layer presence is called paste coverage.** Require per-pad/component association, fitted state, side, and actual paste geometry.
7. **Placement headers are called placement validation.** Current code ignores value semantics; normalize units, finite exact decimals, side vocabulary, rotation, origin, and row provenance first. [VERIFIED: crates/ratemypcb-core/src/lib.rs:1823-1861]
8. **Existing schematic reconciliation is reimplemented or promoted wholesale.** It is intentionally evidence-only and contains established unique-reference fallback semantics; consume its typed mismatch output, qualify/map only the named family/version, and never duplicate or silently alter those comparisons in `dfm.rs`. [VERIFIED: crates/ratemypcb-core/src/schematic.rs:1962-2340]
9. **Undefined precision is treated as perfect.** Zero predicted positives is undefined, not 100%; zero adjudicated positives is a corpus failure.
10. **Mutation `not_checked` cases inflate TN.** Report them separately; semantic TN needs an executable adjudicated clean target.
11. **Required coverage is silently expanded before qualification.** Phase 6 is resolved; still add no required Phase 7 check until the production family and promotion policy are accepted.
12. **Score influences P1.** Score is not an approval input and must not enter unblock ordering. [VERIFIED: crates/ratemypcb-core/src/lib.rs:3682-3693]

## Security Domain

Security enforcement is enabled and this phase processes already-bounded local design data; no network service, authentication, session, or cryptographic protocol is introduced. [VERIFIED: .planning/config.json:3-17]

| ASVS category | Applies | Control |
| --- | --- | --- |
| V2 Authentication | no | No identity boundary in this local core phase. |
| V3 Session Management | no | No session state. |
| V4 Access Control | no | No remote/user authorization surface. |
| V5 Validation, Sanitization and Encoding | yes | Reuse bounded parsers/model validation; validate manifest uniqueness/digests/counts, checked integer arithmetic, canonical IDs, finite collection bounds, and fail-closed unsupported states. |
| V6 Cryptography | limited | Reuse SHA-256 identity helpers; do not invent signing or promotion tokens. |
| V8 Data Protection | yes | Keep local-first artifact paths relative and preserve existing report/provider retention restrictions. |

Threat-focused validation: malicious fixture/manufacturing inputs must not cause quadratic unbounded candidate pairing, integer overflow, panic, nondeterministic order, dangling evidence, or a false complete/pass. Phase 5’s 1,000,000-feature ceiling makes naive global O(n²) pair scans an unacceptable default; plan deterministic spatial pruning or bounded layer-local candidates when clearance families begin. [VERIFIED: .planning/phases/05-manufacturing-evidence-model-and-gerber-baseline/05-CONTEXT.md:39-58]

## Environment Availability

| Dependency | Required by | Available | Version/fallback |
| --- | --- | --- | --- |
| Rust/Cargo | core and tests | yes | `rustc 1.96.0`, `cargo 1.96.0`; workspace minimum 1.85. |
| Node | existing report/viewer tests | yes | `v22.23.1`. |
| `kicad-cli` | optional native-corpus checks | yes | `10.0.5`; fixture-driven normalized native reports remain the CI fallback where KiCad is unavailable. |
| New package/service | none | not applicable | No install and no network runtime dependency. |

## Open Decisions

1. **Phase 6 decision record — RESOLVED.** External `06-01-SUMMARY.md` and `06-08-SUMMARY.md` record no-go for both ODB++ and IPC-2581 this release, FMT-03/FMT-05 complete, FMT-04 N/A, and Phase 6 8/8. Plan 07-03 is unblocked; private parser SHA `a4216f6909754155555e9290c2ec84e0eb16d267` remains research only.
2. **Threshold authority normalization — RESOLVED IN PLAN 07-03.** Current `Preset` stores four `f64` values and remains non-authoritative for Phase 7. The early bounded local declaration seam converts explicit source/version-bound fabrication profile or project rules, including drill/clearance/edge/annular/mask/paste limits, into existing fixed-point `ManufacturingConstraint` values with provenance before geometry plans can run. Missing/duplicate/stale/unrepresented authority remains `not_checked`; fixtures cannot bypass the production seam. [VERIFIED: crates/ratemypcb-core/src/lib.rs:537-625] [VERIFIED: crates/ratemypcb-core/src/fabrication.rs:1345-1354]
3. **Hole↔pad association — HIGH.** Current package facts can have copper object component/pin semantics and separate XNC drill facts, but no explicit canonical cross-document hole-to-pad association was found. Recommendation: keep annular-ring `not_checked` until existing reconciliation can expose an unambiguous association; if a minimal fact is required, discuss it as a Phase 5 contract extension rather than guessing by nearest geometry.
4. **Placement convention — HIGH.** Side vocabulary, coordinate units/origin, bottom-side mirroring, and rotation direction are not normalized. Recommendation: freeze one source-declared convention contract and make unknown convention `not_checked`; do not infer from column names.
5. **Construction/order source — RESOLVED IN PLAN 07-03 FOR REPRESENTED FACTS; RESIDUAL GAPS EXPLICIT.** The same bounded local declaration seam carries source/version-bound customer order/profile acknowledgements and normalizes only existing `ConstructionEvidence`/semantically matching constraint fields. At current HEAD, unrepresented drill-span/plating and castellation/edge-plating/profile acknowledgements stay confirmation gaps; their match/conflict branches are deferred rather than guessed or added to a second model.
6. **Family promotion storage — MEDIUM.** Human inference decisions belong in the existing checkpoint; runtime deterministic eligibility still needs a small family/version policy table. Recommendation: one static core table generated/edited alongside reviewed metrics, not a configurable plugin registry.

## Assumptions Log

| # | Claim | Risk if wrong |
| --- | --- | --- |
| A1 | `assembly.population-parity.v1` is the best first tracer among the discretionary ordering options. [ASSUMED] | Planner may choose another deterministic family, but it must still prove the same full contract and remain within one module. |
| A2 | Layer-local spatial pruning can meet later clearance-family resource bounds without a new dependency. [ASSUMED] | A measured implementation spike may show a need for a different exact algorithm; no dependency should be selected without a separate decision. |

All numeric manufacturing acceptance values remain intentionally unassumed; they must come from explicit project/profile/order evidence.

## Sources

### Primary in-repository (HIGH confidence)

- `crates/ratemypcb-core/src/fabrication.rs` — canonical model, exact geometry, assembly/construction, capability enum/ledger, dispatcher, review contract.
- `crates/ratemypcb-core/src/fabrication/native.rs` — native KiCad facts, drill/slot/connectivity semantics, authoritative derivation/reconciliation.
- `crates/ratemypcb-core/src/lib.rs` — current production integration, BOM/placement paths, evidence finalization, required evidence, approval, report/assessment validators.
- `crates/ratemypcb-core/src/schematic.rs` — occurrence-first BOM/placement reconciliation and evidence-only precedent.
- `crates/ratemypcb-core/tests/fabrication_release.rs` and `schematic_release.rs` — capability, mutation, provenance, and reconciliation patterns.
- `.planning/phases/05-manufacturing-evidence-model-and-gerber-baseline/05-VERIFICATION.md` — accepted Phase 5 state.
- `.planning/phases/07-decision-grade-dfm-and-assembly-analysis/07-CONTEXT.md` and `07-PROMOTION-CHECKPOINT.md` — locked Phase 7 policy and inference gate.
- External Phase 6 `06-01-SUMMARY.md` and `06-08-SUMMARY.md` — exact no-go, FMT disposition, and 8/8 closure receipts.

### Authoritative engineering documentation (MEDIUM confidence)

- KiCad 9 PCB Editor: <https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html> — configured clearance, annular width, holes, courtyards, mask, creepage, and DRC semantics. [CITED]
- KiCad 9 CLI: <https://docs.kicad.org/9.0/en/cli/cli.html#pcb_drc> — native PCB DRC report command and exit behavior. [CITED]
- Ucamco Gerber Layer Format 2026.05: <https://www.ucamco.com/files/downloads/file_en/554/gerber-layer-format-specification-revision-2026-05_en.pdf> — exact image primitives, apertures, transforms, polarity, attributes, profile, and component data. [CITED]
- Ucamco XNC 2021.11: <https://www.ucamco.com/files/downloads/file_en/452/xnc-format-specification-revision-2021-11_en.pdf> — finished tool diameter and drill/rout object semantics. [CITED]
- Eurocircuits pattern/drill classification: <https://www.eurocircuits.com/pcb-classification-drill-class/> — process-specific track/gap/ring/tool classification and production-tool versus finished-hole distinction. [CITED]
- AISLER KiCad DRC source: <https://github.com/AislerHQ/aisler-support/tree/master/kicad/aisler-2-layer-simple-drc> — example that fabricator rules are source-specific and incomplete by their own declaration. [CITED]

### Sources deliberately not used for policy

- No inaccessible/paywalled IPC text was used to invent numeric acceptance criteria.
- Distributor offer packaging was not used as physical package authority.
- The two `/tmp` prep notes were treated as hypotheses and checked against HEAD, not as current source of truth.

## Metadata

**Confidence breakdown:**

- Current code seams: HIGH — source-of-truth files and tests opened at HEAD `5e0fa62a5865cdea1a7755c6bedcedab3a64ba07`.
- Dependency/gate state: HIGH — exact Phase 6 receipts, reconciled STATE/requirements/roadmap, context, and validation agree on no-go and Plan 07-03 readiness.
- Deterministic/inference taxonomy: HIGH — locked by context and consistent with official KiCad/Ucamco semantics.
- Exact family algorithms: MEDIUM — prerequisites are source-grounded, but each needs adjudicated corpus proof and some canonical association gaps remain.
- Numeric thresholds: intentionally unresolved — only explicit source/version-bound profile/project/order inputs may supply them.

**Research date:** 2026-08-30
**Valid until:** Recheck after any separately authorized intelligent-format reopening or any change to fabrication/native/BOM/placement contracts, whichever occurs first.
