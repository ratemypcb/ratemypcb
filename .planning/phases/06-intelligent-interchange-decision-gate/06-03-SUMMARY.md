---
phase: 06-intelligent-interchange-decision-gate
plan: 03
subsystem: private-odbpp-geometry-proof
tags: [rust, odbpp, fixed-point, arc-extents, computational-geometry, provenance]
requires:
  - phase: 06-intelligent-interchange-decision-gate
    plan: 02
    provides: Bounded matrix/profile/basic-geometry parser and policy-free evidence mapping.
provides:
  - Exact checked directed-arc and supported-primitive physical envelopes.
  - Bounded exact winding, intersection, containment, and hole non-overlap proof for line-only profiles.
  - Typed partial/resource omissions for unsupported extent expansion and unproved topology.
  - One fresh correctness review with one focused remediation pass.
affects: [FMT-03-evidence, future-FMT-04-candidate]
requirements-completed: []
completed: 2026-08-30
---

# Phase 6 Plan 03: Arc Extents and Profile Topology Checkpoint

## Outcome

The quarantined stdlib-only private parser now derives exact checked physical envelopes for its explicitly supported primitive subset and proves topology for bounded line-only profiles. Arc-bearing, workload-limited, general-layer, or expansion-dependent cases remain typed Partial/Unsupported rather than gaining false Complete evidence.

This checkpoint does not establish ODB++ adoption, representative conformance, public support, rights, distribution, publication, integration, release eligibility, or approval effects. FMT-03, FMT-04, and FMT-05 remain pending, and IPC-2581's symmetric gaps remain unchanged.

## Exact extent boundary

- Directed arcs use exact integer-picometre radii and polar ordering; only endpoints and cardinal extrema on the clockwise/counter-clockwise sweep contribute.
- Quarter, half, clockwise/counter-clockwise wraparound, and non-endpoint cardinal cases have exact synthetic expected bounds.
- Line/arc strokes require a supported symmetric circle/square aperture and an exactly representable half-width.
- Circle flashes support any retained rotation; square, rectangle, and obround flashes support 0°/90°/180°/270° bounds. Mirror does not change the envelope.
- Surface/profile line and arc segments contribute exact primitive bounds.
- Every coordinate add/subtract/multiply, squared radius, and envelope-domain check is checked. Non-integral radius, odd-picometre half-size, ambiguous full circle, arithmetic overflow, or bounds outside `MAX_COORDINATE_PM` fail closed.
- Unsupported/custom apertures, resized pads, non-circular arbitrary rotations, and unsupported feature records yield no under-bound document fact. They produce provenance-bearing `UnsupportedRecord` omissions and keep `Extents` Partial.
- Proven document bounds and profile extents are sorted deterministic private facts; repeated parses/results and equivalent feature/hole reorderings preserve extent and capability state.

## Exact topology boundary

- Profiles still require one outer island followed by holes.
- For line-only profiles with at most 4,096 total segments, exact integer predicates prove:
  - simple contours without non-adjacent crossing or touching;
  - nonzero authoritative winding;
  - clockwise outer island and counter-clockwise holes;
  - every hole strictly inside the outer island;
  - no hole/outer or hole/hole intersection or touching;
  - no overlapping or nested holes.
- Concave outer contours and ray crossings through vertices are covered without floating point or division.
- Bow-ties, non-adjacent touches, wrong winding, zero-area ambiguity, outside/touching/intersecting/nested holes fail at source provenance.
- Arc-bearing profiles remain parsed with `UnvalidatedSemantic` omissions and Partial `Profile`/`GeometryRegions` capabilities. Profiles above the 4,096-segment topology-work ceiling remain parsed with `ResourceLimit` omissions and the same Partial states.
- General layer-surface topology remains outside this profile-only proof and keeps `GeometryRegions` Partial.

## Fresh correctness review and remediation

Review artifact: `/tmp/ratemypcb-odbpp-private-phase6-plan03-review.md`.

The single fresh-context review found no P0/P1 issue, no false Complete state, no under-bound, and no reachable panic for parser-produced values. Verdict: **OK with notes**.

| Review note | One remediation pass |
| --- | --- |
| P2: supported aperture/rotation branches lacked focused coverage | Added table-driven rectangle/obround 0°/90°/180°/270°, arbitrary-rotation circle, non-circular 45° omission, and resized-pad omission checks. |
| P2: topology predicates lacked wrong-winding, zero-area, concave, and vertex-ray cases | Added focused wrong outer/hole winding, simple zero-area, valid concave containment, concave-notch rejection, and ray-through-vertex regressions. |
| Residual symmetry note for clockwise arc cases | Added clockwise quarter and wraparound exact-bound cases. |

During that same bounded remediation pass, the general crossing branch was tightened to require strictly opposite nonzero orientation signs after collinear on-segment handling. No second zero-findings review was requested.

## Verification

- Private `cargo fmt --check`: pass.
- Private full `cargo test`: **33 passed, 0 failed**; binary/doc targets also pass with no failures.
- Private `cargo clippy --all-targets -- -D warnings`: pass.
- Focused primary LSP diagnostics for `src/lib.rs`, `src/geometry.rs`, and `src/main.rs`: **0 diagnostics**.
- Locked/offline public unsupported-format regression: **1 passed, 0 failed, 6 filtered**.
- `cargo tree --offline --depth 1` lists only `ratemypcb-odbpp`; `publish = false`; private `.git` remains absent.
- Tests use project-authored synthetic strings/directories only.

## Changed private files

- `/Users/mattiafiumara/repos/ratemypcb-odbpp-private/src/geometry.rs`
- `/Users/mattiafiumara/repos/ratemypcb-odbpp-private/README.md`

No Git initialization, staging, commit, remote, fetch, push, merge, reset, clean, stash, publication, release, public integration, or Phase 7 action occurred.

## Residual boundary and next slice

Still unsupported: archives/`.Z` decompression; concurrently attacker-mutated trees; user-defined/complex symbol expansion; text/barcode and feature-attribute semantics; resized-pad expansion; non-circular arbitrary-rotation bounds; general-layer and arc-bearing topology proof; step-repeat; connectivity/components/pins/netlists; drills/routes/tools; stackup/construction/XML; representative corpus conformance; public integration/support; and all legal/release claims.

The next smallest private parser slice is bounded line-only topology for general layer surfaces, reusing the exact segment predicates while keeping multi-island/hole association and every arc-bearing surface explicitly partial until separately proven.
