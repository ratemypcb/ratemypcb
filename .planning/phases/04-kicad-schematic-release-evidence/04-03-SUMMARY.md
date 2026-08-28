---
phase: 04-kicad-schematic-release-evidence
plan: 03
status: complete
completed: 2026-08-27
requirements-completed: [SCH-01, SCH-02, SCH-03, SCH-04, SCH-05, SCH-06, SCH-07, SCH-08]
commits: 0
---

# Phase 4 Plan 03 Summary

Integrated the Phase 4 schematic slice through the CLI selector, structured doctor output, report/schema source-pair contract, existing viewer/evidence surfaces, skill/docs, and adversarial/full-suite gates. No Phase 5 work or schematic-family promotion was added.

## Production behavior

- Added optional `ReviewOptions.schematic` and CLI `--schematic PATH`. It can only choose among multiple automatically derived schematic roots. Traversal, absolute, missing, duplicate-filename, child/non-root, and unnecessary non-ambiguous selection fail as invalid input; the selector cannot bypass inventory/path bounds or enable ZIP native execution.
- `doctor --json` now reports detected KiCad version/major/support, supported majors `[8,9,10]`, and separate PCB DRC, schematic ERC, coherent-project parity, ZIP, Altium, and generic-netlist capabilities. Text doctor output uses the same bounded claims.
- Added a coherent `schematic.sourcePair` with project identity plus exact schematic/board paths and SHA-256 digests. Validation requires it to agree exactly with the surrounding selected-root and board fields.
- Extended the existing review ledger and progressive evidence-link surfaces to display project/source pair, capability producer/class/status, mismatch UUID-path/item location and report-authoritative `gateImpact`, native producer/report versions, ERC/parity channel, and tri-state exclusion. Values are inserted with `textContent`/text nodes; the viewer does not recompute gate, exclusion, severity, or approval policy.
- README, skill, and report-contract reference now agree on complete-project input, selector bounds, KiCad 8/9/10 support, native exit 0/5 completion, auto/required behavior, exact-first occurrence joins, unknown exclusion, coherent parity, ZIP/Altium/netlist limits, evidence-only families, and the closed promotion checkpoint.

## Adversarial evidence

- Added selector regressions for exact ambiguous-root resolution and traversal, absolute, missing, duplicate-filename, and non-ambiguous misuse.
- Added core gate tampering validation: changing a reconciliation mismatch to blocking is rejected.
- Added Node contract mutations rejecting schematic insertion into `requiredEvidence` and blocking mismatch promotion.
- Added viewer/schema/docs alignment and safe-consumption checks for source-pair digests, structural marker location, exclusion unknown, and gate impact.
- Existing Phase 4 released-major, native failure, hierarchy, identity, mismatch, weak-reference, ZIP, `.SchDoc`, and netlist matrices remained green and evidence-only.

## Verification evidence

- `cargo test -p ratemypcb-core --test schematic_release --locked hierarchy_` — 7 passed.
- `cargo test -p ratemypcb-core --test schematic_release --locked reconciliation_` — 4 passed.
- `cargo test -p ratemypcb-core --locked decision_contract_generated_schemas_match_checked_in_json` — passed.
- `cargo test -p ratemypcb-cli --test decision_report --locked schematic_ && node --test tests/report-contract.test.mjs tests/report-ux.test.mjs` — passed.
- `cargo fmt --all -- --check && cargo test --all --locked && node --test tests/board-view.test.mjs tests/report-contract.test.mjs tests/report-ux.test.mjs && git diff --check` — passed: CLI unit 11, CLI integration 4, core unit 66, schematic integration 12, Node 29, no failures.
- Manual KiCad 10.0.5 sanitized fixture review — CLI exit 0; ERC and parity completed; parity retained 7 unknown-exclusion markers; exact source-pair digests serialized; no absolute checkout path; no schematic required-evidence entry.
- `git diff --cached --name-only` — empty.

## Residual risks

- KiCad 8/9 remain documentation/schema-attested rather than locally executed.
- Visible schematic capability/native records resolve through the existing schematic coverage evidence record; mismatches resolve to their occurrence-specific finding evidence. The report does not create a second public-evidence namespace per native marker.
- Source parsing remains intentionally explicit-fact only; it does not recreate ERC, infer wire graphs, or add Altium/native generic-netlist semantics.
- The schematic promotion checkpoint remains closed; no schematic family is required or blocking.
