---
phase: 06-intelligent-interchange-decision-gate
plan: 05
subsystem: private-odbpp-execution-control
status: complete
tags: [rust, odbpp, deadline, cancellation, resource-accounting, synthetic-evidence]
requires:
  - phase: 06-intelligent-interchange-decision-gate
    plan: 04
    provides: Accepted exact fixed-point, provenance, extent, and line-only topology boundary.
provides:
  - One caller-owned absolute deadline and cooperative cancellation control through private discovery, reads, parsing, topology, and mapping.
  - Typed fail-closed interruption with deterministic stage operation snapshots and package resource metrics.
  - Project-authored hostile/scaling evidence with explicitly non-representative local timing and RSS observations.
  - One fresh correctness review and one bounded remediation pass closing three P1 contract defects and one P2 test race.
  - Reviewed PRIVATE repository checkpoint 07a42c937cf550eeb7c9d5d5c233b474cb386a0d on main.
affects: [FMT-03-evidence, future-FMT-04-candidate]
requirements-completed: []
completed: 2026-08-30
---

# Phase 6 Plan 05: Private Execution-Control and Scaling-Evidence Checkpoint

## Outcome

The quarantined stdlib-only ODB++ parser now carries one caller-owned `ParseControl` from product-root discovery through static path validation, bounded reads, matrix/profile/geometry/topology parsing, and private canonical-shaped mapping. The control contains one optional absolute `Instant`, one cloneable atomic `CancellationToken`, and cumulative deterministic counters for `Discovery`, `Read`, `Matrix`, `Profile`, `Geometry`, `Topology`, and `Mapping`. The deadline is never reset per stage or file.

An observed cancellation returns typed `Cancelled`; an elapsed deadline returns typed `DeadlineExceeded`. Both include the observed stage and cumulative operation snapshot. Parsing and mapping return `Result`, so interrupted work returns no `ParsedPackage` or `AdapterResult` and cannot promote incomplete evidence to `Complete`.

This checkpoint adds no ODB++ record semantics and preserves Plan 06-04's exact fixed-point, provenance, extent, topology, capability, and omission boundaries. It does not establish adoption, conformance, representative performance, safety, rights, public support, integration, distribution, publication, release eligibility, or approval effects. FMT-03, FMT-04, and FMT-05 remain pending.

## Control and accounting contract

- Filesystem calls are checked immediately before and after the call. Results are captured first; interruption is observed after the call before an I/O error is mapped or propagated. `read_dir` construction and every manual `next()` iteration follow the same ordering.
- File bodies are read in at most 65,536-byte chunks with cooperative checks between chunks. One-byte growth detection and reopened-path identity checks remain fail closed.
- Every source-reachable or high-cardinality parser/mapping lane checks the same control, including physical lines, records, features, surfaces, exact extents, quadratic topology pairs/point locations, sorts, mapped facts, and final omission collection.
- Profile source/surface/extent work is charged and reported as `Profile`; layer geometry is charged as `Geometry`; topology proof remains `Topology`.
- Matrix metrics retain raw bytes, physical lines, and records. Package metrics retain matrix and geometry bytes/lines/records plus geometry files, features, contour vertices, and package-carried topology work.
- Successful repeated runs over the same generated package produce equal parsed values, adapter evidence, package metrics, and per-stage operation counts.

Declared limits remain explicit: 4 MiB matrix; 1,400 bytes per physical line; 16,384 matrix records; 256 wrapper entries; 64 KiB read chunks; 4 MiB per geometry file; 20 MiB aggregate geometry; 256 geometry files; 500,000 physical source lines; 400,000 feature records; 419,425 features; 1,000,000 contour vertices; 10,000 symbols; 65,536 attribute entries; 4 KiB attribute payloads; 256-byte virtual paths; 64 steps; 512 layers; 16,384 step/layer slots; 4,096 profile topology segments; and 16,777,216 package-carried topology work units.

## Project-authored hostile and scaling evidence

Regular tests generate all inputs at runtime and cover whitespace/record amplification, a 257-entry wrapper tree, a 16,385-record matrix attempt, a 512-point contour, exact repeated accounting, immediately elapsed deadlines, pre-cancellation, post-filesystem-call interruption precedence, synchronized in-progress profile cancellation, and cancellation at mapping's final post-omission checkpoint. No corpus, specification, schema, sample, fixture asset, or measurement output is committed.

The ignored release case was warmed and run with:

```sh
cargo test --release --test synthetic_evidence --no-run
/usr/bin/time -l cargo test --release --test synthetic_evidence synthetic_scaling_evidence -- --ignored --exact --nocapture
```

Observed on this local machine/process:

| Scale | Feature records | Contour segments | Input bytes | Source lines | Parsed records | Topology work | Operations `(discovery, read, matrix, profile, geometry, topology, mapping)` | Local elapsed |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: |
| small | 100 | 35 | 2,050 | 161 | 152 | 1,241 | `(45, 36, 19, 36, 563, 1,500, 272)` | 541 µs |
| medium | 1,000 | 131 | 17,248 | 1,157 | 1,148 | 17,177 | `(45, 36, 19, 36, 4,547, 19,332, 2,072)` | 1,380 µs |
| large | 10,000 | 515 | 164,704 | 10,541 | 10,532 | 265,241 | `(45, 40, 19, 36, 42,083, 285,780, 20,072)` | 8,167 µs |

`/usr/bin/time -l` reported 0.26 s real and 21,889,024 bytes maximum resident set size for the warmed Cargo/test-harness process (also 5,882,336 bytes `peak memory footprint`). These are reproducible local observations only. Harness/process/machine state is included; there is no invented pass/fail, adoption, release, production-performance, or cancellation-latency threshold.

## Fresh correctness review and one remediation pass

Review artifact: `/tmp/ratemypcb-odbpp-private-phase6-plan05-review.md`.

The single fresh-context read-only review found no P0, three valid P1 contract defects, and one valid P2 test race. Its initial verdict was **BLOCK**. All four findings were closed in one bounded remediation pass; no second broad review or zero-findings loop was run.

| Finding | Fix | Focused regression |
| --- | --- | --- |
| P1: failed filesystem results could bypass the post-call interruption check and surface as I/O errors | Every relevant filesystem result is captured, immediately followed by the carried checkpoint, then mapped/propagated; `read_dir.next()` is manual with before/after checks | `interruption_after_failed_filesystem_call_precedes_io_error` gates the exact post-failed-call checkpoint with a barrier and proves `Cancelled` wins over `Io` |
| P1: profile surfaces and extent loops were charged as `Geometry` | `feature_stage(kind)` is carried through surface parsing, polygon finishing, and physical-extent derivation | `profile_surface_work_stays_profile_staged` proves positive Profile/Topology work and zero Geometry work for a profile-only source |
| P1: high-cardinality omission collection could finish after the last mapping check | Omissions are collected into a local vector, followed by a final `Mapping` checkpoint before constructing `AdapterResult` | `synchronized_cancellation_interrupts_active_parse_and_mapping_completion` gates the deterministic final mapping operation and proves no result is returned |
| P2: integration cancellation depended on scheduler timing and workload size | Racy polling/yield tests were removed; unit-only checkpoint gates use two barriers keyed to an observed stage operation, block active work, cancel, then resume deterministically | The same synchronized regression proves cancellation after profile work begins and at mapping completion without sleeps, yields, or workload-size synchronization |

## Gate results

- Private `cargo fmt --check`: pass.
- Private full default `cargo test --no-fail-fast`: **49 passed, 0 failed, 1 ignored by default** (46 library tests plus 3 regular integration tests; binary/doc targets also pass).
- Private `cargo clippy --all-targets -- -D warnings`: pass.
- `CARGO_NET_OFFLINE=true cargo tree --offline --depth 1`: only local `ratemypcb-odbpp`; `publish = false`.
- Focused primary LSP diagnostics for `src/lib.rs`, `src/geometry.rs`, `src/main.rs`, and `tests/synthetic_evidence.rs`: **0 diagnostics**.
- Focused lens found no production defect. Its structural `rust-unwrap` findings are confined to `#[cfg(test)]` setup/assertion paths (including inherited tests), where panic intentionally fails the test; no source suppression or production behavior change was added.
- Ignored release synthetic evidence: pass with the explicitly non-representative observations above.
- Locked/offline public unsupported-format regression: **1 passed, 0 failed, 6 filtered**.
- Plan 06-05 structure: valid, 3 tasks, zero errors/warnings. Phase completeness correctly remains open only for the separate `06-01` FMT-03 decision plan.
- Dependency/source/credential/prohibited-asset audits, `git diff --check`, and staged-file checks: pass.
- Private repository is `PRIVATE`, default branch `main`, origin `https://github.com/ratemypcb/ratemypcb-odbpp.git`, and local/tracking/remote/API SHA all equal `07a42c937cf550eeb7c9d5d5c233b474cb386a0d`.
- Public worktree remains uncommitted and has zero staged files, zero product-source change, and zero Phase 7 change.

## Private commit receipt

- Repository: <https://github.com/ratemypcb/ratemypcb-odbpp> (`PRIVATE`)
- Branch/default: `main`
- Commit: `07a42c937cf550eeb7c9d5d5c233b474cb386a0d`
- Subject: `feat: add bounded parser execution controls`
- Push: ordinary fast-forward from `9a9ffcbca70b279796d84d2f7f8fdfc51b8091d9` to `07a42c937cf550eeb7c9d5d5c233b474cb386a0d`
- Local worktree after push: clean; no force-push, tag, release, visibility change, external asset, or public-worktree commit.

## Later private official-sample receipt

A later explicit human direction authorized retaining only the original official ODB++Design v8.1 `designodb_rigidflex.tgz` in the existing PRIVATE repository. This is not part of Plan 06-05's parser implementation and changes no code or parser semantics.

- Receipt commit: `83e15f1e07eedb62c9f2fc017a08c0c5138766b8` (`test: add private official ODB++ sample`), ordinary fast-forward from `07a42c937cf550eeb7c9d5d5c233b474cb386a0d`.
- Private path: `tests/private-corpus/official-odbplusplus/designodb_rigidflex.tgz`, with adjacent `PROVENANCE.md` only.
- Exact bytes: 11,653,177; SHA-256 `e67cbbdf95044b0a961fea956ef0e292121755b5de413e95a3265269eb24ee78`.
- Checks without filesystem extraction: gzip integrity pass; 1,190 entries under one relative root; no absolute/traversal paths, links, devices/special types, duplicate/case collisions, nested archives, or expansion blocker; Gitleaks 8.30.1 scanned all 57,106,098 streamed bytes with zero findings.
- Quarantine warning: all 839 regular archive entries carry mode `0777`; no extraction occurred and any future authorized extraction must ignore stored permissions.
- Rights remain `unresolved; private testing only; no public CI/redistribution/adoption claim`. Human storage direction is not Siemens clearance and closes no representative-corpus or independent-conformance row.
- Post-receipt formatting, 49 default tests, strict all-target/all-feature Clippy, `git diff --check`, exact staged-file, PRIVATE visibility/default/origin, remote blob/size, and local/tracking/remote/API SHA checks passed; the private worktree is clean.

## Changed private files

- `/Users/mattiafiumara/repos/ratemypcb-odbpp-private/src/lib.rs`
- `/Users/mattiafiumara/repos/ratemypcb-odbpp-private/src/geometry.rs`
- `/Users/mattiafiumara/repos/ratemypcb-odbpp-private/src/main.rs`
- `/Users/mattiafiumara/repos/ratemypcb-odbpp-private/tests/synthetic_evidence.rs`
- `/Users/mattiafiumara/repos/ratemypcb-odbpp-private/README.md`

## Residual boundary

Still unsupported or unproven: archives/`.Z` decompression; concurrently attacker-mutated trees; arc-bearing topology proof; user-defined/complex symbol expansion; text/barcode and feature-attribute semantics; resized-pad expansion; non-circular arbitrary-rotation bounds; step-repeat; connectivity/components/pins/netlists; drills/routes/tools; stackup/construction/XML; permission-manifested representative corpora; independent conformance; representative production timing/RSS/allocation/cancellation latency; audited production integration; maintenance ownership; and every legal/distribution/publication/release/support claim.

Plan 06-05 remains accepted private technical evidence only. A later explicit human direction used Plan 06-06 for secure archive/official-sample evidence; the decision-specific product plan moved to the reserved number 06-08 and is not created. Plan 06-06 does not alter Plan 06-05's fixed-point/control conclusions or complete FMT-03.

## Self-Check

All scoped implementation, focused review-remediation, private/public, diagnostic, dependency, synthetic-evidence, planning-structure, diff, staged-file, private commit/push, and remote-verification checks passed within the stated boundary.
