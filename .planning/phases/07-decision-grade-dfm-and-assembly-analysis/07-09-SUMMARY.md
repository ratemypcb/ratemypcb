---
phase: 07-decision-grade-dfm-and-assembly-analysis
plan: 09
subsystem: bounded-assembly-inference
status: implementation-gates-green/review-pending
review: pending-repository-lead
requires:
  - phase: 07-decision-grade-dfm-and-assembly-analysis
    plan: 08
    provides: Accepted native assembly facts and deterministic assembly families.
  - phase: 06-intelligent-interchange-decision-gate
    plan: 08
    provides: No-go closure for ODB++ and IPC-2581 this release.
provides:
  - One in-place bounded extension of the existing DFM declaration seam for named inference intent.
  - EvidenceOnly assembly access against an explicit process/tool envelope.
  - EvidenceOnly testpoint access against explicit canonical target-net authority and a probe/process envelope.
  - Complete seven-family DFM-03 matrix and two-family corpus metrics.
affects: [07-10, 07-11]
requirements-advanced: [DFM-03, DFM-04, DFM-05]
completed: 2026-09-01
---

# Phase 7 Plan 09 summary

IMPLEMENTATION GATES GREEN. INDEPENDENT REVIEW PENDING.

The repository lead owns the one independent Plan 07-09 review. This feature lead did not start a review and stopped before Plan 07-10.

## Outcome

- Extended the existing `DfmDeclarations` root in place. `--dfm-declarations` and `ReviewOptions::dfm_declarations` remain the only input path.
- Added 15 exact inference record kinds for assembly process/tool, probe, canonical target nets, signal edge-rate/frequency, reference-plane discontinuity, current/process copper, voltage/creepage/material/environment/coating, differential impedance/skew, power/thermal geometry, and interface connector/pin intent.
- Every record retains the original declaration path, byte digest, producer/version, structural record, named model/version, and board applicability in existing canonical constraints. No second flag, input root, board model, constructor field, dependency, or viewer policy was added.
- The parser rejects unknown fields, record/model versions, IDs, units, ranges, non-finite values, partial state, duplicate authority, stale metadata, missing limits, and over-limit records, targets, limits, or parameters before comparison.
- `assembly.access.v1` compares exact fixed-point 2D component-copper geometry to other fitted components and the exact profile. It runs only with complete placement/profile/component facts and one named process/tool diameter and clearance envelope. Evidence names the 2D model and every source assumption.
- `assembly.testpoint-access.v1` adds complete connectivity and pin facts, one named probe/process envelope, and explicit `net-v1-<sha256>` target authority derived from canonical connectivity feature IDs. TP-like references and net names have no target authority.
- Both access families compare obstructions only on the authoritative placement side. Coincident geometry on the opposite side does not block a tool or probe approach.
- Profile qualification uses one exact exterior contour and exact cutout membership. Concave exteriors, cutouts, outside placements, or unavailable exact membership return `not_checked` with no partial finding.
- Generated observations and findings have fixed count and serialized-byte limits. Coverage evidence has a separate fixed byte limit. Any limit failure returns `not_checked` with no partial findings.
- Exact-threshold and safe cases are clean observations. Missing, partial, dangling, unsupported, off-grid, names-only, or unbounded facts are `not_checked` and create no finding.
- Both families have metrics but no reviewed promotion metadata and no human checkpoint approval. Every finding is `EvidenceOnly`. Forged `Blocking` fails report validation.

## TDD and corpus

- The declaration test first failed because `inferenceRecords` was unknown to the existing parser.
- Access and testpoint production tests were added before family integration; the initial access filter failed because the family coverage did not exist.
- Remediation tests cover top and bottom opposite-side controls, concave and cutout profile membership, outside placement, atomic output limits, and analyzer-derived corpus labels and mutation status.
- The final corpus reports:

| Family | TP | FP | FN | TN | Precision | Recall | Mutations | Gate |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `assembly.access.v1` | 2 | 0 | 0 | 2 | 1.000 | 1.000 | 14/14 | EvidenceOnly |
| `assembly.testpoint-access.v1` | 2 | 0 | 0 | 2 | 1.000 | 1.000 | 14/14 | EvidenceOnly |

The DFM-03 matrix covers population, side/rotation, paste availability, native courtyard, footprint-string parity, access, and testpoint access.

## Files changed for Plan 07-09

- `crates/ratemypcb-core/src/dfm.rs`
- `crates/ratemypcb-core/tests/dfm_release.rs`
- `crates/ratemypcb-cli/tests/decision_report.rs`
- `tests/fixtures/dfm/declarations.json`
- `tests/fixtures/dfm/manifest.json`
- `tests/fixtures/dfm/assembly-targets.json`
- `.planning/REQUIREMENTS.md`
- `.planning/ROADMAP.md`
- `.planning/STATE.md`
- `.planning/phases/07-decision-grade-dfm-and-assembly-analysis/07-VALIDATION.md`
- `.planning/phases/07-decision-grade-dfm-and-assembly-analysis/07-09-SUMMARY.md`

## Verification

- Exact Plan 07-09 CLI declaration filter passed at `5d26a8a47003bb83f7faf139e0358b990833b582`.
- Exact declaration, access, testpoint, corpus, metrics, mutation, profile, limit, DFM-03 matrix, and forged-impact filters passed.
- Full DFM release passed 46/46.
- Full workspace passed 305 Rust tests with `CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_OPT_LEVEL=1`.
- Node report/viewer tests passed 31/31.
- `cargo fmt --all -- --check` passed.
- `CARGO_BUILD_JOBS=1 cargo clippy --workspace --all-targets --locked -- -D warnings` passed.
- Authoritative schema generation remained byte-equal to `schemas/report-2.0.json`.
- `git diff --check origin/phase7-dfm-assembly...HEAD` passed.

The plan references `~/.codex/gsd-core/workflows/execute-plan.md` and `templates/summary.md`; neither file is installed. Repository planning state, validation, and summary conventions were used directly.

## Boundaries and residual risk

- The access model is an explicit 2D component-copper union envelope. It is not body-height, collision, field, ampacity, impedance, or thermal simulation. Unsupported geometry stays `not_checked`.
- The later electrical, differential, thermal, and interface families only received bounded declaration records. Plan 07-10 and Plan 07-11 analysis was not started.
- ODB++ and IPC-2581 remain no-go. No private parser, private corpus, intelligent-format support claim, install, dependency, viewer, ODB, stage, commit, or push action occurred.
- `07-PROMOTION-CHECKPOINT.md` still approves no inference family.
- Independent review is pending, so this implementation is not review-accepted or merge-ready.

## Handoff

- Worktree: `/home/mattia/RateMyPCB/worktrees/phase7-07-09-remediation`
- Verification HEAD: `5d26a8a47003bb83f7faf139e0358b990833b582`
- Base: `8eafaea078a492ed19d3e7dd0a02be3764bad59f`
- Git boundary: remediation and base integration are committed; review status remains pending
- Next action: repository lead runs one independent Plan 07-09 review. Plan 07-10 remains untouched.
