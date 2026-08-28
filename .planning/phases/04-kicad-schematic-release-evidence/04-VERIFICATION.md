---
phase: 04-kicad-schematic-release-evidence
status: passed
automated_status: passed
verified: "2026-08-27T09:15:20Z"
score: 10/10 Phase 4 automated must-haves passed after third independent-review remediation
requirements: [SCH-01, SCH-02, SCH-03, SCH-04, SCH-05, SCH-06, SCH-07, SCH-08]
promotion_checkpoint: closed
---

# Phase 4 Verification

## Result

Phase 4 passes its automated contract after a third narrow remediation of two independently confirmed validation defects. The bounded KiCad 8/9/10 native seam, hierarchy identity, explicit facts, reconciliation, CLI, schema, viewer, and documentation agree without adding Phase 5 behavior or promoting schematic evidence into release-blocking policy.

All three Phase 4 plans and summaries exist. SCH-01 through SCH-08 are complete for the declared scope. A fresh independent acceptance review of this remediation remains required.

## Final automated evidence

- `cargo fmt --all -- --check` — passed with no output.
- `cargo clippy --all-targets -- -D warnings` — passed for both workspace crates.
- `cargo test --all --locked` — passed: CLI unit 11, CLI integration 6, core unit 72, schematic integration 15, doc tests 0; no failures.
- `node --test tests/*.test.mjs` — passed: 29 tests, 0 failed.
- `python3 -m py_compile kicad-plugin/review_action.py skills/review-pcb-dfm/scripts/install.py skills/review-pcb-dfm/scripts/enrich_bom.py` — passed.
- `python3 -m unittest discover -v` — passed with 0 discovered Python unit tests.
- `python3 skills/review-pcb-dfm/scripts/install.py --version v0.1.0 --dry-run` — passed without installation or network access.
- `python3 skills/review-pcb-dfm/scripts/enrich_bom.py --self-test` — passed; provider-neutral forbidden-query defaults remain intact.
- `cargo run --locked -q -p ratemypcb-cli -- schema --output /tmp/ratemypcb-report-2.0.json` plus `cmp schemas/report-2.0.json /tmp/ratemypcb-report-2.0.json` — passed; generated and checked-in report schema are byte-for-byte equal at 30,174 bytes.
- `cargo test -p ratemypcb-core --locked decision_contract_generated_schemas_match_checked_in_json` — passed semantic report/assessment schema equality.
- `cargo check --all-targets --locked` — passed.
- `node --check` for every repository JavaScript module — passed for 6 modules.
- `git diff --check` — passed.
- `git diff --cached --name-only` — empty; no staged files.

## Previously recorded live diagnostics

These first-remediation diagnostics are retained as historical environment evidence and were not rerun for the narrow second remediation. Their old composite value is superseded by the canonical native-fact binding and is not claimed as the current expected digest.

- `ratemypcb doctor --json` detected KiCad `10.0.5`, major 10, supported `true`, and supported majors `[8,9,10]`. It reported separate PCB DRC, schematic ERC, coherent-project parity, ZIP, Altium, and generic-netlist capabilities; Nexar credentials remained false.
- A required-native review of `tests/fixtures/kicad/supported/10/hierarchical` exited 0. ERC and parity completed with executable/report version `10.0.5`; the report contained no absolute checkout path and no schematic required-evidence entry. The old run's composite digest predates `schematic:native-export-facts`; second-remediation determinism and channel counts are established by the focused synthetic regressions below rather than reused as live evidence.

## Independent-review remediation evidence

- Confirmed and fixed normalized parent-plus-stem coherence for selected board, schematic, and project; a duplicate-stem cross-directory regression now proves the selected source pair is colocated, while a missing colocated schematic remains explicitly `incoherent_project`.
- Confirmed and fixed reused-sheet board reconciliation: per-occurrence `symbol_instances` reference/unit data is parsed, full sheet-plus-item board paths are joined uniquely, and a two-occurrence/two-footprint mutation is attributed only to the changed occurrence.
- Confirmed and fixed ordering-dependent native marker identity using channel, sheet UUID path, rule type, and sorted item UUIDs; reorder regression IDs and structural locations are identical.
- Confirmed and fixed schematic provenance using a validated deterministic composite of selected source artifact identities/digests, channel-specific ERC/parity evidence records and producer versions, and report-visible normalized native export facts. Raw native export digests remain recorded but their volatile timestamp/path metadata is excluded from canonical reconciliation identity.
- Confirmed and fixed `--fail-on`: only `blocking` findings affect the release-gate exit; a real CLI mismatch regression proves a medium `evidence_only` schematic mismatch exits 0.
- Confirmed and fixed export validation and delimited overflow: malformed/empty native output and malformed rows are rejected, 20,001 records cannot be silently truncated, and completed exports expose bounded normalized facts.
- Confirmed and fixed lowercase SHA-256 runtime/schema disagreement and persisted absolute operational input paths. Mutations for uppercase artifact/occurrence digests and schematic provenance are rejected; an absolute-path CLI regression serializes only `mismatch-project`.
- Confirmed and filled the cited native adversarial test gaps: exit 5 with a valid report, oversized process output, executable/report major mismatch, unsupported-major auto/required mapping, malformed export output, and CLI required-native exit 3 all execute in regressions.
- The reviews' positive observations were reproduced directly: native argv remains shell-free and fixed, majors remain exactly 8/9/10, completed exits remain 0/5, timeout/overflow paths kill and wait, fresh temporary outputs remain bounded, exclusion states remain tri-state, viewer values still use safe text APIs, and generated/checked-in schemas remain equal. No remediation was needed for those already-correct areas.

## Second-remediation evidence

- Canonical native-export binding: `schematic:native-export-facts` now hashes sorted tuples of occurrence key plus every stored semantic native fact field. Raw `native:*` byte digests remain visible and excluded from `schematic:composite`, while the non-native canonical fact digest is included. Runtime validation independently recomputes it and rejects missing, extra, mismatched, or self-consistently recomposed forged digest values. `cargo test -p ratemypcb-core --locked canonical_native_fact -- --nocapture` passed both reorder/raw-volatility/value-mutation and report-forgery tests.
- Channel-specific viewer provenance: the pure `schematicEvidenceRefs` helper returns the exact generic, ERC, or parity evidence ID and falls back to generic only when the requested channel record is absent. `local-viewer.js` uses generic refs for source-pair/capability rows and one channel-specific ref for each native report and all markers from that invocation. The focused Node test passed exact helper outputs and verified that both native report and marker entry calls receive `nativeRefs`.
- Parity counts: native JSON normalization still retains and stabilizes every ordinary violation, unconnected item, and schematic-parity marker, but counts only the invoked semantic channel. The mixed-channel regression passed with ordinary counts `(2 active, 1 excluded, 1 unknown)` and parity counts `(1 active, 1 excluded, 0 unknown)` over the same six retained markers.
- Focused validation passed before the full gate: all 18 schematic unit tests, 5 CLI viewer-filtered tests, and all 11 report UX tests passed.
- The required second-remediation gate passed: formatting, Clippy, full locked Rust tests, all Node tests, generated/checked-in schema equality, and whitespace checks. No staged files were present. The retained Python, `cargo check`, and standalone JavaScript syntax evidence above was not rerun; the required gate used Clippy, full Rust tests, and the full Node suite instead.

## Third-remediation evidence

- Runtime native-report validation now receives an explicit semantic channel at every call site: schematic ERC counts only `erc`, schematic parity counts only `schematic_parity`, and board/profile DRC counts `violations` plus `unconnected_items`. All four marker groups remain retained, bounded, and validated. The mixed parser-to-runtime regression retains six markers, accepts the parser-produced ordinary `(2, 1, 1)` and parity `(1, 1, 0)` counts, and rejects cross-channel validation and forged all-marker counts.
- Canonical native-fact selection now follows the validated `explicit-export-facts` evidence class rather than trusting `sourcePath`. Runtime validation bounds all six fact text fields and permits only coherent source provenance (`kicad-source`, `explicit-source-fact`, occurrence source path) or coherent native provenance (supported bounded `kicad-cli` version, `explicit-export-facts`, exact native export path). Mixed and unknown combinations reject.
- Adversarial runtime mutations now reject source-path-only, evidence-class-only, producer-only, missing-digest, unknown-class, oversized-value, and self-consistently rebound native-fact/composite changes. The same regression confirms legitimate native and source-only reports still validate.
- Focused tests passed before the full gate: `native_drc_channels_and_exclusions_remain_distinct` and `canonical_native_fact_digest_cannot_be_forged_independently_of_facts` each passed independently. The first formatting check reported the newly edited Rust formatting diff; `cargo fmt --all` applied it, and the required subsequent formatting check passed.
- The required third-remediation gate passed: `cargo fmt --all -- --check`, Clippy with warnings denied, all 104 locked Rust tests, all 29 Node tests, byte-for-byte generated report schema equality, semantic report/assessment schema equality, `git diff --check`, and an empty staged-file list.

## GSD and traceability

- Plan frontmatter, plan structure, references, phase completeness, phase plan index, roadmap convention, and planning consistency checks passed.
- All declared must-have artifacts exist: 14/14 artifact checks passed across 04-01 through 04-03.
- SCH-01 through SCH-08 were reported ready by the requirements handler and were marked complete in both checkbox and traceability surfaces.
- The mechanical key-link checker reported the six prose/path links as not source-literal references; this is not a product failure. The linked behavior is exercised by the passing Rust integration, CLI tracer, Node contract, schema, and viewer tests.
- GSD health has no errors. Its two retained warnings predate Phase 4: optional `workflow.ai_integration_phase` is absent, and `.planning/DISPATCH.md` is non-canonical.
- The open-artifact audit retains only Phase 2's acknowledged human-needed accessibility/usability verification.

## Honest deferrals

- KiCad 8 (`8.0.9`) and KiCad 9 (`9.0.6`) fixtures remain documentation/schema-attested, not locally executed. KiCad 10 (`10.0.5`) is the only locally executed released-major fixture.
- Human source/corpus adjudication and independent review remain required before any schematic family becomes required or blocking. The promotion checkpoint is closed.
- Phase 2 hands-on accessibility, representative usability, and cross-browser checks remain deferred and are not claimed here.
- Provider-specific legal/account/query/cache/embed/share/backup/retention/expiry approvals remain deferred. No live provider adapter was enabled and existing provider legal boundaries were preserved.
- Altium native checks, generic-netlist native semantics, and ZIP native execution remain unavailable as documented.

## Conclusion

Automated Phase 4 must-haves pass, so roadmap execution advances to Phase 5. Phase 5 itself remains not started. No commit, staging, installation, release, archive, or remote Git operation was performed.
