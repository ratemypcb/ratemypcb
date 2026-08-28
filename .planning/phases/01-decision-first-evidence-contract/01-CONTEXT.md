# Phase 1: Decision-First Evidence Contract — Context

**Gathered:** 2026-08-26  
**Status:** Ready for planning

<domain>
## Phase Boundary

This phase establishes measurable evidence/report/assessment semantics and proves one real report-to-self-contained-HTML path. It includes only enough presentation to make the tracer's disposition, actions, evidence completeness, and links observable. Full report redesign, BOM matrix, broad accessibility/usability corpus, supply v2, schematic analysis, and fabrication parsers belong to later phases.

</domain>

<decisions>
## Decisions

### Release truth

- **D-01:** The first-screen contract answers, in order: can this be manufactured/released, why not, what is the next action, and how complete/fresh is the evidence.
- **D-02:** Risk, required-check coverage, evidence confidence, freshness, and approval eligibility are independent fields; no score or confidence label substitutes for another.
- **D-03:** Required evidence has independent execution, result, and freshness semantics. Missing, failed, stale, unsupported, unknown, `not_run`, `not_provided`, and `attention` states close approval and never reduce observed risk.
- **D-04:** Assessment `disposition` is the sole release disposition. Scores/ratings are secondary metadata and cannot override disposition or the approval gate.

### Identity and traceability

- **D-05:** Use one global evidence-ID namespace, with stable rule-family `checkId` separate from a SHA-256 occurrence/evidence ID derived from artifact digest, check ID, and canonical structured location. Machine paths, prose, severity, ordering, and array position are excluded; duplicates fail closed. Reuse the workspace `sha2` dependency.
- **D-06:** Every finding/coverage occurrence carries bounded provenance: artifact identity/digest, producer kind/name/version, structured location, evidence class, confidence, and freshness/observation time where applicable. Explicit unknown/not-applicable states replace omission.
- **D-07:** Verdicts, actions, category summaries, and structured questions must be nonempty where required, unique, and reference valid evidence in the exact-byte-digest-bound report; validation rejects broken references.

### Vertical proof and contract ownership

- **D-08:** Phase 1 leads with a production-quality report → exact-byte digest → assessment validation → self-contained HTML tracer. It is kept, not a throwaway prototype.
- **D-09:** Rust DTO/schema generation is authoritative; checked-in schemas are verified outputs, and the current 1.1/1.2 skill/docs/fixture drift must become a failing regression.
- **D-10:** The viewer renders core/assessment decisions and evidence; it does not recompute risk, coverage, approval, or BOM conclusions.
- **D-11:** Golden mutations must fail for ambiguous disposition, unknown-state false pass, broken traceability, and a decision summary that exceeds the declared first-screen information-load budget.
- **D-12:** Preserve unrelated dirty 0.2 implementation while executing incrementally; Phase 1 does not choose ODB++, IPC-2581, supply providers, or a new package. Wire the already-present workspace `sha2` dependency into core without changing `Cargo.lock` for that purpose.
- **D-13:** Core exposes `validate_report(&Report)`; CLI `render_snapshot` invokes it immediately after deserialization and before assessment digest validation/rendering. Core owns both `SCHEMA_VERSION` and `ASSESSMENT_SCHEMA_VERSION` plus generated report/assessment schema paths at 2.0.

### the agent's Discretion

- Exact Rust type names and internal helper boundaries, provided they extend the current `Report`, `Finding`, `Coverage`, `Assessment`, `review`, `approval_eligible`, `validate_assessment`, `report_schema`, CLI render, and viewer seams rather than introducing a framework.
- The deterministic short-ID encoding and canonical location serialization, provided collision tests and stability fixtures exist.
- The exact machine-checkable information-load budget representation, provided the fixture checks one disposition, no more than three actions, explicit missing coverage, and bounded summary text/landmarks.
- Whether the contract uses a breaking major schema bump or an additive transition, provided compatibility is explicit and generated/checked-in/schema/skill versions cannot drift silently.

</decisions>

<specifics>
## Specific Ideas

- A blocked golden should show a real missing required check and one evidence-linked action that unlocks it.
- The tracer HTML should expose stable anchors such as `#finding-…`/`#coverage-…`; the exact prefix is discretionary.
- Information overload is tested structurally, not through pixel snapshots: one disposition, ≤3 actions, visible completeness/freshness, scores after the decision summary.

</specifics>

<canonical_refs>

## Canonical References

- `.planning/PROJECT.md` — core value, brownfield boundary, shipped versus dirty capability distinction.
- `.planning/REQUIREMENTS.md` — EVID-01 through EVID-08 acceptance scope.
- `.planning/research/ARCHITECTURE.md` — responsibility map and viewer/core boundary.
- `.planning/research/FEATURES.md` — decision surface and measurable outcomes.
- `crates/ratemypcb-core/src/lib.rs` — current report/evidence/assessment/gate/schema/review seams.
- `crates/ratemypcb-cli/src/main.rs` — exact-byte digest, render validation, CLI dispatch.
- `crates/ratemypcb-cli/src/viewer.rs` — self-contained snapshot embedding.
- `crates/ratemypcb-cli/assets/local-viewer.{html,css,js}` — current score-first report consumer.
- `schemas/report-1.2.json`, `schemas/assessment-1.0.json` — current dirty contract outputs.
- `skills/review-pcb-dfm/references/report-contract.md` — current 1.1-facing human contract and drift evidence.

</canonical_refs>

<code_context>

## Existing Code Insights

### Reusable Assets

- `Report`, `Finding`, `Coverage`, `Assessment`, `AssessmentAction`, `CategorySummary`: existing serialized DTOs.
- `review`, `required_coverage`, `approval_eligible`, `validate_assessment`, `report_schema`: central deterministic policy seams.
- `digest_bytes`, `render_snapshot`, `viewer::snapshot`: proven exact-byte binding and offline HTML path.
- Viewer `renderReport` and native DOM creation: safe rendering without HTML interpolation.

### Established Patterns

- serde camelCase DTOs and JSON-schema-producing function.
- Explicit coverage statuses and fail-closed approval.
- Bounded local file/archive reads and escaped embedded JSON.
- Inline Rust tests plus Node's built-in test runner; no new test framework needed.

### Integration Points

- Core emits decision/evidence data; assessment validation enforces references.
- CLI validates report digest before snapshot.
- Viewer consumes report/assessment and renders stable anchors/summary.
- Contract fixtures exercise current `narrow-board.kicad_pcb` and generated JSON/HTML without live services.

</code_context>

<deferred>
## Deferred Ideas

- Full visual hierarchy, BOM matrix, accessibility completion, user testing, and overload navigation — Phase 2.
- Supply snapshot/provider work — Phase 3.
- Schematic/ERC/parity evidence — Phase 4.
- Gerber canonical parsing and format decisions — Phases 5-6.
- Advanced analyzers and release hardening — Phases 7-8.

</deferred>

---
*Phase: 01-decision-first-evidence-contract*
*Context gathered: 2026-08-26*
