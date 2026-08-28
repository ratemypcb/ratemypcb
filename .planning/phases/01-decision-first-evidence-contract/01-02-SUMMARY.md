---
phase: 01-decision-first-evidence-contract
plan: 02
subsystem: cli-viewer-contract
tags: [rust, javascript, html, exact-digest, provenance, offline]
requires: [01-01]
provides:
  - Actual CLI report-to-digest-to-assessment-to-self-contained-HTML tracer
  - Decision-first report landmarks with secondary scores
  - Shared deterministic evidence-reference anchor mapping and provenance details
affects: [01-03, report-ux]
actuals:
  tasks: 2
  commits: 0
tech-stack:
  added: []
  patterns: [validated-data-only viewer, public evidence ID anchors, repository-relative CLI golden]
key-files:
  created: [crates/ratemypcb-cli/tests/decision_report.rs]
  modified: [crates/ratemypcb-cli/assets/local-viewer.html, crates/ratemypcb-cli/assets/local-viewer.css, crates/ratemypcb-cli/assets/local-viewer.js, crates/ratemypcb-cli/src/viewer.rs, .planning/phases/01-decision-first-evidence-contract/01-02-PLAN.md]
key-decisions:
  - "Disposition leads the report and snapshot title; scores remain secondary."
  - "JavaScript renders validated report/assessment fields and does not derive release, category, or BOM policy."
  - "Rust verifies reference integrity and shared anchor-helper wiring without claiming runtime DOM execution."
requirements-completed: [EVID-04, EVID-05, EVID-07]
status: complete
completed: 2026-08-26
---

# Phase 1 Plan 02: Decision-First HTML Tracer Summary

**The real CLI now carries unchanged report bytes through exact digest binding, validated assessment, and a decision-first self-contained HTML report.**

## Accomplishments

- Added a dependency-free integration tracer that runs the compiled `review`, `digest`, and `render --assessment` commands from the repository root with a repository-relative fixture identity.
- Put disposition, scope, selected artifact, evidence time, rationale, actions, required-evidence completeness/freshness, and only then scores into the HTML source order.
- Removed viewer-owned approval/risk/category/BOM scoring and pass-like fallbacks; rendered core and assessment values directly.
- Added visible evidence IDs, complete supplied provenance, and one URL-safe deterministic anchor helper shared by claim links and evidence targets.
- Covered valid rendering plus altered digest, invalid report, and broken evidence-reference rejection.

## Files Created/Modified

- `crates/ratemypcb-cli/assets/local-viewer.html` — decision-first semantic landmarks, evidence details, and policy-neutral BOM surface.
- `crates/ratemypcb-cli/assets/local-viewer.css` — decision hierarchy and evidence/provenance presentation.
- `crates/ratemypcb-cli/assets/local-viewer.js` — validated-data rendering, shared evidence anchors, and preserved board/Gerber behavior.
- `crates/ratemypcb-cli/src/viewer.rs` — disposition-first escaped snapshot title and updated offline/policy tests.
- `crates/ratemypcb-cli/tests/decision_report.rs` — actual CLI tracer and boundary rejection tests.
- `.planning/phases/01-decision-first-evidence-contract/01-02-PLAN.md` — records superseding spike corrections without overclaiming browser DOM execution.
- `.planning/phases/01-decision-first-evidence-contract/01-02-SUMMARY.md` — this execution record.

## Verification

- `cargo test -p ratemypcb-cli --test decision_report --locked tracer` — pass (1 test).
- `cargo test -p ratemypcb-cli --test decision_report --locked assessment` — pass (1 test).
- `cargo test -p ratemypcb-cli --test decision_report --locked` — pass (2 tests).
- `cargo test -p ratemypcb-cli --locked` — pass (13 tests total; 0 failed).
- `cargo test --all --locked` — pass (60 tests total; 0 failed).
- `node --test tests/board-view.test.mjs` — pass (1 test).
- `node --check crates/ratemypcb-cli/assets/local-viewer.js` — pass.
- Rust/JavaScript primary LSP diagnostics — clean; pi-lens diagnostics — clean.
- `rustfmt --edition 2024 --check` on touched Rust files and `git diff --check` — pass.
- `cargo fmt --all -- --check` — fails only on pre-existing unauthorized formatting drift in `crates/ratemypcb-cli/src/main.rs`, `crates/ratemypcb-core/src/lib.rs`, and `crates/ratemypcb-core/src/stackup.rs`; these files were not changed by this plan.

## Deviations and Residual Risks

- Per the validated correction, no browser/jsdom dependency was added and Rust tests do not claim JavaScript-created anchors were executed. Dependency-free runtime deep-link/keyboard verification remains for Plan 01-03's Node evaluator or Phase 2.
- The existing assessment validator does not enforce a global three-action maximum; this plan's validated tracer contains one action and the viewer does not truncate validated input. Core was outside the authorized file set.
- No files outside the authorized plan set were changed, and no commit was created.

## Next Phase Readiness

Plan `01-02` is complete. Plan `01-03`, wave progression, and state/roadmap updates remain with the repository manager.
