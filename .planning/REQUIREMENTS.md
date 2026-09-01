# Requirements: RateMyPCB Decision-Grade Release Review

**Defined:** 2026-08-26  
**Core Value:** A release decision must be honest, actionable, and traceable: missing evidence blocks approval and is never mistaken for low risk or a pass.

## v1 Requirements

### Evidence and Decision Contract

- [x] **EVID-01**: Report data models risk, required-check coverage, evidence confidence, freshness, and approval eligibility as separate fields with documented semantics.
- [x] **EVID-02**: Any missing, failed, stale, unsupported, `not_run`, `not_provided`, or `not checked` required check prevents approval without reducing reported risk.
- [x] **EVID-03**: Every check has a stable rule-family ID and every occurrence has a deterministic instance ID that survives ordering and prose-only changes.
- [x] **EVID-04**: Every finding and coverage result exposes source artifact identity/digest, tool or provider and version, structured location, evidence class, confidence, and freshness when applicable.
- [x] **EVID-05**: Assessment verdicts, actions, category summaries, and structured questions are rejected unless every claimed evidence reference resolves in the bound report.
- [x] **EVID-06**: Rust report DTO/schema generation is authoritative and a test proves the checked-in schema is JSON-semantically equal to generated output.
- [x] **EVID-07**: One deterministic golden executes report → exact-byte digest → assessment validation → self-contained HTML and proves the disposition and evidence links survive end to end.
- [x] **EVID-08**: Contract goldens fail on ambiguous disposition, unknown-state false passes, broken evidence traceability, and a decision summary that exceeds its declared information-load budget.

### Decision-First Report Experience

- [x] **UX-01**: The first viewport shows disposition, bounded verdict, review scope, artifact/revision identity, and assessment/evidence time before any score.
- [x] **UX-02**: The first viewport shows at most three release-prioritized actions, each linked to valid evidence, plus the dominant blockers or unanswered release questions.
- [x] **UX-03**: Required evidence completeness displays completed/attention/not-run/not-provided counts, explicit missing checks, source/tool versions, and freshness without calling attention “passed.”
- [x] **UX-04**: Findings, actions, category claims, questions, and limitations expose copyable stable evidence IDs and working deep links to provenance-bearing details.
- [x] **UX-05**: The BOM default is a release-impact-sorted risk matrix with compact identity, lifecycle, availability, commercial, alternate, and release-impact states; full offer details use progressive disclosure.
- [ ] **UX-06**: Tabs, disclosures, status updates, tables, canvas fallback, focus, keyboard operation, responsive layout, and print output meet the defined WCAG 2.2 acceptance checks.
- [ ] **UX-07**: A redistribution-safe golden corpus and evaluation harness measure 10-second disposition/action comprehension, unknown-state interpretation, traceability, accessibility, and 10-to-10,000-line/finding report scale.

### Supply Snapshot and Risk

- [x] **SUP-01**: Supply snapshot v2 validates schema version, sizes, timestamps, enums, unique identities/observations, nonnegative nullable quantities, decimal-string money, sorted breaks, and expiry before evaluation.
- [x] **SUP-02**: Supply identity is exact raw and canonical manufacturer+MPN; multiple candidates, manufacturer conflicts, not-found, provider errors, and not-checked are distinct and never first-result selected.
- [x] **SUP-03**: Per-provider observations preserve provider, canonical/original seller, authorization, SKU, packaging, region, stock status/value, MOQ, order multiple, factory lead time, price breaks/currency, retrieved/upstream times, and provenance.
- [x] **SUP-04**: Mouser, DigiKey, and LCSC each show checked/not-checked/error independently; aggregator omission is never treated as zero stock or proof that the named distributor was checked.
- [x] **SUP-05**: Availability risk compares authorized usable stock and commercial constraints with required build quantity derived from BOM quantity, build quantity, and explicit attrition/buffer inputs.
- [x] **SUP-06**: Lifecycle normalizes active/new/NRND/last-time-buy/EOL/obsolete/unknown while preserving raw/source/time and surfacing cross-source conflicts.
- [x] **SUP-07**: Pricing is applicable only when quantity, MOQ, order multiple, packaging, region, and currency match; incomparable currencies and missing fields remain not checked.
- [x] **SUP-08**: Provider terms profiles gate query, memory/disk retention, HTML embedding, sharing, and expiry/redaction; unauthorized scraping and durable restricted-data embedding are impossible paths.
- [x] **SUP-09**: Provider/manufacturer suggestions remain provenance-labeled candidates; only an explicitly approved alternate with evidence references can mitigate release risk.
- [x] **SUP-10**: Sanitized contract fixtures cover ambiguity, duplicates, stale/upstream-old observations, partial provider failure, authorization, packaging, commercial applicability, conflicts, terms restrictions, and v1 migration.

### KiCad Schematic and Cross-Artifact Evidence

- [x] **SCH-01**: Inventory recognizes `.kicad_sch` and selects a coherent root/project while preserving child sheets, libraries, variables, and project context under existing bounds.
- [x] **SCH-02**: Hierarchy evidence uses occurrence/UUID paths and reports missing children, unresolved variables, ambiguous roots, broken instances, and unsupported cycles without reference-only conflation.
- [x] **SCH-03**: Supported KiCad projects run native `kicad-cli sch erc` with exact tool/version, preserve non-excluded markers and exclusions, and report timeout/tool/version failure as not run.
- [x] **SCH-04**: Supported KiCad board reviews run native schematic parity where available and preserve the exact source pair/revision and parity markers.
- [x] **SCH-05**: Native exports/source evidence cover fitted/DNP symbols, fields, footprint assignment, pin types, power intent/flags, connectivity, and occurrence paths, with source parsing limited to facts native outputs omit.
- [x] **SCH-06**: Reconciliation detects supported schematic↔PCB↔BOM↔placement reference/UUID, value, footprint, fitted/DNP, pin-pad, net, quantity, and stale-revision mismatches.
- [x] **SCH-07**: Deterministic schematic gate families meet the adjudicated precision threshold before blocking; decoupling/interface/power heuristics remain confidence-labeled and non-blocking until separately promoted.
- [x] **SCH-08**: Altium native files and generic exported netlists expose explicit capability/limitation states and never imply native ERC, hierarchy, parity, or source-aware validation that did not run.

### Canonical Manufacturing Evidence and Gerber Baseline

- [x] **FAB-01**: One canonical model represents product identity, layer system, fixed-point geometry/transforms, tools/drills/routes, profile, connectivity, assembly, construction, constraints, provenance, omissions, and conflicts.
- [x] **FAB-02**: Input adapters declare provided capabilities and analyzers declare required capabilities so unsupported evidence becomes not checked rather than pass.
- [x] **FAB-03**: A bounded production parser handles supported RS-274X geometry including units, formats, apertures/macros, interpolation/arcs, regions, polarity, transforms, and step-repeat without silent critical-record loss.
- [x] **FAB-04**: Gerber X2/Job attributes establish layer roles and retain net/component/pin semantics only when explicitly and completely supplied.
- [x] **FAB-05**: A bounded Excellon adapter preserves units, tools, plated/non-plated state when supplied, drill/slot/rout geometry, and layer spans or explicit unknowns.
- [x] **FAB-06**: Gerber/X2+Excellon package completeness and source↔release-package reconciliation report layer, outline, drill, extent, and available connectivity mismatches with provenance.
- [x] **FAB-07**: Current filename/token screening is labeled partial and non-approval evidence until real parsing succeeds; Gerber remains the supported fabrication baseline.
- [x] **FAB-08**: Official/sanitized corpus, mutation, malformed, truncation, and resource-bound fixtures verify parser completeness and deterministic output.

### Intelligent Interchange Decision

- [x] **FMT-01**: ODB++ receives a documented legal/license, representative-corpus, semantic-conformance, virtual-archive security, performance, and maintenance feasibility result.
- [x] **FMT-02**: IPC-2581 receives an equivalent standard/XSD-rights, representative-corpus, schema/semantic-conformance, hostile-XML security, performance, crate maturity, and maintenance feasibility result.
- [x] **FMT-03**: A checkpoint compares ODB++ and IPC-2581 against identical canonical capabilities and records adopt-one, adopt-both, or no-go without changing Gerber baseline policy. **Disposition: no-go for both formats for this release.**
- [x] **FMT-04**: Any adopted intelligent-format adapter reaches the canonical model with explicit capabilities, omissions, provenance, bounds, corpus tests, and no format-badge shortcut to approval. **Disposition: Not Applicable under no-go; no adapter was adopted or implemented.**
- [x] **FMT-05**: If neither format passes, the product reports both as unsupported/not checked and retains native KiCad plus Gerber/X2+Excellon as the strongest path. **Disposition: Complete because current product behavior already satisfies the no-go path; format presence cannot improve approval.**

### Advanced DFM and Assembly Decisions

- [ ] **DFM-01**: Shared geometry analyzers report measured clearance, annular ring, copper-to-edge, mask sliver, paste/mask relationship, outline, and drill/tool issues only when required capabilities are present.
- [ ] **DFM-02**: Stackup, thickness/material, drill-span, impedance/special-process, and profile/order requirements show evidence, conflicts, and explicit confirmation gaps without fabricated defaults. **Plan 07-07 is complete after one independent `BLOCK` review and one remediation pass; final Phase 7 closure remains in Plan 07-11.**
- [ ] **DFM-03**: Assembly analyzers cover placement/BOM population, side/rotation, paste availability, courtyard/access/test-point risks, and package/footprint consistency with source-linked locations. **Implemented through Plan 07-09; Plan 07-09 independent review and final Phase 7 closure remain pending.**
- [ ] **DFM-04**: Net-aware return-path, high-current, creepage, differential, thermal, and interface checks declare assumptions/capabilities and remain inference-labeled unless deterministic evidence is validated. **Plan 07-09 added the sole bounded named/versioned intent seam used by later inference families; no family has human approval.**
- [ ] **DFM-05**: Each analyzer family has adjudicated positive/hard-negative/mutation fixtures and reports precision/recall; only families meeting policy can block release.
- [ ] **DFM-06**: Category/disposition actions prioritize the smallest release-unblocking fix and never let an analyzer score override missing required evidence or the approval gate.

### Hardened Release and Skill Adoption

- [ ] **REL-01**: Archive/format/provider boundaries enforce compressed and actual bytes, entries, path/depth/Unicode/case uniqueness, ratios, nesting, references/cycles, allocations, features, and time budgets without extracting untrusted trees.
- [ ] **REL-02**: Fuzz/property/adversarial tests cover all newly supported parsers and snapshot validators with no panics, traversal, uncontrolled expansion, hangs, or false completion.
- [ ] **REL-03**: Benchmarks enforce declared review and HTML-render budgets on representative small, large, and 10,000-line/finding corpora while preserving deterministic IDs/output.
- [ ] **REL-04**: Self-contained HTML clearly identifies sensitive embedded design data and applies provider terms redaction/omission policy before writing the file.
- [ ] **REL-05**: CLI, schemas, report-contract reference, examples, viewer fixtures, and `review-pcb-dfm` skill agree on one released version and decision-first workflow.
- [ ] **REL-06**: The skill's final delivery reports disposition, strongest action, missing/stale evidence, native tool/version, profile, artifact identity, and disclaimer without unsupported readiness claims.
- [ ] **REL-07**: CI/release verification runs Rust tests, Node viewer/golden/accessibility tests, schema equality, corpus checks, fuzz smoke/regressions, and performance gates with documented commands.
- [ ] **REL-08**: A release-candidate review of representative KiCad and fabrication packages meets the comprehension, traceability, honesty, accessibility, parser, security, and performance acceptance metrics before adoption.

## v2 Requirements

### Broader EDA and Operations

- **EDA2-01**: Source-aware Altium validation through a licensed, dependable native automation path when customer demand justifies it.
- **EDA2-02**: Additional EDA-native adapters beyond KiCad, based on measured usage and legal/maintenance feasibility.
- **OPS2-01**: Organization policy packs and signed review attestations after the local evidence contract is stable.
- **SUP2-01**: Historical supply trends and manufacturer PCN/PDN verification from separately licensed authoritative data.
- **DFM2-01**: Device-datasheet-aware electrical rule packs after provenance and qualification workflows exist.

## Out of Scope

| Feature | Reason |
| --------- | -------- |
| Automatic alternate approval | Equivalence requires explicit engineering authority and evidence |
| Unofficial distributor scraping | Violates the official-provider and terms-first product boundary |
| ODB++-only release policy | Gerber interoperability remains mandatory and format does not imply completeness |
| Compliance certification | RateMyPCB is a preflight and decision aid, not a certifying authority |
| Hosted design upload by default | Conflicts with local-first sensitive-design containment |
| Rebuilding KiCad ERC/DRC semantics | Native released tooling is the primary authority |

## Traceability

| Requirement | Phase | Status |
| ------------- | ------- | -------- |
| EVID-01 | Phase 1 | Complete |
| EVID-02 | Phase 1 | Complete |
| EVID-03 | Phase 1 | Complete |
| EVID-04 | Phase 1 | Complete |
| EVID-05 | Phase 1 | Complete |
| EVID-06 | Phase 1 | Complete |
| EVID-07 | Phase 1 | Complete |
| EVID-08 | Phase 1 | Complete |
| UX-01 | Phase 2 | Complete |
| UX-02 | Phase 2 | Complete |
| UX-03 | Phase 2 | Complete |
| UX-04 | Phase 2 | Complete |
| UX-05 | Phase 2 | Complete |
| UX-06 | Phase 2 | Pending |
| UX-07 | Phase 2 | Pending |
| SUP-01 | Phase 3 | Complete |
| SUP-02 | Phase 3 | Complete |
| SUP-03 | Phase 3 | Complete |
| SUP-04 | Phase 3 | Complete |
| SUP-05 | Phase 3 | Complete |
| SUP-06 | Phase 3 | Complete |
| SUP-07 | Phase 3 | Complete |
| SUP-08 | Phase 3 | Complete |
| SUP-09 | Phase 3 | Complete |
| SUP-10 | Phase 3 | Complete |
| SCH-01 | Phase 4 | Complete |
| SCH-02 | Phase 4 | Complete |
| SCH-03 | Phase 4 | Complete |
| SCH-04 | Phase 4 | Complete |
| SCH-05 | Phase 4 | Complete |
| SCH-06 | Phase 4 | Complete |
| SCH-07 | Phase 4 | Complete |
| SCH-08 | Phase 4 | Complete |
| FAB-01 | Phase 5 | Complete |
| FAB-02 | Phase 5 | Complete |
| FAB-03 | Phase 5 | Complete |
| FAB-04 | Phase 5 | Complete |
| FAB-05 | Phase 5 | Complete |
| FAB-06 | Phase 5 | Complete |
| FAB-07 | Phase 5 | Complete |
| FAB-08 | Phase 5 | Complete |
| FMT-01 | Phase 6 | Complete |
| FMT-02 | Phase 6 | Complete |
| FMT-03 | Phase 6 | Complete — no-go for both formats this release |
| FMT-04 | Phase 6 | Not Applicable — no adapter adopted |
| FMT-05 | Phase 6 | Complete — unsupported/not-checked path verified |
| DFM-01 | Phase 7 | Pending |
| DFM-02 | Phase 7 | Plan 07-07 complete; final Phase 7 closure pending |
| DFM-03 | Phase 7 | Plan 07-09 implemented; independent review and final closure pending |
| DFM-04 | Phase 7 | Plan 07-09 declaration seam complete; inference families and final closure pending |
| DFM-05 | Phase 7 | Pending |
| DFM-06 | Phase 7 | Pending |
| REL-01 | Phase 8 | Pending |
| REL-02 | Phase 8 | Pending |
| REL-03 | Phase 8 | Pending |
| REL-04 | Phase 8 | Pending |
| REL-05 | Phase 8 | Pending |
| REL-06 | Phase 8 | Pending |
| REL-07 | Phase 8 | Pending |
| REL-08 | Phase 8 | Pending |

**Coverage:**

- v1 requirements: 60 total
- Mapped to phases: 60
- Unmapped: 0 ✓

---
*Requirements defined: 2026-08-26*
*Last updated: 2026-09-01 after Plan 07-09 implementation gates passed; the repository lead's independent review and final Phase 7 closure remain pending.*
