# Roadmap: RateMyPCB Decision-Grade Release Review

## Overview

The milestone first makes release truth explicit and testable, then turns that contract into a comprehensible report. It deepens supply and schematic evidence as independent vertical outcomes, establishes a canonical manufacturing model with real Gerber/X2+Excellon parsing, makes an evidence-gated ODB++ versus IPC-2581 decision, adds calibrated DFM/assembly analysis, and finally hardens and adopts the complete workflow. Gerber remains the baseline throughout; missing evidence always closes approval.

## Phases

- [x] **Phase 1: Decision-First Evidence Contract** - Freeze measurable release-decision semantics and prove them through one report-to-HTML tracer. (completed 2026-08-26)
- [ ] **Phase 2: Report UX and Golden Corpus** - Make the first screen immediately actionable, traceable, accessible, and measurable at real report scales.
- [x] **Phase 3: Supply Snapshot v2 and Demand-Aware Risk** - Deliver exact-identity, named-provider, terms-aware BOM sourcing evidence without invented substitutes. (completed 2026-08-26)
- [x] **Phase 4: KiCad Schematic Release Evidence** - Add native schematic/ERC/parity and source-aware cross-artifact consistency with honest boundaries. (completed 2026-08-27)
- [x] **Phase 5: Manufacturing Evidence Model and Gerber Baseline** - Parse real Gerber/X2+Excellon into one canonical model and reconcile release artifacts. (completed 2026-08-30)
- [x] **Phase 6: Intelligent Interchange Decision Gate** - Evaluate ODB++ and IPC-2581 symmetrically and ship only adapters that earn adoption. (completed 2026-08-30; no-go for both formats this release)
- [ ] **Phase 7: Decision-Grade DFM and Assembly Analysis** - Turn validated geometry, construction, connectivity, BOM, and placement facts into calibrated release actions.
- [ ] **Phase 8: Hardened Release and Skill Adoption** - Prove parser security, performance, privacy, version alignment, and end-to-end skill usefulness.

## Phase Details

### Phase 1: Decision-First Evidence Contract

**Goal**: Engineers receive an unambiguous, evidence-linked release disposition whose approval semantics cannot be improved by missing evidence, proven through a vertical self-contained HTML tracer.  
**Depends on**: Nothing (first phase)  
**Requirements**: EVID-01, EVID-02, EVID-03, EVID-04, EVID-05, EVID-06, EVID-07, EVID-08  
**Success Criteria** (what must be TRUE):

1. The same golden report expresses separate risk, coverage, confidence, freshness, and approval fields, and every missing-required-evidence mutation closes approval without lowering risk.
2. Semantically identical findings retain IDs across ordering/prose mutations, while different canonical occurrences receive different IDs and expose provenance.
3. Report bytes bind to an assessment whose verdict, actions, categories, and questions all resolve to evidence; a broken reference is rejected.
4. A deterministic command produces self-contained HTML containing one unambiguous disposition and evidence links, while ambiguity, false passes, broken links, and information overload fail fixtures.
5. Generated and checked-in report schemas match, with the dirty 0.2 schema/version drift made explicit rather than silently accepted.

**Research**: Complete enough to execute from `01-RESEARCH.md`; no external dependency choice in this phase.  
**Checkpoint**: Review contract fixture/metric thresholds after the tracer, before downstream UI and evidence phases treat them as stable.  
**Plans**: 3/3 plans complete

Plans:

- [x] 01-01-PLAN.md — Establish evidence semantics, stable identities, provenance, gate policy, and schema equality through a golden report tracer.
- [x] 01-02-PLAN.md — Carry the decision summary and evidence links through assessment validation and self-contained HTML.
- [x] 01-03-PLAN.md — Add adversarial contract/evaluation fixtures for ambiguity, false passes, traceability, and information load.

**UI hint**: yes

### Phase 2: Report UX and Golden Corpus

**Goal**: An engineer can understand the release decision and first action in one screen, inspect every claim's evidence, and use the complete report accessibly at small and overload scale.  
**Depends on**: Phase 1  
**Requirements**: UX-01, UX-02, UX-03, UX-04, UX-05, UX-06, UX-07  
**Success Criteria** (what must be TRUE):

1. Representative users identify disposition and first action within 10 seconds in at least 90% of trials, with scores visually secondary.
2. The first viewport shows no more than three evidence-linked actions plus blockers/questions and an honest required-evidence/freshness summary.
3. Every visible claim and limitation deep-links to a provenance-bearing evidence record; automated tests find zero broken links.
4. The BOM risk matrix remains usable from 10 to 10,000 lines/findings through sorting, filtering, and progressive disclosure without hiding `not checked`.
5. Keyboard, screen-reader spot checks, automated accessibility, responsive, and print acceptance all pass with zero serious/critical automated findings.

**Research**: UI-spec and usability protocol required before implementation.  
**Checkpoint**: Human verification of the decision hierarchy and representative usability results.  
**Plans**: TBD  

- [x] 02-01-PLAN.md
- [x] 02-02-PLAN.md
- [x] 02-03-PLAN.md

**UI hint**: yes

### Phase 3: Supply Snapshot v2 and Demand-Aware Risk

**Goal**: Engineers can judge exact-part lifecycle and build-quantity availability at Mouser, DigiKey, and LCSC from fresh, lawful, provenance-preserving evidence, with every unavailable check explicit.  
**Depends on**: Phases 1-2  
**Requirements**: SUP-01, SUP-02, SUP-03, SUP-04, SUP-05, SUP-06, SUP-07, SUP-08, SUP-09, SUP-10  
**Success Criteria** (what must be TRUE):

1. Ambiguous, duplicate, conflicting, not-found, error, unknown, and stale snapshots cannot become exact matches or passes.
2. Each BOM line shows exact manufacturer+MPN, required quantity, lifecycle, and independent Mouser/DigiKey/LCSC check states with per-offer commercial evidence.
3. Stock, MOQ, order multiple, packaging, lead time, and price applicability are evaluated against build demand without cross-currency or missing-field guesses.
4. Provider terms policy prevents forbidden queries/storage/HTML embedding/sharing and yields provider-scoped `not checked` instead of scraping.
5. Provider suggestions never reduce risk; only an explicit approved alternate with evidence can do so.

**Research**: Mandatory account-schema and terms review for each provider before its adapter is planned.  
**Checkpoint**: Blocking legal/product decision per provider for query, retention, embedding, and sharing permissions.  
**Plans**: TBD  

- [x] 03-01-PLAN.md
- [x] 03-02-PLAN.md
- [x] 03-03-PLAN.md

**UI hint**: yes

### Phase 4: KiCad Schematic Release Evidence

**Goal**: KiCad users receive native, hierarchy-aware schematic/ERC/parity evidence and explicit schematic↔PCB↔BOM↔placement consistency, while unsupported EDA/export claims remain bounded.  
**Depends on**: Phases 1 and 3  
**Requirements**: SCH-01, SCH-02, SCH-03, SCH-04, SCH-05, SCH-06, SCH-07, SCH-08  
**Success Criteria** (what must be TRUE):

1. A coherent hierarchical KiCad project runs supported native ERC and parity with complete tool/version/failure/exclusion evidence.
2. Reused sheets, multi-unit symbols, power intent, fitted/DNP state, and occurrence paths are not conflated by reference-only matching.
3. Seeded supported schematic↔PCB↔BOM↔placement mismatches are all detected and linked to source occurrences.
4. Tool failure, unsupported version, missing child/context, Altium native source, and lossy netlist export remain explicit not-run/not-checked capability states.
5. Only deterministic schematic families meeting adjudicated precision can block; heuristics stay inference-labeled and non-blocking.

**Research**: Supported released KiCad major matrix and native JSON/export fixture runs required.  
**Checkpoint**: Promote individual schematic checks to release-blocking only after corpus metrics pass.  
**Plans**: 3/3 plans complete  

Plans:

- [x] 04-01-PLAN.md — Establish the bounded KiCad 8/9/10 native runner, ERC/parity semantics, normalized markers, failure states, provenance, and sanitized corpus.
- [x] 04-02-PLAN.md — Add hierarchy occurrence inventory, explicit source/export facts, exact-first cross-artifact reconciliation, and bounded non-KiCad capabilities.
- [x] 04-03-PLAN.md — Integrate CLI/schema/viewer/skill surfaces and close adversarial, gate-policy, and full-phase regressions.

**UI hint**: yes

### Phase 5: Manufacturing Evidence Model and Gerber Baseline

**Goal**: Engineers receive geometry-backed Gerber/X2+Excellon release-package evidence and native-source reconciliation through one canonical, provenance-aware manufacturing model.  
**Depends on**: Phases 1 and 4  
**Requirements**: FAB-01, FAB-02, FAB-03, FAB-04, FAB-05, FAB-06, FAB-07, FAB-08  
**Success Criteria** (what must be TRUE):

1. Real RS-274X and Excellon fixtures produce bounded canonical geometry/tools/layers/profile evidence with explicit parser capabilities and omissions.
2. X2/Job net/component/pin semantics are claimed only when present and complete; legacy/file-inferred roles are visibly weaker.
3. Missing/incompatible layer, outline, drill, extent, and available connectivity evidence prevents fabrication approval and links to source records.
4. Native KiCad and release-package mismatches are reported rather than silently merged.
5. Malformed, truncated, mutation, official/sanitized corpus, and resource-bound fixtures terminate deterministically without critical-record loss.

**Research**: Package-legitimacy, official Ucamco corpus, Excellon approach, and performance/security discovery required.  
**Checkpoint**: The rejected crates.io 0.5.0 candidate remains historical STOP evidence. The exact RateMyPCB fork branch/push and later production adoption are separate blocking-human decisions.  
**Plans**: 6/6 plans complete

Plans:

- [x] 05-01-PLAN.md — Establish the canonical provenance-aware manufacturing model, fixed-point geometry/capability contracts, deterministic identity, resource budgets, and honest legacy-screening downgrade. Independently accepted; FAB-01, FAB-02, and FAB-07 complete.
- [x] 05-02-PLAN.md — Create the explicitly authorized `ratemypcb/gerber-parser` fork and sibling clone, validate upstream tokenizer commit `f4160c7…` plus one bound non-breaking outer-error funnel, then gate its exact signed commit/protected push and immutable dev-only candidate adoption. Completed with human PASS for exact head `54004bc…`.
- [x] 05-03-PLAN.md — Promoted exact approved head `54004bc…` under PASS_F416 and closed bounded, error-accounted fixed-point RS-274X semantics for FAB-03 after fresh independent ACCEPT with empty P0/P1/P2.
- [x] 05-04-PLAN.md — Complete-only X2/Gerber Job authority, strict/named-legacy XNC, G75 full-circle bounds, conservative proof-less block geometry, and rectangle-only Complete package profiles passed the ordinary bounded independent product review; FAB-04/FAB-05 complete.
- [x] 05-05-PLAN.md — Native KiCad/release-package reconciliation, geometry/model identity, and one carried interruptible deadline passed the same review; FAB-06 complete.
- [x] 05-06-PLAN.md — Hostile/resource/corpus/determinism and ordinary repository gates passed with one bounded independent product review; FAB-08 and Phase 5 complete. The former packet/freeze/GPG/canonical-review/zero-findings machinery remains withdrawn.

### Phase 6: Intelligent Interchange Decision Gate

**Goal**: RateMyPCB makes an evidence-based ODB++ versus IPC-2581 adoption decision and exposes any adopted format through the same canonical capabilities without weakening Gerber support or approval honesty.  
**Depends on**: Phase 5  
**Requirements**: FMT-01, FMT-02, FMT-03, FMT-04, FMT-05  
**Success Criteria** (what must be TRUE):

1. ODB++ and IPC-2581 each have comparable legal, corpus, conformance, security, performance, and maintenance evidence with unresolved gaps visible.
2. A recorded checkpoint selects adopt-one, adopt-both, or no-go using identical canonical capability criteria.
3. Any adopted adapter passes representative corpus and hostile-input tests and reports omissions; format presence alone never raises approval.
4. If no adapter passes, reports honestly say unsupported/not checked while native KiCad plus Gerber/X2+Excellon remains fully supported.

**Research**: Symmetric evidence found neither ODB++ nor IPC-2581 adoption-ready for this release.
**Checkpoint**: Complete — the human selected no-go for both formats for this release. FMT-03 and FMT-05 are complete; FMT-04 is not applicable because no adapter was adopted.
**Plans**: 8/8 complete. Exact decision receipts: external Phase 6 `06-01-SUMMARY.md` and `06-08-SUMMARY.md`. The later private-use evidence does not reopen Phase 6.

#### Deferred private ODB++ integration lane

Private development and internal processing of customer-supplied files may continue after this release. This lane does not change Plans 07-10 or 07-11, Phase 8 release criteria, the public support statement, or FMT-04.

The lane must pass these gates before private product integration:

1. Resolve publication rights.
2. Assemble a rights-cleared representative corpus.
3. Prove semantic conformance for the supported subset.
4. Prove hostile-input security.
5. Set and pass performance and resource limits.
6. Assign maintenance ownership.
7. Approve the private deployment boundary.
8. Approve customer-data handling.
9. Approve the product disclaimer.
10. Approve each exact product claim.

The sanitized decision record is `.planning/decisions/2026-09-02-odbpp-private-internal-use.md`.

### Phase 7: Decision-Grade DFM and Assembly Analysis

**Goal**: Engineers receive source-linked, capability-aware fabrication and assembly findings that prioritize release-unblocking actions and distinguish deterministic violations from conservative inference.  
**Depends on**: Phases 3-6  
**Requirements**: DFM-01, DFM-02, DFM-03, DFM-04, DFM-05, DFM-06  
**Success Criteria** (what must be TRUE):

1. Geometry checks report measured clearance, ring, edge, mask/paste, outline, and drill evidence only when prerequisites exist.
2. Construction/profile/order gaps and conflicts are explicit for stackup, material, thickness, drill spans, impedance, finish, and special processes.
3. Assembly evidence links BOM population, placement, orientation, paste, footprint/package, access, and test-point risks to source locations.
4. Net-aware/thermal/high-current/return/creepage/interface conclusions show assumptions and remain inference-labeled until validated.
5. Each blocker family meets corpus precision policy, and top actions remain the smallest release-unblocking steps rather than score optimization.

**Research**: Per-analyzer engineering references, capability prerequisites, and adjudication protocol required.  
**Checkpoint**: Human approval before any inference family becomes release-blocking.  
**Plans**: 9/11 implemented. Plans 07-01 through 07-08 are accepted/reviewed. Plan 07-09 implementation gates are green and the repository lead's one independent review is pending. Plan 07-10 was not started.
**UI hint**: yes

### Phase 8: Hardened Release and Skill Adoption

**Goal**: The complete local review is secure, bounded, deterministic, accessible, version-aligned, and demonstrably useful enough for the `review-pcb-dfm` skill to become the release-review default.  
**Depends on**: Phases 1-7  
**Requirements**: REL-01, REL-02, REL-03, REL-04, REL-05, REL-06, REL-07, REL-08  
**Success Criteria** (what must be TRUE):

1. Adversarial archives, formats, and provider snapshots cannot traverse, exhaust uncontrolled resources, hang, panic, or falsely report completion under declared budgets.
2. Small, large, and overload benchmarks meet declared review/render limits while IDs and outputs remain deterministic.
3. Offline HTML identifies sensitive embedded data and excludes/redacts provider observations when terms disallow persistence.
4. CLI, generated/checked-in schemas, report contract, viewer fixtures, CI, release metadata, and skill instructions agree on one released workflow/version.
5. Representative release-candidate reviews meet comprehension, traceability, honesty, accessibility, parser, security, and performance gates before skill adoption.

**Research**: Threat-model and release-gate refresh required after all adapters/analyzers are known.  
**Checkpoint**: Final human release/skill-adoption gate; no automatic publish or commit.  
**Plans**: TBD  
**UI hint**: yes

## Progress

| Phase | Plans Complete | Status | Completed |
| ------- | ---------------- | -------- | ----------- |
| 1. Decision-First Evidence Contract | 3/3 | Complete | 2026-08-26 |
| 2. Report UX and Golden Corpus | 3/3 | In Progress | |
| 3. Supply Snapshot v2 and Demand-Aware Risk | 3/3 | Complete | 2026-08-26 |
| 4. KiCad Schematic Release Evidence | 3/3 | Complete | 2026-08-27 |
| 5. Manufacturing Evidence Model and Gerber Baseline | 6/6 | Complete | 2026-08-30 |
| 6. Intelligent Interchange Decision Gate | 8/8 | Complete — no-go | 2026-08-30 |
| 7. Decision-Grade DFM and Assembly Analysis | 9/11 | In Progress — Plan 07-09 implementation green; independent review pending; Plan 07-10 not started | - |
| 8. Hardened Release and Skill Adoption | 0/TBD | Not started | - |
