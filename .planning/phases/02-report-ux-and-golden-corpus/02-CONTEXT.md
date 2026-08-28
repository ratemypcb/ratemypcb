# Phase 2: Report UX and Golden Corpus — Context

**Gathered:** 2026-08-26  
**Status:** Ready for execution

## Phase boundary

Make the validated Phase 1 report/assessment understandable, traceable, accessible, printable, and usable at 10–10,000 BOM lines/findings. The viewer remains a presentation-only consumer. Supply-provider semantics, schematic analysis, and manufacturing parser depth remain later phases.

## Locked decisions

- Preserve decision → actions/questions → required evidence → secondary scores in source and visual order.
- Show at most three validated actions; never truncate or repair invalid assessment data in the viewer.
- Required evidence distinguishes completed, attention, not run, and not provided; unknown and stale remain explicit and never pass-like.
- All visible claims and limitations use the Phase 1 global evidence namespace and provenance records. Evidence IDs are copyable and deep links focus their targets.
- BOM ordering uses an authoritative report-provided release-impact state. The viewer may sort/filter that state but never derive a new release decision or risk grade.
- Render BOM rows in bounded batches; counts stay honest and every filtered row remains reachable.
- Use native HTML/CSS/JavaScript and existing Rust/Node harnesses; add no dependency.
- Static automation is not browser, screen-reader, print-preview, performance, or human-comprehension evidence. Record those checks honestly as human-needed when unavailable.

## Implementation seams

- `local-viewer.html`: landmarks, tabs, BOM controls/caption/status, canvas fallback.
- `local-viewer.js`: shared evidence navigation, required-evidence taxonomy, policy-neutral BOM sorting/filtering/batches, tabs and fallback status.
- `local-viewer.css`: first viewport, focus/forced colors, responsive and print.
- `ratemypcb-core/src/lib.rs`: authoritative not-provided execution state, BOM release-impact state, evidence-linked limitation references.
- `tests/report-ux.test.mjs` and `tests/fixtures/report-ux/`: redistribution-safe corpus and dependency-free structural/scale evaluator.

## Deferred

- Runtime browser automation dependency, provider-specific BOM fields, live supply data, participant recordings, and Phase 3+ evidence depth.
