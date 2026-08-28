---
phase: 4
slug: kicad-schematic-release-evidence
status: complete
nyquist_compliant: true
wave_0_complete: true
created: 2026-08-27
---

# Phase 4 — Validation Strategy

> Fast, fixture-backed validation for native execution, hierarchy identity, cross-artifact reconciliation, and bounded capability claims.

## Test Infrastructure

| Property | Value |
| --- | --- |
| **Framework** | Rust built-in unit/integration harness + Node built-in `node:test` |
| **Config** | Workspace `Cargo.toml`; no Node config or new test framework |
| **Focused native command** | `cargo test -p ratemypcb-core --locked schematic::tests::native_` |
| **Focused release command** | `cargo test -p ratemypcb-core --test schematic_release --locked` |
| **CLI tracer command** | `cargo test -p ratemypcb-cli --test decision_report --locked schematic_` |
| **Full phase command** | `cargo fmt --all -- --check && cargo test --all --locked && node --test tests/board-view.test.mjs tests/report-contract.test.mjs tests/report-ux.test.mjs && git diff --check` |
| **Feedback target** | Focused test after every task; full suite after every wave |

## Sampling Rate

- **After every task:** run its exact `<verify><automated>` command.
- **After every wave:** run the full Rust suite plus relevant Node report/viewer suites.
- **Before Phase 4 verification:** run fixture manifest checks, generated-schema equality, plan/requirement traceability, full phase command, and `git diff --check`.
- **No watch mode:** every command terminates.
- **No network/install gate:** fixtures are checked in; tests do not download KiCad, packages, schemas, or corpora.

## Fixture Contract

Every native fixture manifest records:

- project-authored/sanitized redistribution status;
- exact command vector and expected exit class;
- producer version and report `kicad_version`;
- official schema URL/branch;
- whether output was locally executed or documentation/schema-attested;
- source/output SHA-256 digests and sanitization notes.

KiCad 10.0.5 may be recaptured locally during execution. KiCad 8/9 fixtures remain explicitly documentation/schema-attested unless pinned binaries are actually run; tests and docs may not call those fixtures locally executed.

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirements | Threat refs | Secure behavior | Test type | Automated command | Status |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 04-01-01 | 04-01 | 1 | SCH-03, SCH-04 | T4-01, T4-03, T4-07 | Only majors 8/9/10 dispatch; 0/5 complete; channels and tri-state exclusions survive normalized parsing | unit + fixture contract | `cargo test -p ratemypcb-core --locked schematic::tests::native_` | ✅ green |
| 04-01-02 | 04-01 | 1 | SCH-03, SCH-04, SCH-07 | T4-01, T4-02, T4-05 | Shared runner is bounded/non-mutating; auto failures are not-run; required failures error; native version/digests reach provenance | unit + regression | `cargo test -p ratemypcb-core --locked native_` | ✅ green |
| 04-02-01 | 04-02 | 2 | SCH-01, SCH-02, SCH-05 | T4-02, T4-03, T4-04 | Root/child graph is bounded and project-confined; reused occurrence paths remain distinct; broken context fails closed | integration | `cargo test -p ratemypcb-core --test schematic_release --locked hierarchy_` | ✅ green |
| 04-02-02 | 04-02 | 2 | SCH-04, SCH-05, SCH-06, SCH-07, SCH-08 | T4-03, T4-04, T4-05, T4-06 | Exact-first joins detect every mutation; weak fallback is labeled; ZIP/Altium/netlist claims remain bounded and non-blocking | integration + mutation | `cargo test -p ratemypcb-core --test schematic_release --locked reconciliation_` | ✅ green |
| 04-03-01 | 04-03 | 3 | SCH-01, SCH-03, SCH-04, SCH-06, SCH-08 | T4-05, T4-06 | CLI selector/doctor, report/schema, viewer, and skill expose one capability contract without client-side policy | CLI + schema + Node | `cargo test -p ratemypcb-cli --test decision_report --locked schematic_ && node --test tests/report-contract.test.mjs tests/report-ux.test.mjs` | ✅ green |
| 04-03-02 | 04-03 | 3 | SCH-02, SCH-03, SCH-04, SCH-06, SCH-07, SCH-08 | T4-01..T4-07 | Adversarial failures cannot become passes, excluded markers cannot block, evidence-only families cannot enter approval, and all docs/contracts align | full regression | `cargo fmt --all -- --check && cargo test --all --locked && node --test tests/board-view.test.mjs tests/report-contract.test.mjs tests/report-ux.test.mjs && git diff --check` | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

## Requirement Acceptance Matrix

| Requirement | Required automated evidence |
| --- | --- |
| SCH-01 | Directory and ZIP inventory recognize schematic/project/library/netlist candidates under existing bounds; coherent board/project/root selection and explicit ambiguity/override tests. |
| SCH-02 | Reused child occurrences have different project/root/UUID-path identities; missing child, unresolved variable, broken path, ambiguous root, duplicate instance, and cycle fixtures fail closed. |
| SCH-03 | Exact 8/9/10 adapters; exit 0/5 completion; unavailable/timeout/7/11/nonzero/missing/oversized/malformed/truncated/version-mismatch not-run; required-mode error; all markers and tri-state exclusions preserved. |
| SCH-04 | Parity runs only for a coherent board/root/project pair; parity remains a distinct channel with pair paths/digests/revision/version and stable marker evidence. |
| SCH-05 | Native BOM/netlist/position outputs are authoritative; source fallback facts expose producer/evidence class per fact and cover occurrence, UUID/unit, fitted/DNP, fields, footprint, pins, power intent, and connectivity. |
| SCH-06 | Clean control plus one mutation each for reference/UUID, value, footprint, fitted/DNP, pin-pad, net, quantity, placement, and stale revision; every mismatch resolves to source occurrence evidence. |
| SCH-07 | New schematic families are explicitly evidence-only/non-blocking; gate mutation tests reject silent promotion; no heuristic family is introduced. |
| SCH-08 | `.SchDoc`, recognized generic netlists, unknown netlists, and ZIP fixtures prove unsupported/not-checked capabilities and absence of native ERC/hierarchy/parity/source-aware claims. |

## Threat Boundaries

| Ref | Boundary | Required regression |
| --- | --- | --- |
| T4-01 | Native process/exit/output | Fixed argv (no shell), 120s-or-lower timeout, kill/wait/cleanup, bounded stderr/report, fresh output, 0/5 only completion, no save/refill mutation. |
| T4-02 | Project/archive paths | Root-confined relative children, no traversal/external symlink, bounded entries/depth/bytes, duplicate normalized paths rejected, ZIP native checks not run. |
| T4-03 | Parser/resources | UTF-8/JSON/type validation and explicit byte/token/depth/string/occurrence/marker limits; over-limit is not-run/not-checked, never truncated success. |
| T4-04 | Identity/reconciliation | Project + root digest + UUID occurrence path + item UUID; reference fallback only when unique and confidence-labeled. |
| T4-05 | Evidence/provenance/gate | Actual producer version and exact artifact/pair digests; canonical location IDs; excluded/unknown states retained; evidence-only families cannot affect approval. |
| T4-06 | Capability/presentation | No native Altium/generic-netlist/ZIP claims; viewer uses `textContent`/existing escaped payload and does not derive analysis or gate policy. |
| T4-07 | Fixture provenance | Project-authored/sanitized data, command/version/schema manifests, no customer/provider/credential data, no unsupported local-execution claim. |

## Wave 0 Requirements

Existing Rust and Node built-in infrastructure is sufficient. Plan 04-01 creates the sanitized released-major/native-failure corpus before its parser tests. Plan 04-02 creates `crates/ratemypcb-core/tests/schematic_release.rs` and the hierarchy/mismatch/bounded-EDA corpus before running integration commands. No package installation, test framework, live service, credential, or missing fixture placeholder is allowed.

## Manual-Only Verifications

None are required to prove Phase 4's automated contract. The promotion checkpoint remains deliberately closed: a human must separately approve any future schematic check family before it becomes release-blocking. Phase 2 hands-on accessibility/usability and cross-browser checks remain deferred and are not claimed by Phase 4.

## Validation Sign-Off Checklist

- [x] Every task's focused command passes.
- [x] All released-major fixture manifests validate and preserve execution-attestation distinctions.
- [x] Clean control and every required mutation are adjudicated.
- [x] Generated `schemas/report-2.0.json` equals checked-in JSON semantically and byte-for-byte.
- [x] Auto and required native failure paths produce the documented report/exit behavior.
- [x] No new schematic family appears in required evidence or blocking policy.
- [x] Full Rust/Node/format/diff gate passes.
- [x] GSD plan frontmatter, plan structure, phase index, and SCH-01..08 traceability pass.

**Approval:** automated validation complete; independent acceptance review and the separate promotion/adjudication gate remain pending.
