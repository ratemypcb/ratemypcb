---
phase: 06-intelligent-interchange-decision-gate
plan: 02
subsystem: private-odbpp-parser
tags: [rust, odbpp, fixed-point, geometry, provenance, bounded-parsing]
requires:
  - phase: 05-manufacturing-evidence-model-and-gerber-baseline
    provides: Canonical capability, provenance, omission, conflict, fixed-point, and fail-closed contracts.
provides:
  - Private bounded stable-unpacked-tree ODB++ matrix/profile/basic-geometry parser.
  - Exact MM/INCH picometre conversion and typed L/P/A/S evidence.
  - Deterministic policy-free AdapterResult-shaped facts, capabilities, omissions, and conflicts.
  - Focused closure of the independent review's resource, symlink, identity, order, and evidence-honesty findings.
affects: [FMT-03-evidence, future-FMT-04-candidate]
requirements-completed: []
completed: 2026-08-30
---

# Phase 6 Plan 02: Private ODB++ Parser Checkpoint

## Outcome

The human-authorized private crate at `/Users/mattiafiumara/repos/ratemypcb-odbpp-private` now has a tested, stdlib-only parser core for caller-owned stable unpacked trees. It does not integrate with RateMyPCB, expose approval policy, establish format adoption, or resolve implementation/distribution/publication rights.

FMT-03, FMT-04, and FMT-05 remain pending. IPC-2581 remains unsupported/not checked with its symmetric evidence gaps unchanged.

## Exact capabilities

- One-root/wrapper discovery and typed matrix `STEP`/`LAYER` parsing.
- Deterministic matrix ordering; duplicate column/row/name/ID and dangling layer-reference rejection.
- Canonical-order range rejection before any `u32` matrix row could be dropped during `i32` mapping.
- Plain-ASCII profile/layer feature parsing with `U MM|INCH` and `UNITS=MM|INCH`.
- Exact checked picometre conversion; finer-than-picometre values fail rather than round.
- Typed symbol/attribute lookup retention and L line, P pad, A arc, and S surface parsing.
- Typed polarity, pad orientation/resize, arc direction/center/radius, contour islands/holes, and per-record source path/line/byte provenance.
- Profile contour and cutout feature projection with authoritative profile polarity.
- Private deterministic `AdapterResult`-shaped documents, layers, apertures, features, profiles, capability ledger, omissions, and conflicts; no approval field.
- File/aggregate bytes, physical-line count/bytes, records, files, steps/layers, symbols, attributes, virtual paths, features, and contour vertices are bounded.
- Static final and ancestor symlinks are rejected for both parsed files and unsupported `.Z` evidence paths.

## Independent review remediation

| Finding | Root-cause fix | Focused regression |
| --- | --- | --- |
| Physical-line/record amplification | Check physical bytes before trimming; cap every physical line including blank/comments; enforce surface record budget. | `bounds_physical_lines_before_trimming_and_counts_ignored_lines` |
| Ancestor-symlinked `.Z` evidence | Reuse one canonical-containment/full-component path validator for reads and compressed evidence. | `rejects_ancestor_symlinks_for_compressed_evidence` |
| Profile layer ID collision | Give synthetic profile layers the separate `odbpp-profile-layer:` namespace. | `profile_layer_identity_cannot_collide_with_a_legal_matrix_name` |
| Dropped oversized layer order | Reject matrix rows outside the canonical `i32` order range. | `rejects_layer_rows_outside_the_canonical_order_range` |
| Empty/unrelated matrix capability evidence | Attach matrix step/layer provenance to `DocumentSyntax`, `LayerRoles`, and `LayerOrder`. | `matrix_only_capabilities_retain_matrix_provenance` |
| Compressed file mislabeled missing | Map existing `.Z` files to `UnsupportedRecord`. | `reports_compressed_geometry_without_reading_it` |
| Hardcoded profile polarity / missing cutouts | Derive profile-layer polarity and project hole polygons as deterministic cutout features. | `profile_polarity_and_hole_cutouts_are_projected_from_source` |
| Generic roles over-narrowed | Map generic `MASK`/`DOCUMENT` to `Other`. | `generic_matrix_types_do_not_gain_narrower_roles` |
| Public virtual-path amplification | Require normalized relative ASCII paths within 256 bytes before provenance retention. | `rejects_unbounded_or_non_normalized_public_virtual_paths` |
| Matrix-specific shared diagnostic | Use path-neutral package-file identity wording. | Covered by shared path tests and source inspection. |

No repeated review/zero-findings ceremony was created; focused regressions plus full gates are the closure evidence.

## Verification

- Private `cargo fmt --check`: pass.
- Private full `cargo test`: **23 passed, 0 failed**; binary/doc tests also pass with no test failures.
- Private `cargo clippy --all-targets -- -D warnings`: pass.
- Focused primary LSP diagnostics for `src/lib.rs`, `src/geometry.rs`, and `src/main.rs`: **0 diagnostics**.
- Locked/offline public unsupported-format regression: **1 passed, 0 failed, 6 filtered**.
- No third-party dependency was added; `Cargo.toml` remains `publish = false`.

## Changed private files

- `/Users/mattiafiumara/repos/ratemypcb-odbpp-private/src/lib.rs`
- `/Users/mattiafiumara/repos/ratemypcb-odbpp-private/src/geometry.rs`
- `/Users/mattiafiumara/repos/ratemypcb-odbpp-private/src/main.rs`
- `/Users/mattiafiumara/repos/ratemypcb-odbpp-private/README.md`

No Git repository, staging, commit, remote, fetch/push, merge, publication, or release action was performed.

## Unsupported boundary and next slice

Still unsupported: archives and `.Z` decompression; concurrently attacker-mutated trees; user-defined/complex symbol expansion; text/barcode and feature-attribute semantics; resized-pad expansion; full winding/self-intersection/hole-containment proof; arc-aware extents; step-repeat; connectivity/components/pins; netlists; drills/routes/tools; stackup/construction; XML; representative corpus conformance; and RateMyPCB integration/support.

The next smallest private technical slice is arc-aware extents plus full profile topology validation (winding, self-intersection, and hole containment). It can reduce current `Profile`/`GeometryRegions` omissions without broadening into archives, public integration, or policy.
