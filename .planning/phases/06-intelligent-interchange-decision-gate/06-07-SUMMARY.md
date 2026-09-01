---
phase: 06-intelligent-interchange-decision-gate
plan: 07
subsystem: private-odbpp-precision-degradation
status: complete
tags: [rust, odbpp, exact-precision, partial-evidence, private-corpus]
requires:
  - phase: 06-intelligent-interchange-decision-gate
    plan: 06
    provides: Secure bounded official archive ingestion reaching exact semantic parsing.
provides:
  - Exact source-spanned typed precision omissions with no rounding or canonical-model change.
  - Safe independent-record, whole-contour, and whole-surface degradation with malformed records still fatal.
  - Deterministic partial adapter evidence from the committed official sample.
  - PRIVATE main checkpoint a4216f6909754155555e9290c2ec84e0eb16d267.
affects: [FMT-03-evidence, future-FMT-04-candidate]
requirements-completed: []
completed: 2026-08-30
duration: 50m
---

# Phase 6 Plan 07: Exact Precision Degradation

## Outcome

A coordinate finer than one picometre no longer aborts the complete package. The parser keeps the canonical integer-picometre model unchanged and never rounds. It records the exact raw token, unit, typed `FinerThanPicometre` reason, virtual path, physical line, token byte span, and source lexeme. An independent L/P/A record is omitted; an affected OB..OE contour is consumed and omitted; an affected island drops its whole surface. No partial malformed contour reaches topology or mapping.

Empty feature files become explicit missing-record evidence. A feature file above the existing 4 MiB semantic budget becomes a resource omission without being read. Unknown compressed/resource-limited files downgrade every geometry capability they could contain. Custom symbols beginning with `r` or `s` are no longer misclassified as standard round/square apertures, and zero-sized standard apertures remain explicit unsupported expansion rather than aborting independent evidence.

## Changed private files

- `src/geometry.rs`
- `tests/archive_evidence.rs`
- `README.md`

No dependency, corpus asset, public product source, Phase 7 file, or canonical geometry type changed.

## Focused regressions

Project-authored tests cover:

- metric layer and imperial profile sub-picometre tokens;
- mixed valid/invalid records;
- unsafe whole-island/surface drop and safe neighboring-contour retention;
- exact raw token/unit/reason/path/line/byte span;
- deterministic omission IDs/order and repeated adapter equality;
- no false Complete for units, points, primitive geometry, regions, profile, extents, transforms, polarity, or unknown whole-file contents;
- early precision plus malformed trailing polarity, missing symbol, and malformed later contour coordinates remaining fatal;
- deadline and cancellation returning typed interruption with no partial result;
- exact official adapter/capability/omission/conflict counts.

## Fresh review and one remediation pass

Fresh read-only reviewer run `2127bba1-d94b-4d8e-acc3-e76ce9571164` returned BLOCK with three P1 findings and one P2 assertion gap. One remediation pass closed them:

1. Reordered independent-record validation, checked omitted-record symbol references, and continued syntax/coordinate validation through already-dropped contours.
2. Added `partial_transforms` so a precision-omitted pad cannot leave Transforms Complete.
3. Separated known-empty files from unknown compressed/resource-limited files and downgraded all capabilities unknown files could contain.
4. Asserted the official exact Complete capability set and that every omission-named capability is non-Complete.

No second broad review or zero-findings loop was run.

## Official private evidence

Release-mode exact result:

- Archive: 1,372 raw entries; 1,190 logical entries; 182 GNU long-name metadata entries; 839 files; 351 directories.
- Adapter facts: 41 documents; 44 layers; 482 apertures; 47 features; 8 physical bounds; 0 proven region topologies; 0 profiles.
- Capabilities: 36 total — 2 Complete (`LayerRoles`, `LayerOrder`), 13 Partial, 21 NotProvided, 0 Unsupported/Failed/Omitted.
- Omissions: 150,407 total — 2 MissingRecord, 278 UnsupportedRecord, 150,115 UnsupportedPrecision, 1 ResourceLimit, 11 UnvalidatedSemantic.
- Conflicts: 0.
- First precision evidence: `steps/CELLULAR_FLIP-PHONE/profile:12`, token `-0.494322244094`, inch, bytes 77..92, `FinerThanPicometre`; the unsafe outer contour/surface is absent.
- Next first unsupported construct: `steps/CELLULAR_FLIP-PHONE/layers/ASSEMT/features`, 33,606,137 bytes versus the retained 4,194,304-byte geometry-file budget; represented as one resource omission, not parsed or raised to support.
- Operations `(archive_read, decompression, materialization, discovery, read, matrix, profile, geometry, topology, mapping)`: `(360, 9414, 28439, 781, 972, 1404, 1016, 475788, 94, 151040)`.
- Release observation: 368,053 µs archive-to-adapter inside the test; 0.67 s real / 0.23 s user / 0.18 s sys; 226,197,504-byte maximum resident-set report and 12,927,552-byte peak memory-footprint report.

This remains one private producer-unstated, rights-unresolved sample. It proves neither conformance, representation, adoption, production performance, distribution authority, publication authority, nor product support.

## Gates

- `cargo fmt --all -- --check`: pass.
- `cargo test --locked --all-targets`: 61 passed, 0 failed, 1 ignored non-representative scaling case.
- Strict `cargo clippy --locked --all-targets --all-features -- -D warnings`: pass.
- Primary Rust LSP on changed Rust files: zero diagnostics.
- Focused full lens: no production finding; its remaining `rust-unwrap` signals are inside `#[cfg(test)]` assertion/setup code where panic intentionally fails the test.
- Dependency/source/license/duplicate tree: unchanged lock, registry-only, permissive license metadata present, no duplicate versions.
- `cargo audit --deny warnings`: 1,226 RustSec advisories loaded; zero vulnerabilities/warnings.
- Public locked/offline unsupported-format regression: 1 passed, 0 failed, 6 filtered.
- GSD Plan 06-07 structure: valid, 3 tasks, zero warnings; references valid; consistency pass. Health retains unrelated pre-existing workspace/config warnings.
- Private/public `git diff --check`, exact private file set, zero public staged files, absent 06-08 decision plan, and no public product/Phase-7 mutation: pass.

## Private commit receipt

- Repository: <https://github.com/ratemypcb/ratemypcb-odbpp> (`PRIVATE`)
- Branch/default: `main`
- Commit: `a4216f6909754155555e9290c2ec84e0eb16d267`
- Subject: `fix: preserve evidence on unsupported geometry precision`
- Push: ordinary fast-forward from `1d7d859e74d995f86eb6b4110ef86673cd4f1938`.
- Local HEAD, `origin/main`, `ls-remote`, and GitHub API head equality: verified.
- Private worktree: clean.
- No force, tag, release, visibility change, external contact, corpus copy, or public commit.

## Remaining blocker and ownership

FMT-03, FMT-04, and FMT-05 remain pending. The next decision-product plan is reserved as 06-08 and was not created. The remaining blocker is human/legal/conformance/adoption authority plus the next resource-limited official layer and broader unsupported ODB++ semantics; none is authorized by this parser checkpoint.

- Phase workspace: `wks_fd3443e0c6f9a467`.
- Current feature owner/Paseo agent: `235d10a7-8a97-4c66-bb3a-c2c708143514`.
- Current Pi session: `01a05435-ea83-7b7f-9158-54c0ee4256bb`.
- Archived prior owner: `359e53f3-89da-4661-924c-043f2810e318`.
- Fresh review owner: `2127bba1-d94b-4d8e-acc3-e76ce9571164`.
