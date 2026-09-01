---
phase: 6
slug: intelligent-interchange-decision-gate
status: complete
nyquist_compliant: true
wave_0_complete: true
created: 2026-08-30
---

# Phase 6 — Validation Strategy

> Prove the no-go decision changes no product behavior: both formats remain unsupported/not checked, native KiCad plus Gerber/X2+Excellon remains strongest, and format presence cannot improve approval.

## Test Infrastructure

| Property | Value |
| --- | --- |
| **Framework** | Private Rust crate tests with locked audited archive dependencies, plus existing public unsupported-format regression and GSD structure/state checks |
| **Private crate** | `/Users/mattiafiumara/repos/ratemypcb-odbpp-private` (`publish = false`); final PRIVATE research SHA `a4216f6909754155555e9290c2ec84e0eb16d267` remains quarantined |
| **Private quick run** | `cargo fmt --check && cargo test --no-fail-fast && cargo clippy --all-targets -- -D warnings` |
| **Public non-integration run** | `CARGO_NET_OFFLINE=true cargo test -p ratemypcb-cli --test decision_report --locked schematic_doctor_and_snapshot_expose_capabilities_without_client_policy` |
| **Plan check** | GSD plan-structure and reference checks for `06-01-PLAN.md` through `06-08-PLAN.md`, plus summary schema checks for `06-01-SUMMARY.md` and `06-08-SUMMARY.md` |
| **Full checkpoint** | Private quick run + ignored release synthetic evidence under `/usr/bin/time -l` + primary LSP/focused lens + offline dependency/source audit + public non-integration run + planning assertions + private remote equality/visibility + `git diff --check` + staged-file check |

## Sampling Rate

- After each private parser change: run formatting, all private tests, strict Clippy, and primary LSP diagnostics.
- After canonical-shaped evidence changes: run repeated-parse/result equality checks and the existing public unsupported-format regression.
- Before handoff: validate all eight plan structures/references, both new summary schemas, requirement/state/roadmap consistency, planning-only changes, exact public HEAD, and zero staged files.
- A later ODB++ response/corpus/conformance packet is evidence for a separately authorized reopening, not a Phase 6 blocker.
- No watch mode and no network-dependent test gate.

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirements | Threat refs | Secure behavior | Test type | Automated command | Status |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 06-01-01 | 06-01 | 1 | FMT-01, FMT-02 | T6-01..T6-05 | Unresolved legal/corpus/conformance/security/performance/dependency evidence stays visible and release-ineligible | source + regression | `CARGO_NET_OFFLINE=true cargo test -p ratemypcb-cli --test decision_report --locked schematic_doctor_and_snapshot_expose_capabilities_without_client_policy` plus planning assertions | ✅ green |
| 06-01-02 | 06-01 | 1 | FMT-03 | T6-01..T6-05 | Human selects exactly one release outcome | human decision | No-go selected for both ODB++ and IPC-2581 for this release | ✅ complete |
| 06-01-03 | 06-01 | 1 | FMT-03 | T6-03, T6-05 | Record no-go and route only to FMT-05 verification | planning-state check | Decision and 06-08 plan validated | ✅ complete |
| 06-02-01 | 06-02 | 2 | FMT-03, FMT-04 | T6-02..T6-05 | Existing matrix parser remains bounded, deterministic, explicit about unhandled fields, and limited to stable unpacked trees | unit + static | Private quick run | ✅ green |
| 06-02-02 | 06-02 | 2 | FMT-03, FMT-04 | T6-03..T6-05 | Exact MM/INCH fixed point plus L/P/A/S profile/basic geometry retain source spans and fail closed on malformed paths | unit + integration | 23 private tests, strict Clippy, formatting, primary LSP | ✅ green |
| 06-02-03 | 06-02 | 2 | FMT-03, FMT-04 | T6-01..T6-05 | Private facts map deterministically to policy-free capability/omission/conflict evidence without public integration or approval effects | equality + regression | 23 private tests plus locked/offline public non-integration regression | ✅ green after focused review remediation |
| 06-03-01 | 06-03 | 3 | FMT-03, FMT-04 | T6-03..T6-05 | Arc-aware envelopes use exact checked fixed point and fail closed on overflow/sub-picometre results | unit + adversarial | Quarter/half/CW+CCW wraparound/cardinal/extreme/aperture-rotation/reorder/repeat tests; 33-test private run | ✅ green |
| 06-03-02 | 06-03 | 3 | FMT-03, FMT-04 | T6-03..T6-05 | Straight profile topology is Complete only after bounded winding/intersection/containment proof; all other subsets stay typed Partial | unit + adversarial | Winding/bow-tie/touch/zero-area/concave/vertex-ray/outside/intersecting-hole/resource/arc-partial tests | ✅ green |
| 06-03-03 | 06-03 | 3 | FMT-03, FMT-04 | T6-01..T6-05 | One fresh read-only correctness review, one remediation pass, full private/public/planning gates, and no product/Git effect | review + regression | `/tmp/ratemypcb-odbpp-private-phase6-plan03-review.md` plus full checkpoint gates | ✅ green after one review/remediation pass |
| 06-04-01 | 06-04 | 4 | FMT-03, FMT-04 | T6-03..T6-05 | One exact checked predicate engine validates bounded line-only profile and general-surface topology | unit + adversarial | Multi-island/hole/winding/crossing/containment/work-budget synthetic tests plus 43-test private run | ✅ green |
| 06-04-02 | 06-04 | 4 | FMT-03, FMT-04 | T6-03..T6-05 | Only nonempty complete surfaces project deterministic association facts; arc/work/compressed/unsupported/malformed cases keep GeometryRegions Partial | mapping + equality | Typed omission, unsupported-only, compressed two-layer, malformed-public, reorder, and repeat checks | ✅ green after focused review remediation |
| 06-04-03 | 06-04 | 4 | FMT-03, FMT-04 | T6-01..T6-05 | One fresh correctness review, one remediation pass, and full private/public/planning gates without product/Git effect | review + regression | `/tmp/ratemypcb-odbpp-private-phase6-plan04-review.md` plus full checkpoint gates | ✅ green after one review/remediation pass |
| 06-05-01 | 06-05 | 5 | FMT-03, FMT-04 | T6-03..T6-05 | One absolute deadline/cancellation control covers discovery, every relevant filesystem call, 64 KiB reads, and matrix work; interruption wins over failed I/O after the call | unit + integration | Full private suite plus `interruption_after_failed_filesystem_call_precedes_io_error` | ✅ green |
| 06-05-02 | 06-05 | 5 | FMT-03, FMT-04 | T6-03..T6-05 | Profile/geometry/topology/mapping share the control, stage accounting is exact, and final omission collection cannot return after cancellation | unit + equality | Profile-only stage test, barrier-synchronized active parse/final-map cancellation, and repeated package/evidence/account equality | ✅ green after focused review remediation |
| 06-05-03 | 06-05 | 5 | FMT-03, FMT-04 | T6-01..T6-05 | Project-authored hostile/scaling evidence remains bounded/non-representative and the reviewed commit is preserved only in the PRIVATE repository | hostile + measurement + review | 49 default tests; ignored release evidence under `/usr/bin/time -l`; review artifact; private local/tracking/remote/API SHA equality | ✅ green after one review/remediation pass |
| 06-06-01 | 06-06 | 6 | FMT-03, FMT-04 | T6-02..T6-05 | Byte-identified gzip/USTAR/GNU-long-name ingestion validates raw paths/types/budgets, ignores stored modes, uses no-follow owner-only RAII materialization, and carries the existing control | unit + integration | Raw-entry/path/type/limit/reorder/permission/cancellation/deadline/cleanup regressions plus strict Clippy/LSP | ✅ green after review remediation |
| 06-06-02 | 06-06 | 6 | FMT-03, FMT-04 | T6-01..T6-05 | The exact private official archive is fully ingested/accounted and its sub-picometre semantic error remains fail closed with no capability result | official evidence | `cargo test --locked --test archive_evidence -- --nocapture` plus isolated release `/usr/bin/time -l` | ✅ green; evidence only |
| 06-06-03 | 06-06 | 6 | FMT-03, FMT-04 | T6-01..T6-05 | Locked dependencies are registry/permissive/advisory-clean; one fresh review and one remediation pass close all valid findings before an exact PRIVATE push | audit + review + regression | 56 default tests, Clippy/LSP/lens, RustSec/license/source/duplicate audits, public non-integration regression, remote equality | ✅ green after one review/remediation pass |
| 06-07-01 | 06-07 | 7 | FMT-03 evidence | T6-03..T6-05 | Exact precision degradation remains quarantined research | private evidence | Accepted summary at PRIVATE SHA `a4216f6…` | ✅ green |
| 06-08-01 | 06-08 | 8 | FMT-05 | T6-03 | Existing CLI/doctor surface reports both formats unsupported and unavailable analysis not checked | focused regression | Locked/offline `decision_report` regression | ✅ green |
| 06-08-02 | 06-08 | 8 | FMT-03, FMT-04, FMT-05 | T6-01..T6-05 | Planning records no-go, N/A adapter, and unchanged strongest path | planning checks | GSD, diff, HEAD, staging, path guards | ✅ green |

## Threat Boundaries

| Ref | Boundary | Required result |
| --- | --- | --- |
| T6-01 | Legal overclaim | Separate public access/open-format evidence from project-specific implementation and asset rights; unresolved is not “illegal.” |
| T6-02 | Corpus rights | Downloadability and human-directed private storage/execution never imply third-party CI, sanitization, or redistribution permission. The exact official archive receipt/run is quarantined in the PRIVATE repository; other ambiguous bytes remain outside both repositories. |
| T6-03 | Semantic authority | Generic XML, `.tgz`, format names, schema validity, or parser success cannot complete canonical capabilities or improve approval. |
| T6-04 | Hostile input | The unpacked bounds remain. Unix single-member gzip + exact USTAR + bounded GNU-long-name ingestion now adds raw path/type/permission/nesting/count/size/ratio/overflow/trailing-data/no-follow/write-failure/cancellation/deadline/cleanup proof. `.Z`, other containers/extensions, non-Unix ingestion, concurrent hostile mutation, arc-bearing topology proof, XML, and representative hostile/cancellation-latency proof remain unsupported/unproven. |
| T6-05 | Supply chain/maintenance | PRIVATE HEAD `a4216f6909754155555e9290c2ec84e0eb16d267` is `publish = false` and locks four direct/16 transitive registry packages with permissive choices, zero duplicates, and zero RustSec warnings. Public dependency/adapter adoption still requires rights, production audit, ownership, and checkpoint eligibility. |

## Wave 0 Requirements

The existing Rust test harness covers this slice. Four narrowly scoped maintained dependencies fill stdlib gaps for gzip, TAR parsing, secure temporary ownership, and Unix no-follow flags. Tests construct project-authored hostile archives at runtime plus read the one committed private official receipt.

## Manual-Only Verification

| Behavior | Requirement | Why Manual | Test Instructions |
| --- | --- | --- | --- |
| Select adopt ODB++, adopt IPC-2581, adopt both, or no-go | FMT-03 | Product/legal cost and adoption authority belong to a human | Complete: human selected no-go for both formats for this release. |

## Validation Sign-Off

- [x] Existing unsupported-format regression passes (1 passed; 6 filtered).
- [x] Phase 5 planning truth is reconciled to 6/6 complete.
- [x] `06-CONTEXT.md`, `06-RESEARCH.md`, and Plans 06-01 through 06-08 exist; all eight plan structures/references and both new summary schemas pass the explicit GSD checks.
- [x] Human authorization for local private parser implementation is recorded separately from FMT-03 adoption and public rights.
- [x] The private crate remains `publish = false`. One official archive is retained/read under explicit human direction only in the authorized PRIVATE repository; four locked maintained dependencies implement gzip/TAR/temp/no-follow boundaries. No specification/schema, persistent extracted tree, other sample, or generated corpus artifact was added.
- [x] Matrix plus exact units/profile/line/pad/arc/surface parsing and deterministic AdapterResult-shaped evidence pass 23 focused tests, formatting, strict Clippy, and primary LSP diagnostics.
- [x] The independent review's valid P1/P2/P3 roots were remediated with focused regressions: physical lines, compressed ancestor symlinks/classification, profile identity/polarity/cutouts, canonical row range, matrix capability provenance, conservative generic roles, bounded virtual paths, and path-neutral diagnostics.
- [x] The locked/offline public unsupported-format regression passes (1 passed; 6 filtered), proving no RateMyPCB integration or support behavior changed.
- [x] Unsupported constructs and the stable-tree/concurrent-mutation ceiling remain explicit in code, evidence omissions, and README.
- [x] No public-worktree files are staged. Authorized private Git mutations are ordinary fast-forward commits only: reviewed parser checkpoints, the corpus receipt, and Plan 06-06's audited dependency/archive checkpoint. No force/tag/release/visibility change, publication, public integration, or Phase 7 change occurred.
- [x] Plan 06-03 exact directed-arc/supported-primitive bounds and bounded line-profile topology pass 33 project-authored synthetic tests, formatting, strict Clippy, and focused LSP.
- [x] The single fresh correctness review found no P0/P1 issue and two optional P2 coverage gaps; one remediation pass added aperture/rotation, winding/zero-area/concave/vertex-ray, and clockwise arc symmetry regressions without a second review ceremony.
- [x] The locked/offline public regression still passes (1 passed; 6 filtered); GSD state/roadmap/four-plan structure, `git diff --check`, zero-staged, public-product, and Phase 7 checks all pass.
- [x] Plan 06-04 reuses the exact profile predicates for deterministic multi-island/general-surface associations and passes 43 private tests, formatting, strict Clippy, and focused primary LSP with zero diagnostics.
- [x] Its single fresh review found no P0, two valid P1 capability-honesty defects, and two valid P2 fail-closed/coverage gaps. One remediation pass added partial-before-zero counting, compressed-region effects, true cross-file budget coverage, and malformed-public-value guards/regressions; no second review was requested.
- [x] Focused lens found no unresolved Plan 06-04 production issue: the topology-specific public-value panic was removed, remaining production `expect`s guard parser-validated internal invariants, and `rust-unwrap` blockers are intentional `#[cfg(test)]` setup/assertion failures inspected/dispositioned as false-positive rather than suppressed in source.
- [x] Plan 06-04 private/public/dependency/planning/diff/staged/product/Phase-7 gates pass; no publication, release, integration, corpus, or dependency action occurred.
- [x] Plan 06-05 carries one absolute deadline/cancellation control through discovery/read/matrix/profile/geometry/topology/mapping, reports typed fail-closed interruption, and preserves deterministic byte/line/record/feature/vertex/topology/operation accounting.
- [x] Its single fresh review found three valid P1 control-contract defects and one valid P2 scheduler race. One bounded remediation pass fixed post-filesystem-call ordering, profile stage charging, final mapping completion, and barrier-synchronized cancellation, with focused regressions and no second broad review.
- [x] The full private gate passes 49 default tests (46 library + 3 integration), strict Clippy, formatting, offline dependency audit, and zero primary LSP diagnostics; focused lens has no production finding and reports only intentional test setup/assertion unwraps.
- [x] The ignored release evidence passes with exact small/medium/large operation counts, 541/1,380/8,167 µs local elapsed observations, 0.26 s process real time, and 21,889,024-byte maximum RSS, all explicitly non-representative with no threshold.
- [x] The accepted private research culminates at SHA `a4216f6909754155555e9290c2ec84e0eb16d267`; it remains quarantined and grants no product claim.
- [x] The retained `designodb_rigidflex.tgz` is 11,653,177 bytes with SHA-256 `e67cbbdf95044b0a961fea956ef0e292121755b5de413e95a3265269eb24ee78`. Gzip integrity and bounded metadata/path/type/expansion checks pass without filesystem extraction; Gitleaks 8.30.1 scanned all 57,106,098 streamed bytes with zero findings. All 839 regular entries carry mode `0777`, recorded as a quarantine warning.
- [x] Post-receipt private gates pass: formatting, 49 default tests (46 library + 3 integration; 1 ignored scaling case), strict all-target/all-feature Clippy, provenance Markdown diagnostics, `git diff --check`, exact two-file staging, and remote archive blob/size verification.
- [x] Plan 06-06 byte-identifies gzip method 8, exact USTAR magic/version, and 182 bounded GNU long-name records; it safely materializes 1,190 logical entries while rejecting hostile raw paths/types/modes/nesting/limits/trailing data and preserving cleanup/interruption.
- [x] Plan 06-06's official run records 1,372 raw entries and 58,189,824 decoded bytes, succeeds through matrix parsing, then fails typed at profile line 12's sub-picometre coordinate with zero package/capabilities.
- [x] The fresh security review found no P0, three P1, and two P2 issues. One remediation pass added raw exact header/type accounting, bounded GNU long names, Unix no-follow opens, raw path parsing, non-Unix fail-closed behavior, typed write evidence, and focused regressions; no second broad review was requested.
- [x] The Plan 06-06 full private gate passes 56 default tests (52 library + 1 official + 3 synthetic; 1 ignored scaling case), strict Clippy, formatting, locked/offline tests/tree, zero LSP diagnostics, no production lens finding, zero Gitleaks findings, zero duplicate versions, and zero RustSec warnings across 20 locked non-root packages.
- [x] The locked/offline public regression remains 1 passed/6 filtered; current GSD state/roadmap consistency, `git diff --check`, zero-staged/public-product, and zero-Phase-7 checks pass.
- [x] FMT-03 Complete: human selected no-go for both formats for this release.
- [x] FMT-04 Not Applicable: no adapter was adopted.
- [x] FMT-05 Complete: unchanged product behavior reports unsupported/not checked, preserves native KiCad plus Gerber/X2+Excellon, and gives format presence no approval effect.

**Approval:** Phase 6 is Complete at 8/8 plans. PRIVATE SHA `a4216f6909754155555e9290c2ec84e0eb16d267` remains quarantined research only. Future ODB++ reply/corpus/conformance evidence may support reopening but is not a current blocker.
