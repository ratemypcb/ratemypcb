---
phase: 04-kicad-schematic-release-evidence
plan: 01
status: complete
completed: 2026-08-27
requirements-completed: [SCH-03, SCH-04, SCH-07]
commits: 0
---

# Phase 4 Plan 01 Summary

Implemented the released-major native KiCad seam and migrated existing PCB/profile DRC to it. No 04-02 root selection, hierarchy reconciliation, source parsing, CLI `--schematic`, viewer, or documentation work was added.

## Production behavior

- `KiCadMajor` dispatch accepts exactly 8, 9, and 10; 7, 11, future, and unparseable versions fail closed.
- Fixed shell-free ERC/DRC argv omit `--refill-zones` and `--save-board`; parity is an explicit DRC option and ordinary DRC does not enable it.
- Exit 0 and 5 are completed analyses. All other exits, unavailable/spawn failures, timeout, missing/oversized/malformed/truncated/wrong-kind reports, marker overflow, and executable/report-major mismatch fail closed.
- Auto failures serialize as provenance-bearing `not_run`; required failures remain `Error::Native` for the existing CLI exit-3 mapping.
- Ordinary native DRC runs from a bounded intact project copy, while the process runner uses a separate fresh private output directory, bounded stdout/stderr/report files, a 120-second kill+wait timeout, and cleanup on every return path. A live required-native review confirmed no `.kicad_prl` or other sidecar was created in the source fixture.
- Normalization retains ERC occurrence paths and distinct ordinary DRC, unconnected, schematic-parity, and ERC channels. Exclusions are `Option<bool>`; excluded and unknown-exclusion markers remain serialized.
- Active generic DRC counts/findings include only explicit `excluded: false` markers and never parity. Unknown exclusions make coverage attention rather than pass.
- Native evidence uses the observed executable version, retains the report version, exact board SHA-256, and structural marker location. Profile DRC uses the same runner and preserves normalized delta markers.
- `report-2.0.json` now bounds and describes only the native report/marker slice owned by this plan.

## Fixture evidence

Created `tests/fixtures/kicad/supported/{8,9,10}/hierarchical/` with the required project, child sheet reused twice, PCB, BOM, placement, netlist, ERC, DRC, version, and digest manifest files. Synthetic BOM/netlist exports explicitly represent multi-unit, power-flag, fitted, and DNP states.

- KiCad 8 (`8.0.9`): documentation/schema-attested, `locallyExecuted: false`.
- KiCad 9 (`9.0.6`): documentation/schema-attested, `locallyExecuted: false`; exclusion limitation remains explicit.
- KiCad 10 (`10.0.5`): locally executed on the intact synthetic fixture tree. ERC exited 0 with three sheet occurrence records; DRC exited 5 with six markers.
- Every manifest SHA-256 entry is checked by a unit test. The corpus contains only project-authored synthetic/generic data.

Created `tests/fixtures/kicad/native-failures/` for supported/unsupported/unparseable versions, known and omitted KiCad 9 exclusion state, malformed JSON, truncated JSON, and the bounded runner failure contract.

## Verification evidence

- `cargo test -p ratemypcb-core --locked schematic::tests::native_` — pass, 10 tests after final additions through the broader `native_` filter.
- `cargo test -p ratemypcb-core --locked native_` — pass, 13 tests, 0 failed.
- `cargo test -p ratemypcb-core --locked schema_is_versioned` — pass, 1 test.
- `cargo test -p ratemypcb-core --locked decision_contract_generated_schemas_match_checked_in_json` — pass, 1 test.
- `cargo test --all --locked` — pass: CLI 11, CLI integration 2, core 63, doc tests 0; no failures.
- `cargo fmt --all -- --check` — pass, no output.
- `git diff --check` — pass, no output.
- Local fixture commands:
  - `kicad-cli sch erc --format json --severity-all --exit-code-violations --output /tmp/ratemypcb-04-01-erc.json root.kicad_sch` — exit 0, KiCad 10.0.5, 3 sheets.
  - `kicad-cli pcb drc --format json --severity-all --exit-code-violations --output /tmp/ratemypcb-04-01-drc.json root.kicad_pcb` — exit 5, KiCad 10.0.5, 6 markers.
  - CLI required-native review of the KiCad 10 board — exit 0 despite native exit 5; report status `completed`, executable/report version `10.0.5`, six retained unknown-exclusion markers, no source-tree `.kicad_prl`, and native evidence digest `f458f4c09141c0e2572aad3f5682e34b020a05495559c580e8fb9f6f76b4134a`, matching the board SHA-256.
- One attempted Cargo invocation supplied two test filters and failed with Cargo's `unexpected argument` usage error; each filter was then run separately and passed.
- `git diff --cached --name-only` — empty; no staged files.

## Residual risks

- KiCad 8/9 outputs remain documentation/schema-attested rather than locally executed; manifests and tests prevent a false execution claim.
- Producer omission of `excluded` remains unknown, including locally captured KiCad 10 output; these markers are retained and cannot silently become active findings or a pass.
- Final schematic aggregates, coherent board/schematic pair digest wiring, root selection, and reconciliation remain intentionally owned by 04-02/04-03.
