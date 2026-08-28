# Feature Research

**Confidence:** High for user outcomes and current gaps; medium where legal access, corpus quality, or live provider behavior is unverified.

## Decision Surface

The first viewport must answer four questions without interpreting a score:

1. **Can I manufacture/release this?** Explicit `approve`, `revise`, or `blocked` disposition and scope.
2. **Why not?** Dominant evidence-linked blockers and missing required checks.
3. **What next?** At most three ordered actions, each linked to evidence.
4. **How complete/fresh is this?** Required checks completed, `not checked` items, source/tool versions, artifact identity, and observation age.

Scores are secondary. Risk, evidence confidence, coverage, freshness, and approval are distinct.

## Required Feature Families

### Evidence and report contract

- Stable check IDs and deterministic instance IDs derived from canonical source identity, not list order.
- Provenance on each conclusion: artifact/digest, tool/version, source location, observation class, confidence, freshness, and conflicts.
- Every assessment verdict, action, category claim, and structured question references valid evidence IDs.
- Versioned required-check denominator; no `passed`/`clear`/approval for missing, failed, stale, or unsupported required evidence.

### Report experience

- Decision-first one-screen summary, progressive disclosure, copyable deep links, print-complete appendix.
- Accessible tabs/disclosures/tables/focus/status announcements and canvas alternative.
- BOM risk matrix sorted by release impact; expand for exact identity and commercial detail.
- Golden reports from small to overload scale, mutation fixtures, and measured comprehension/traceability/a11y outcomes.

### Supply risk

- Exact raw and canonical manufacturer+MPN identity; ambiguous, not-found, error, and not-checked are distinct.
- Per-provider/per-seller observations for Mouser, DigiKey, and LCSC: authorization, stock versus required build quantity, MOQ, order multiple, packaging/SKU, lead time, price breaks/currency, retrieval/upstream age, provenance, and conflicts.
- Provider/legal policy decides whether a field may be queried, retained, embedded, or shared; no scraping fallback.
- Provider/manufacturer suggestions remain unapproved candidates; only explicit approved alternates mitigate risk.

### Schematic and consistency

- Coherent KiCad root/child hierarchy inventory with occurrence paths.
- Native ERC and supported schematic parity with exact tool/version and failure states.
- Symbols, fields, footprint assignment, power intent, connectivity, DNP/exclusions, and conservative high-confidence heuristics.
- Schematic↔PCB↔BOM↔placement consistency by occurrence/reference/UUID where available.
- Altium native files inventory-only unless licensed native validation evidence exists; exported netlists establish only retained fields.

### Fabrication and DFM

- Canonical provenance-aware manufacturing model for layers, geometry, tools/drills, profile, connectivity, assembly, stackup, constraints, capabilities, and omissions.
- Real bounded Gerber/X2+Excellon parsing and native-source/package reconciliation.
- ODB++ and IPC-2581 evaluated symmetrically; adopt only the adapter that passes all gates, or record no-go.
- Shared capability-gated analyzers for clearance, annular ring, copper-to-edge, mask/paste, outline/drills, stackup conflicts, placement/access, and selected net/return/high-current/creepage/thermal checks.

## Anti-Features

- A green score compensating for missing coverage.
- Aggregate seller stock as proof that all named distributors were checked.
- `0` standing in for unknown stock/lead time/price.
- A provider suggestion presented as an alternate or substitute.
- Filename/token checks labeled as geometric parse or CAM validation.
- Format badge used as evidence completeness.
- Hand-rolled KiCad ERC or source-aware Altium claims.
- Durable restricted provider data in self-contained HTML without permission.

## Measurable Outcomes

- ≥90% of representative users identify disposition and first action within 10 seconds; compare against the current viewer.
- 100% of assessment claims/actions and visible findings resolve to valid provenance-bearing evidence links.
- Zero false passes/approvals from `not_run`, `not_provided`, `not checked`, stale, parser-failed, or unsupported required checks.
- Supported deterministic gate checks reach ≥95% precision on adjudicated goldens before becoming blockers.
- Seeded cross-artifact mismatches are detected in 100% of supported fixtures.
- All report functions work by keyboard; automated accessibility scans have zero serious/critical findings.
- Bounded adversarial inputs terminate within declared budgets without traversal, uncontrolled expansion, panics, or silent critical-record loss.

## Primary Sources

- WCAG 2.2: <https://www.w3.org/TR/WCAG22/>
- W3C tables: <https://www.w3.org/WAI/tutorials/tables/>
- KiCad Schematic Editor: <https://docs.kicad.org/9.0/en/eeschema/eeschema.html>
- Ucamco Gerber X2: <https://www.ucamco.com/en/gerber/demo-1>
- Altium design validation: <https://www.altium.com/documentation/altium-designer/schematic/design-validation>
- Nexar lifecycle: <https://support.nexar.com/support/solutions/articles/101000434626-supply-lifecycle-information-for-parts>
