---
phase: 2
slug: report-ux-and-golden-corpus
status: planned
nyquist_compliant: true
wave_0_complete: true
created: 2026-08-26
---

# Phase 2 — Validation Strategy

| Layer | Command | Scope |
| --- | --- | --- |
| Core contract | `cargo test -p ratemypcb-core --locked decision_contract` | not-provided, limitation refs, authoritative BOM impact, schemas |
| Viewer/corpus | `node --test tests/report-contract.test.mjs tests/report-ux.test.mjs tests/board-view.test.mjs` | corpus hashes, traceability, static a11y, scale model |
| CLI tracer | `cargo test -p ratemypcb-cli --test decision_report --locked` | validated payload and self-contained HTML |
| Full | `cargo fmt --all -- --check && cargo test --all --locked` | workspace regression |

Each plan runs its focused command, then the combined Node/Rust gates at phase end. No watch mode or package installation.

## Human-needed evidence

Runtime browser keyboard, screen-reader, responsive, print-preview, DOM timing, automated browser accessibility, and representative-user comprehension follow the checked-in protocols. Static checks must not be labeled WCAG conformance or usability proof.
