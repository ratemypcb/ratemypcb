# Phase 1 Research: Decision-First Evidence Contract

**Researched:** 2026-08-26  
**Confidence:** High for current-code seams and contract direction; medium for the final stable-ID encoding until fixtures prove collision/stability behavior.

## Research Summary

The shortest safe path is to deepen the existing `Report`/`Assessment` contract rather than introduce another report layer. Current core already centralizes serialized evidence, required coverage, approval policy, assessment reference validation, schema output, exact-byte digest, and offline snapshot. Phase 1 should make those seams explicit and test them end to end before Phase 2 changes presentation broadly.

The tracer uses existing Rust and browser-native code only. No new dependency is required. One blocked golden from `tests/fixtures/narrow-board.kicad_pcb` can produce deterministic evidence, bind an assessment, render offline HTML, and assert disposition/actions/completeness/evidence anchors. Mutations then prove fail-closed behavior.

## Current-State Findings

- Core emits report schema 1.2 while the skill and report contract require/describes 1.1; `viewer.rs` still embeds a 1.1 test fixture.
- `Finding.id` mixes rule-family and occurrence identity; native summary IDs use enumeration order.
- `Coverage` and `Finding` lack artifact digest, structured location, evidence class, per-item confidence, and freshness.
- Global `confidence` is derived mostly from required coverage and board presence, so it can overstate weak heuristic evidence.
- `approval_eligible` already fails closed on required coverage and medium-or-higher findings; retain this seam and make required-check outcomes more explicit.
- `validate_assessment` validates verdict/category/action refs but assessment questions are bare strings and cannot be traced.
- Viewer computes category/BOM ratings and visually centers rating before disposition; Phase 1 only removes decision recomputation needed by the tracer, leaving full redesign for Phase 2.
- `snapshot` safely escapes embedded JSON and makes a self-contained HTML artifact; reuse it.

## Validation Architecture

### Contract layers

1. **Core unit/property-style table tests** in `crates/ratemypcb-core/src/lib.rs`: coverage/gate truth table, deterministic IDs, provenance validation, assessment reference validation, generated-schema equality.
2. **CLI package integration test** in `crates/ratemypcb-cli/tests/decision_report.rs`: execute review JSON, digest exact bytes, write/validate assessment, render HTML, assert decision landmarks and evidence anchors.
3. **Node contract evaluation** in `tests/report-contract.test.mjs`: mutate normalized report/assessment/summary fixtures and reject ambiguous disposition, false pass from unknowns, broken links, >3 actions, duplicate decision landmarks, and score-before-decision ordering.

### Fast feedback

- Core: `cargo test -p ratemypcb-core --locked decision_contract`
- CLI tracer: `cargo test -p ratemypcb-cli --test decision_report --locked`
- Report evaluation: `node --test tests/report-contract.test.mjs`
- Full phase: `cargo test --all --locked && node --test tests/board-view.test.mjs tests/report-contract.test.mjs`

No browser automation framework is needed for Phase 1. Structural semantic assertions are sufficient for the contract tracer; accessibility/usability automation belongs to Phase 2.

### Golden mutation matrix

| Mutation | Expected result |
| ---------- | ----------------- |
| Required check becomes not-run/not-provided/failed/stale/unsupported | Approval false; risk unchanged; explicit missing coverage |
| Findings reordered or prose changed | Same occurrence IDs |
| Canonical source occurrence changes | Different occurrence ID |
| Unknown assessment evidence ref | Validation error |
| Question lacks evidence ref | Validation error |
| No disposition or competing dispositions | Evaluation failure |
| More than three top actions | Evaluation failure |
| Decision summary lacks completeness/freshness | Evaluation failure |
| Score landmark precedes decision summary | Evaluation failure |
| Generated schema differs from checked-in schema | Test failure |

## Architectural Responsibility Map

| Capability | Owner | Evidence |
| ------------ | ------- | ---------- |
| Risk/coverage/confidence/freshness/approval semantics | Core report policy | Serialized fields + truth-table tests |
| Stable IDs/provenance | Core evidence types/helpers | Stability/collision fixtures |
| Assessment references | `validate_assessment` | Invalid-ref tests including questions |
| Exact-byte binding | CLI `digest_bytes` / render path | CLI integration tracer |
| Self-contained HTML | `viewer::snapshot` | HTML semantic assertions |
| Decision presentation | HTML/JS consumer | One disposition, ≤3 actions, evidence anchors |
| Information-load evaluation | Node contract test | Mutation failures |

The viewer must not own any policy row above presentation.

## Implementation Direction

- Extend the existing DTOs and policy functions; avoid creating an abstraction layer with one implementation.
- Represent a stable rule family separately from occurrence identity. Canonical identity input includes source artifact digest + rule ID + structured location identity, excluding severity and prose.
- Add explicit required-check summary data from core so the viewer does not infer it.
- Make questions structured like actions/categories with evidence refs.
- Choose and document schema compatibility explicitly. If required fields break 1.2 consumers, use a major bump; do not call a breaking contract additive.
- Generate the checked-in schema through the existing CLI/core schema function and compare parsed JSON values in tests to avoid formatting-only failures.
- Keep the first HTML tracer intentionally small: release decision, why, top actions, completeness/freshness, then score. Full design refinements wait for Phase 2.

## Threat Model Inputs

- **Tampering:** assessment references a different report or nonexistent evidence; keep digest and reference validation.
- **Repudiation:** unstable IDs/source omission prevent audit; canonical IDs and provenance mitigate.
- **Information disclosure:** offline HTML embeds design data; Phase 1 preserves escaping/local behavior and marks sensitivity for Phase 8 policy.
- **Denial of service:** oversized summary/actions/evidence arrays create unusable output; bounded top actions and existing input limits, with broader report-scale work in Phase 2/8.
- **Elevation/false authority:** score or missing evidence yields approval; fail-closed gate and decision ordering mitigate.
- **Supply-chain:** no package installation in Phase 1.

## Resolved Questions

1. **New framework?** Resolved: no; current Rust + Node built-in test + native HTML stack is sufficient.
2. **Separate report service/view model?** Resolved: no; core emits the contract and viewer consumes it.
3. **Pixel snapshot required?** Resolved: no for Phase 1; semantic HTML contract assertions are more stable and directly test ambiguity/load.
4. **Can scores remain?** Resolved: yes, as secondary metadata only; they never override disposition/gate.
5. **Should absent evidence lower risk?** Resolved: no; it lowers coverage/confidence and closes approval.

## Gated Questions (Not Phase 1 Blockers)

- Exact ODB++/IPC-2581 adoption — Phase 6.
- Provider storage/display permissions — Phase 3.
- Stable KiCad ERC occurrence fields across released majors — Phase 4.
- Full visual/a11y/usability protocol — Phase 2.

## Open Questions (RESOLVED)

All Phase 1 questions needed for planning are resolved above. The plans select an explicit breaking report/assessment 2.0 contract under D-09 because the new required semantics are not additive; 1.2/1.0 files remain historical, and active-version drift becomes a regression.

## Primary Sources

- WCAG status/structure baseline: <https://www.w3.org/TR/WCAG22/>
- W3C headings: <https://www.w3.org/WAI/tutorials/page-structure/headings/>
- W3C disclosure guidance: <https://www.w3.org/WAI/people-use-web/tools-techniques/presentation/>

## Code Sources

- `crates/ratemypcb-core/src/lib.rs`: DTOs, `review`, `required_coverage`, `approval_eligible`, `validate_assessment`, `report_schema`.
- `crates/ratemypcb-cli/src/main.rs`: `digest_bytes`, `render_snapshot`.
- `crates/ratemypcb-cli/src/viewer.rs`: `snapshot`.
- `crates/ratemypcb-cli/assets/local-viewer.{html,css,js}`: current report consumer.
- `schemas/report-1.2.json`, `schemas/assessment-1.0.json`, `skills/review-pcb-dfm/references/report-contract.md`: current drift.
