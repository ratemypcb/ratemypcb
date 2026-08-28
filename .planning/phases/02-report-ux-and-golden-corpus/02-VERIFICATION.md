---
phase: 02-report-ux-and-golden-corpus
verified: 2026-08-26T20:03:52Z
status: human_needed
score: 3/5 roadmap success criteria passed; 2/5 retain human-only acceptance
behavior_unverified: 4
---

# Phase 2 Verification Report

**Goal:** Understand the decision/action in one screen, inspect provenance, and use the report accessibly at normal and overload scale.  
**Status:** Implementation and automated/browser-runtime must-haves pass. Representative-human and assistive-technology acceptance remains deferred and is not a blocker to independent Phase 3 engineering.

## Roadmap truths

| # | Truth | Status | Evidence |
| --- | --- | --- | --- |
| 1 | ≥90% identify disposition and first action within 10 seconds | HUMAN NEEDED | Protocol exists; 0 representative trials. Chrome 320px and 1440px checks show the disposition and first action, but this is not comprehension evidence. |
| 2 | ≤3 linked actions/questions and honest required-evidence summary precede scores | PASS | Contract budget and HTML ordering tests pass; Chrome 320px screenshot showed disposition and first action without horizontal overflow. |
| 3 | Every active-report claim/limitation resolves to provenance | PASS | Corpus evaluator covers assessment, required evidence, findings, coverage, and limitations. Chrome claim navigation materialized and focused the target. Legacy unlinked limitations remain visible with an explicit legacy warning rather than being silently removed. |
| 4 | BOM handles 10–10,000 rows/findings with sorting/filtering/progressive access | PASS | Tests import the exact production view-model. Chrome generated overload showed 100 initial BOM rows and 100 evidence records, 5,734 initial DOM nodes, 299ms useful decision, and 54ms filter/deep-link/category interaction. |
| 5 | Keyboard/screen-reader/browser a11y/responsive/print acceptance | HUMAN NEEDED | Static semantics and a Chrome runtime/responsive/PDF subset pass. Hands-on keyboard, screen reader, approved browser a11y audit, Safari, and complete viewport/print matrix remain unrun. |

## Review-finding disposition

- Fixed stale CI schema target and added all Phase 2 Node suites.
- Bounded evidence records and category claim links in 100-item batches; deep links materialize only their target.
- Shared the production BOM filter/sort model directly with Node tests.
- Made manifest expected dispositions/counts/actions executable assertions and added coverage claims to provenance checks.
- Removed unsafe module-scope fragment decoding.
- Replaced false evidence timestamps with freshness plus an honest missing-time label; null evidence timestamps render `Not provided`.
- Print now expands details and materializes report content before printing.
- Legacy-valid limitations remain visible with an explicit missing-reference label.
- Added viewer `aria-pressed`, explicit zoom names, non-color selection mark, corrected Layers heading depth, and darkened attention text.

## Requirements

- **UX-01–UX-05:** Implementation and automated/browser-runtime evidence complete.
- **UX-06:** Human/AT acceptance remains deferred.
- **UX-07:** Corpus, exact production model, and browser overload measurement complete; representative comprehension and screen-reader/a11y acceptance remain deferred.

## Automated gates

- `cargo fmt --all -- --check` — passed.
- `cargo clippy --all-targets --locked -- -D warnings` — passed.
- `cargo test --all --locked` — passed: 61 Rust tests, 0 failed.
- `node --test tests/board-view.test.mjs tests/report-contract.test.mjs tests/report-ux.test.mjs` — passed: 23 tests/subtests, 0 failed.
- Generated schema 2.0 diff — passed.
- `git diff --check` — passed.
- `git diff --cached --name-only` — empty.

## Deferred human acceptance

- Ten representative-engineer comprehension and unknown-interpretation trials.
- Hands-on full keyboard/no-trap and clipboard-permission check.
- NVDA/VoiceOver qualitative screen-reader path.
- Approved real-browser automated accessibility audit.
- Safari and remaining viewport/A4/Letter print-preview matrix.

## Residual risks

- No WCAG conformance, representative comprehension, screen-reader, or cross-browser claim is made.
- Printing a deliberately huge 10,000-row report materializes all rows and can produce a very large PDF; interactive screen use remains bounded.
- Older report 2.0 limitations without references are retained but explicitly labeled as legacy and unlinked.
