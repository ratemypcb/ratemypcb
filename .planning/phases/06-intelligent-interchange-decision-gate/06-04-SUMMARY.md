---
phase: 06-intelligent-interchange-decision-gate
plan: 04
subsystem: private-odbpp-region-topology
status: complete
tags: [rust, odbpp, fixed-point, computational-geometry, topology, provenance]
requires:
  - phase: 06-intelligent-interchange-decision-gate
    plan: 03
    provides: Exact checked surface predicates, arc-aware extents, and bounded line-profile topology.
provides:
  - Shared bounded exact topology proof for line-only profiles and general layer surfaces.
  - Deterministic provenance-preserving multi-island and hole associations.
  - Fail-closed capability and omission mapping for unsupported, compressed, arc-bearing, work-limited, and malformed typed evidence.
  - One fresh correctness review with one focused remediation pass closing two P1 and two P2 findings.
affects: [FMT-03-evidence, future-FMT-04-candidate]
requirements-completed: []
completed: 2026-08-30
---

# Phase 6 Plan 04: General Line-Surface Topology Checkpoint

## Outcome

The quarantined stdlib-only private parser now reuses Plan 06-03's exact integer predicate engine to prove bounded line-only topology for both profiles and general layer surfaces. Proven general surfaces can contain multiple disjoint islands and multiple uniquely associated holes. Canonical region-topology facts are emitted only for nonempty, fully proven island associations and retain source provenance.

Arc-bearing, workload-limited, compressed, unsupported-record, malformed typed, or otherwise unproved geometry stays typed Partial/Unsupported/ResourceLimit. This checkpoint does not establish ODB++ adoption, conformance, rights, public support, distribution, publication, integration, release eligibility, or approval effects. FMT-03, FMT-04, and FMT-05 remain pending, and IPC-2581's symmetric gaps are unchanged.

## Exact Complete boundary

A parsed line-only surface is topology-complete only when all of the following hold:

- every contour is explicitly closed, has at least three segments, nonzero exact winding, and no non-adjacent crossing or touching;
- islands are clockwise and holes are counter-clockwise;
- islands are pairwise disjoint, non-touching, and non-nested;
- every hole is strictly inside exactly one island;
- holes do not intersect/touch an island and are pairwise disjoint, non-touching, non-overlapping, and non-nested;
- the package-carried checked `MAX_TOPOLOGY_WORK` budget covers the proof; and
- at least one proven island association exists.

Associations are sorted by exact geometry and then provenance, so equivalent contour orderings and repeated parses produce identical typed results. Each association retains island and hole start points plus source line/byte provenance.

The same `TopologySegment`, `validate_simple_contour`, `contour_orientation`, `segments_intersect`, and `point_location` implementation serves profiles and general surfaces. No second topology engine, floating-point predicate, geometry dependency, or approximate fallback was introduced.

## Exact Partial/Unsupported boundary

- Any arc-bearing surface retains parsed geometry but receives `UnvalidatedSemantic`; it emits no region-topology fact.
- A surface batch that exceeds the remaining package-carried proof budget receives `ResourceLimit`; the budget is not reset per profile or layer file.
- Unsupported `T`/`B` records make aggregate `GeometryRegions` Partial even when the proven-region count is zero.
- Retained `features.Z`/compressed geometry makes aggregate `GeometryRegions` Partial and its omission explicitly affects that capability; a compressed profile also keeps `Profile` Partial.
- A public typed surface with no proven island association is not complete and emits an `UnvalidatedSemantic` omission rather than a topology fact.
- An absent public typed `profile_topology` value keeps `Profile` Partial and no longer reaches a production `expect`.
- Neighboring complete surfaces may retain their individual topology facts, but any unproved region evidence keeps the aggregate capability Partial.

## Fresh correctness review and one remediation pass

Review artifact: `/tmp/ratemypcb-odbpp-private-phase6-plan04-review.md`.

The single fresh-context review found no P0, two valid P1 capability-honesty defects, and two valid P2 fail-closed/coverage gaps. Its initial verdict was **BLOCK**. All four findings were closed in one focused remediation pass; no second review or zero-findings ceremony was requested.

| Finding | Fix | Focused regression |
| --- | --- | --- |
| P1: unsupported-only regions became `NotProvided` because zero count was checked before partial state | `set_count_capability` now gives explicit partial evidence precedence over a zero proven count | `unsupported_only_geometry_is_partial_not_not_provided` |
| P1: compressed layer geometry could leave aggregate regions Complete | compressed geometry now sets `partial_regions` and includes `GeometryRegions` in the typed omission; compressed profiles also affect `Profile` | `compressed_geometry_keeps_aggregate_regions_partial` |
| P2: carried-budget coverage stopped within one feature file | added a canonical profile-then-layer package where each surface fits alone but the later layer exceeds only the remaining package budget; repeated parsing is equal | `carries_topology_work_from_profile_into_later_layer_file` |
| P2: public typed values could omit profile topology or use empty default surface topology | removed the profile `expect`; `SurfaceTopology::complete` now requires a nonempty association and mapping synthesizes a typed omission for empty proof state | `malformed_public_topology_values_fail_closed_without_panicking` |

Focused lens also identified the matrix parser's structurally safe active-record `expect`; it was replaced with an explicit typed fail-closed branch. Remaining production `expect`s guard parser-validated internal invariants outside Plan 06-04's public topology mapping; its `rust-unwrap` blockers are confined to `#[cfg(test)]` setup/assertion paths where panic intentionally fails the test. The test findings were inspected and marked false-positive rather than changing production behavior or adding source suppressions.

## Gate results

- Private `cargo fmt --check`: pass.
- Private full `cargo test`: **43 passed, 0 failed**; binary/doc targets also pass.
- Private `cargo clippy --all-targets -- -D warnings`: pass.
- Focused primary LSP diagnostics for the private lib.rs, geometry.rs, and main.rs: **0 diagnostics**.
- Focused lens: no unresolved Plan 06-04 production finding; the topology-specific public-value panic was removed, internal parser-invariant checks were bounded, and test-only unwrap diagnostics were dispositioned as intentional test failures.
- Locked/offline public unsupported-format regression: **1 passed, 0 failed, 6 filtered**.
- `cargo tree --offline --depth 1` lists only `ratemypcb-odbpp`; `publish = false`; private `.git` remains absent.
- GSD state/roadmap and all four Phase 6 plan structures validate.
- `git diff --check`, zero-staged, zero-public-product-change, and zero-Phase-7-change checks pass.
- Tests use only project-authored synthetic strings/directories; no Siemens or ambiguous corpus asset was added.

## Changed private files

- `/Users/mattiafiumara/repos/ratemypcb-odbpp-private/src/lib.rs`
- `/Users/mattiafiumara/repos/ratemypcb-odbpp-private/src/geometry.rs`
- `/Users/mattiafiumara/repos/ratemypcb-odbpp-private/README.md`

No Git initialization, staging, commit, remote, fetch, push, merge, reset, clean, stash, publication, release, public integration, or Phase 7 action occurred.

## Residual boundary

Still unsupported: archives/`.Z` decompression; concurrently attacker-mutated trees; arc-bearing topology proof; user-defined/complex symbol expansion; text/barcode and feature-attribute semantics; resized-pad expansion; non-circular arbitrary-rotation bounds; step-repeat; connectivity/components/pins/netlists; drills/routes/tools; stackup/construction/XML; representative corpus conformance; public integration/support; and every legal/distribution/publication/release claim.

No Plan 06-05 or further parser slice was created. The next Phase 6 action remains separate evidence collection and the later explicit FMT-03 adopt-ODB/adopt-IPC-2581/adopt-both/no-go checkpoint when its applicable gates are ready.

## Self-Check

All Plan 06-04 implementation, review-remediation, private/public, diagnostic, dependency, planning, diff, and staged-file checks passed.
