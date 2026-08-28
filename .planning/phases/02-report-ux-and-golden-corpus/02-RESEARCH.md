# Phase 2 Research: Report UX and Golden Corpus

**Researched:** 2026-08-26

## Direction

The three embedded viewer assets are the production seam for loopback and self-contained reports. Keep analysis in core; add only authoritative display fields that the current contract lacks. Native controls, semantic HTML, CSS media queries, and dependency-free tests are sufficient.

## Findings

- Decision content is already before scores in source, but masthead/spacing can push useful content below a 720px viewport.
- Evidence links share one anchor helper but lack copy, target focus, hash activation, and return navigation.
- Required evidence currently groups execution only and conflates not-provided with not-run.
- Limitations are plain strings and need core-provided evidence references; viewer fallback inference would violate policy neutrality.
- BOM eagerly creates every row and has no controls. Bounded batches, stable report-provided rank sorting, filtering, honest counts, and Load more provide the smallest scalable implementation.
- Tabs need roving tabindex and Home/End. Canvas needs a textual fallback. CSS needs focus-visible, forced-colors, responsive, reduced-motion, and print treatment.
- Node tests can prove corpus integrity, source/contract structure, sort/filter model invariants, and 10k pure-data scale. They cannot attest browser layout, accessibility tree, keyboard runtime, print preview, or human comprehension.

## Validation architecture

1. Core unit/schema tests protect not-provided, limitation refs, and BOM release-impact data.
2. Node corpus evaluator checks hashes, traceability, unknown honesty, UI structure, and generated 10/100/1k/10k models.
3. Existing CLI integration proves validated exact-byte data reaches self-contained HTML.
4. Human-needed protocols separately cover browser/keyboard/screen-reader/print and 10-second comprehension.

## No new dependencies

Use Rust tests, `node:test`, stdlib crypto/fs/performance, and native browser APIs only.
