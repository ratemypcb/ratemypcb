---
phase: 04-kicad-schematic-release-evidence
plan: 02
status: complete
completed: 2026-08-27
requirements-completed: [SCH-01, SCH-02, SCH-04, SCH-05, SCH-06, SCH-07, SCH-08]
commits: 0
---

# Phase 4 Plan 02 Summary

Implemented bounded schematic inventory, occurrence identity, explicit fact provenance, native/export authority, deterministic reconciliation, and conservative non-KiCad capability states. No CLI `--schematic`, viewer, doctor/help, skill prose, heuristic ERC recreation, or 04-03 work was added.

## Production behavior

- Directory, standalone, and ZIP inventory recognizes KiCad schematic/project/private settings/library context, `.SchDoc`, and bounded s-expression/XML netlist candidates. Standalone schematic roots load bounded relative children but cannot enable parity; ZIP sources are never staged for native execution.
- A bounded UTF-8 s-expression parser enforces file/aggregate bytes, token/string/nesting, child, occurrence, component, net, and record limits. Root selection prefers coherent board/project basenames, removes child-only candidates, honors a selected standalone root, and makes remaining ambiguity explicit.
- Relative child paths are project-confined. Explicit project text variables may resolve child paths only in one selected project context. Missing child, unresolved variable, external path, broken/duplicate instance path, recursion/cycle, unsupported syntax, and limits remain typed capability states.
- Occurrences use project identity + root digest + sheet UUID path + item UUID. Reused child symbols retain distinct keys and sheet paths even when source path/item UUID/reference are identical.
- Source facts include only fields explicitly present: occurrence/item identity, reference/unit, value/footprint, `in_bom`, `on_board`, DNP, explicit pins/nets/electrical type, library ID, and explicit power-symbol/flag state. Every fact carries producer, evidence class, source path, and confidence.
- Fixed shell-free KiCad 8/9/10 native adapters now include BOM, `kicadsexpr` netlist, and position exports with fresh bounded outputs. Completed native exports override packaged exports only for fields they emit; unavailable auto exports become typed `not_run` capability states and required failures remain errors.
- Reconciliation joins occurrence UUID path + item UUID first, board UUID/path second, then unique references only as an explicit low-confidence fallback. It checks reference/UUID, value, footprint, DNP/fitted/BOM/board state, explicit pin-pad/net mapping, grouped BOM quantity/population, placement population/value, and declared BOM/placement revision.
- The report carries root/board/artifact digests, declared revisions, occurrences, fact-specific capabilities, mismatches, native ERC/parity reports, and limitations. Mismatch locations retain sheet/item/source identity.
- `gateImpact` is report-authoritative and defaults legacy/existing findings to `blocking`; every schematic reconciliation mismatch is `evidence_only`. Approval ignores only explicitly evidence-only findings, and no schematic family was added to `requiredEvidence`.
- `.SchDoc` remains inventory-only. Recognized generic netlists expose only explicit component/net/pin export fields; unknown syntax is unsupported. Neither state creates native ERC, hierarchy, parity, DNP, revision, or Altium-native claims.

## Fixture and test evidence

- Added adversarial hierarchy fixtures for missing/resolved/unresolved children, ambiguity, broken/duplicate instance paths, reused children, and cycles.
- Added a clean coherent reconciliation control plus deterministic mutations for reference/UUID, value, footprint, fitted/DNP, pin-pad, net, grouped quantity, placement, and revision.
- Added bounded `.SchDoc`, recognized s-expression/XML netlist, unknown netlist, and generated ZIP inventory tests.
- Added `crates/ratemypcb-core/tests/schematic_release.rs` with 10 focused integration tests and native export command/runner unit coverage.

## Verification evidence

- `cargo test -p ratemypcb-core --test schematic_release --locked hierarchy_` — pass.
- `cargo test -p ratemypcb-core --test schematic_release --locked reconciliation_` — pass.
- `cargo test -p ratemypcb-core --test schematic_release --locked bounded_eda_` — pass.
- `cargo test -p ratemypcb-core --locked schematic::tests::native_` — pass, including 04-01 native regressions plus bounded export vectors/runner.
- `cargo test -p ratemypcb-core --locked native_` — pass.
- `cargo test -p ratemypcb-core --locked schema_is_versioned` — pass.
- `cargo test -p ratemypcb-core --locked decision_contract_generated_schemas_match_checked_in_json` — pass.
- `cargo test --all --locked` — pass: CLI unit 11, CLI integration 2, core unit 66, schematic release integration 10, doc tests 0; no failures.
- `cargo fmt --all -- --check` — pass, no output.
- `git diff --check` — pass, no output.
- `git diff --cached --name-only` — empty.

## Residual risks

- KiCad 8/9 native exports remain command-contract/documentation attested rather than locally executed. The new export runner was exercised with a bounded mock executable; this plan does not claim a new live KiCad capture.
- KiCad source connectivity is intentionally limited to explicit fields represented by the bounded parser/export. No wire-graph interpretation, ERC rule recreation, or heuristic power analysis is performed.
- Altium remains inventory-only and generic XML support intentionally recognizes a narrow export root/field subset.
- Schematic families remain evidence-only pending the independent promotion/adjudication gate.
