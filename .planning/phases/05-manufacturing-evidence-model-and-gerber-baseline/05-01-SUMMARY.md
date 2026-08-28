---
phase: 05-manufacturing-evidence-model-and-gerber-baseline
plan: 01
status: complete
updated: 2026-08-27
requirements-completed: [FAB-01, FAB-02, FAB-07]
commits: 0
---

# Phase 5 Plan 01 Summary

Implemented and remediated the canonical fixed-point manufacturing model and fail-closed adapter/analyzer contract, then removed legacy filename/token/browser screening as fabrication approval authority. Automated Plan 05-01 gates are green and fresh independent acceptance closed FAB-01, FAB-02, and FAB-07. Plan 05-01 itself added no parser dependency, production Gerber adapter, or Plan 05-03 package work.

## Implementation

- Task 05-01-01 supplied the provenance-aware canonical fabrication DTOs, checked picometre geometry and transforms, exact resource contract, deterministic versioned identities/digests, report validation, generated schema, and model regressions.
- Replaced string manufacturing inventory with bounded original-byte inputs and serialized outcomes carrying normalized virtual path, kind candidate, exact size, retained/omitted/failed state, typed reason, and an exact original-byte SHA-256 only when bytes were retained. Directory and ZIP paths enforce 256 retained files, 4 MiB per file, 20 MiB retained aggregate, 2,000 archive entries, 512-byte normalized paths, and 12 directory levels without silent manufacturing omission.
- Added policy-free adapter facts/results and stable `package-gerbers`, `gerber-syntax`, and `drill-data` analyzer declarations. Dispatch requires every named prerequisite to be `complete`; partial, not-provided, unsupported, failed, stale, omitted, absent, or complete-without-a-semantic-result stays `not_checked`.
- Routed the three required fabrication families through the capability ledger. Before production adapters exist, retained Gerber/Excellon bytes produce attention, absent bytes produce not-provided, and bounded omissions produce failed coverage; none can pass.
- Deleted `gerber_syntax_valid` and drill token validation as pass sources. Filename/token observations are only `legacy-filename-screening` / `legacy-token-screening` partial capabilities with filename/file-content authority and an evidence-only limitation.
- Removed Gerber filename fallback from authoritative report stackup. The remaining helper is named `from_gerber_filename_inference` and explicitly warns that its result is partial and not construction evidence.
- Added runtime rejection for forged passed fabrication coverage over incomplete capability prerequisites. Filename-perfect, token-perfect, malformed-byte, per-file, aggregate, file-count, browser-authority, and observed-risk/approval mutation regressions remain fail-closed.
- Remediated independent-review blockers: transformed bounds now cover every geometry point and arc center; quantized expansion cannot claim complete expanded geometry; duplicate prerequisites and conflict-affected complete capabilities fail closed; retained byte inputs and outcomes are an exact bijection; tool spans and provenance locations are document-bounded; and fabrication evidence IDs are bound to the canonical model digest.
- Over-limit manufacturing inputs are not read or digested after metadata/count/aggregate rejection and carry an explicit absent digest. Retained reads enforce file/aggregate deadlines, while recognized manufacturing files below dot-prefixed directories remain accounted outcomes.
- Step-repeat counts include an offset-zero instance. Each feature definition counts once globally, its first repeat reuses that original, and later references charge full grids. Validation checked-multiplies and aggregates all instances, charges compact expansion allocation, validates step coordinates and count-minus-one offsets, and rejects repeated geometry beyond ±10 m.
- Retained outcomes now require exactly digest-present/reason-absent; omitted and failed outcomes require exactly digest-absent/reason-present in inventory, fabrication-review, report-mutation, and generated-schema contracts.
- Completed the generated fabrication JSON Schema with typed definitions, required fields, enum/reference constraints, collection bounds, geometry/transform variants, and `additionalProperties: false`. Pure Rust structural assertions prove the typed layer/geometry branches a conforming Draft 2020-12 validator must enforce, while serde mutation checks reject the same malformed runtime values without undeclared Python or network tooling.

## Verification evidence

- `cargo test -p ratemypcb-core --test fabrication_release --locked model_` — 20 passed.
- `cargo test -p ratemypcb-core --test fabrication_release --locked capability_` — 8 passed.
- `cargo test -p ratemypcb-core --locked legacy_fabrication_` — 7 passed.
- `cargo test -p ratemypcb-core --locked decision_contract_generated_schemas_match_checked_in_json` — passed; report and assessment schemas are semantically equal to checked-in JSON.
- `cargo fmt --all -- --check` — passed.
- `cargo clippy --all-targets -- -D warnings` — passed.
- `cargo test --all --locked` — 138 tests passed across CLI, core, fabrication, and schematic suites; no failures.
- `node --test tests/*.test.mjs` — 29 tests passed; no failures.
- CLI-generated `/tmp/ratemypcb-phase5-report-schema.json` matched `schemas/report-2.0.json` byte-for-byte and semantically.
- Manifest/lock/tree scan found neither `gerber_parser` nor `gerberx2`.
- `git diff --check` — passed.
- `git diff --cached --name-only` — empty.

## Acceptance state

- Two independent reviews returned blocking findings before remediation. Every finding received a targeted regression and the complete host gate remained green.
- A fresh post-remediation independent review rechecked all prior findings, the Plan 05-01 contract, planning traceability, parser-dependency absence, and host-executed gates, then returned **ACCEPTED** with no unresolved issue.
- Plan 05-01 and FAB-01/FAB-02/FAB-07 are complete. Plan 05-02 may proceed only through its separately gated dependency spike and blocking-human checkpoint.

## Honest deferrals

- Production Gerber/X2 parsing, the exact `gerber_parser = "=0.5.0"` dev-only readiness spike, local official corpus evidence, and the blocking-human PASS/STOP adoption checkpoint remain Plan 05-02.
- XNC/Excellon semantics, Gerber Job, package completeness, native KiCad/package reconciliation, product surfaces, official XNC closure, and final independent Phase 5 review remain Plan 05-03.
- ODB++/IPC-2581 decisions remain Phase 6; calibrated DFM/assembly policy remains Phase 7; broad fuzzing, performance/privacy/release hardening, publication, and skill adoption remain Phase 8.
- Phase 2 human accessibility/cross-browser/comprehension verification, provider legal/account gates, KiCad 8/9 local execution, source-aware Altium, and broader EDA deferrals remain open.
