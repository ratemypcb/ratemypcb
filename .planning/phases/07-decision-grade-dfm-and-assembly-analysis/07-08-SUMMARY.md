---
phase: 07-decision-grade-dfm-and-assembly-analysis
plan: 08
subsystem: deterministic-assembly-correlation
status: reviewed-remediated
review: blocked-findings-remediated
reviewer: f52187cd-e86f-4076-9b03-df0b50f1ad8a
second_review: not-requested
requires:
  - phase: 07-decision-grade-dfm-and-assembly-analysis
    plan: 07
    provides: Accepted construction confirmation and central qualification policy.
  - phase: 06-intelligent-interchange-decision-gate
    plan: 08
    provides: No-go closure for ODB++ and IPC-2581 this release.
provides:
  - Policy-free native KiCad assembly placement and courtyard evidence.
  - EvidenceOnly side/rotation, paste availability, native courtyard, and footprint-string families.
  - Four-family assembly corpus with positive, hard-negative, unsupported, and mutation rows.
  - One completed remediation pass for all four valid independent-review findings.
affects: [07-09, 07-10, 07-11]
requirements-advanced: [DFM-03, DFM-05]
completed: 2026-09-01
---

# Phase 7 Plan 08 summary

REVIEWED AND REMEDIATED. THE FOUR VALID BLOCK FINDINGS ARE CLOSED LOCALLY. NO SECOND REVIEW WAS REQUESTED.

Independent reviewer `f52187cd-e86f-4076-9b03-df0b50f1ad8a` returned BLOCK. This fresh owner applied exactly one remediation pass and reran the full gate set.

## Corrected outcome

- Native KiCad normalization no longer drops `board_only` footprints. Every footprint participates in assembly population. Only explicit `dnp` and `exclude_from_pos_files` attributes control fitted state and completeness. A mixed fitted, board-only, DNP, and excluded production regression now stays Partial when exclusion authority is unresolved.
- `assembly.paste-availability.v1` no longer uses mask and paste maps as each other's completeness authority. It builds the required pad set independently from authoritative copper X2 `SMDPad` component/pin facts, then requires one exact paste geometry for every fitted key. A two-pad case where mask and paste both omit pad 2 is `not_checked`. Missing copper authority and non-SMD copper authority are also `not_checked`.
- Upstream schematic reconciliation now retains typed board, BOM, and netlist footprint comparison receipts. Each receipt carries the occurrence key, field, source kind, expected and actual strings, join and confidence, both source paths and digests, location, and the upstream match result. `assembly.footprint-string-parity.v1` consumes those receipts without rerunning comparison or suffix logic. Missing BOM, missing netlist, missing schematic footprint field, incomplete source identity, or ambiguous join is `not_checked`.
- Native courtyard summaries are suppressed only when a completed courtyard-family result produced matching active replacement findings. Partial execution, failed prerequisites, or ignored courtyard checks preserve the original native finding. The active-overlap plus ignored-missing-courtyard regression proves this path.
- Side/rotation behavior remains unchanged: exact source-linked placement identities, explicit conventions, matching revisions, and modulo-360 microdegree comparison only.
- All four assembly families remain `EvidenceOnly`. No promotion metadata, required coverage, approval policy, dependency, viewer, ODB++, IPC-2581, physical-package, distributor-packaging, or name-similarity inference was added.
- `schemas/report-2.0.json` now includes typed schematic footprint comparison receipts and remains byte-identical to authoritative `report_schema()` output.

## Qualification metrics

| Family | TP | FP | FN | TN | Precision | Recall | Mutations | Promotion |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `assembly.side-rotation.v1` | 2 | 0 | 0 | 2 | 1.000 | 1.000 | 14/14 | none |
| `assembly.paste-availability.v1` | 1 | 0 | 0 | 2 | 1.000 | 1.000 | 14/14 | none |
| `assembly.courtyard-native.v1` | 3 | 0 | 0 | 2 | 1.000 | 1.000 | 14/14 | none |
| `assembly.footprint-string-parity.v1` | 2 | 0 | 0 | 2 | 1.000 | 1.000 | 14/14 | none |

The manifest now names independent paste-requiring-pad authority and per-source footprint comparisons as family prerequisites. Promotion remains absent.

## Plan 07-08 files

- `crates/ratemypcb-core/src/fabrication.rs`
- `crates/ratemypcb-core/src/fabrication/native.rs`
- `crates/ratemypcb-core/src/dfm.rs`
- `crates/ratemypcb-core/src/schematic.rs`
- `crates/ratemypcb-core/src/lib.rs`
- `crates/ratemypcb-core/tests/fabrication_release.rs`
- `crates/ratemypcb-core/tests/dfm_release.rs`
- `schemas/report-2.0.json`
- `tests/fixtures/dfm/manifest.json`
- `tests/fixtures/dfm/assembly-targets.json`
- `.planning/phases/07-decision-grade-dfm-and-assembly-analysis/07-08-SUMMARY.md`

Accepted Plan 07-01 through 07-07 changes remain unstaged in the same worktree.

## Verification

- Final focused side/rotation, paste, courtyard, footprint, board-only normalization, courtyard suppression, and schema commands passed.
- Full DFM release passed 36/36.
- Full fabrication release passed 109/109 with `CARGO_PROFILE_TEST_OPT_LEVEL=1`.
- Full schematic release passed 15/15.
- Full workspace passed 292 Rust tests with `CARGO_PROFILE_TEST_OPT_LEVEL=1 cargo test --workspace --locked`.
- Node report/viewer tests passed 31/31. Viewer syntax check passed.
- `cargo fmt --all -- --check` passed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` passed.
- Authoritative schema generation, byte diff, and schema equality test passed.
- JSON validation passed for 54 files. The intentionally truncated hostile native JSON remains the sole expected syntax failure.
- `git diff --check` passed.

The default debug-profile full fabrication run can hit the pre-existing five-second wall-clock boundary in the 9,999-aperture-block stress control on a loaded host. The optimized full fabrication and workspace runs pass, and all focused Plan 07-08 tests pass under the default profile.

## GSD and boundaries

The plan's referenced `~/.codex/gsd-core/workflows/execute-plan.md` and template are not installed. Direct continuity checks passed: Plan 07-07 is complete, no `07-09-SUMMARY.md` exists, and execution stopped before Plan 07-09.

- Phase 6 receipts still record no-go for ODB++ and IPC-2581.
- No dependency, viewer, intelligent-format, private parser/corpus, required-coverage, promotion, stage, commit, or push change occurred.
- HEAD remains `5e0fa62a5865cdea1a7755c6bedcedab3a64ba07` on `phase7-dfm-assembly`.
- Git index remains empty. All work is unstaged.

## Corrected handoff

- Workspace: `wks_4c340b66223c27bd`
- Worktree: `/Users/mattiafiumara/.paseo/worktrees/3s4r2ob6/phase7-dfm-assembly`
- Reviewer: `f52187cd-e86f-4076-9b03-df0b50f1ad8a`
- State: reviewed and remediated in one pass; all requested gates green; no second review requested
- Git boundary: unstaged only, no commit or push
- Next action: repository lead records this corrected Plan 07-08 handoff. Do not begin Plan 07-09.
