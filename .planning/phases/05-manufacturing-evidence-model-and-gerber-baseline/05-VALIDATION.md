---
phase: 5
slug: manufacturing-evidence-model-and-gerber-baseline
status: executing
nyquist_compliant: true
wave_0_complete: true
created: 2026-08-27
---

# Phase 5 — Validation Strategy

> Fast, fixture-backed validation for the canonical model, bounded Gerber/X2 and XNC adapters, package completeness, and native-source reconciliation.

## Test infrastructure

| Property | Value |
| --- | --- |
| **Framework** | Rust built-in unit/integration harness + Node built-in `node:test` |
| **Config** | Workspace `Cargo.toml`; no new test framework |
| **Model command** | `cargo test -p ratemypcb-core --test fabrication_release --locked model_` |
| **Gerber spike command** | `cargo test -p ratemypcb-core --test fabrication_release --locked gerber_adoption_spike_` |
| **Gerber production command (PASS only)** | `cargo test -p ratemypcb-core --test fabrication_release --locked gerber_` |
| **XNC/package command** | `cargo test -p ratemypcb-core --test fabrication_release --locked package_` |
| **CLI tracer** | `cargo test -p ratemypcb-cli --test decision_report --locked fabrication_` |
| **Full phase command** | `cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test --all --locked && node --test tests/*.test.mjs && git diff --check` |
| **Feedback target** | Focused test after every task; full Rust/Node gate after each plan |

## Sampling rate

- **After every task:** run its exact `<verify><automated>` command.
- **After each plan:** run all locked Rust tests plus relevant Node contract suites.
- **Before fork candidate staging:** preserve the historical crates.io 0.5.0 STOP, create only the user-authorized public `ratemypcb/gerber-parser` fork and absent-path sibling clone, and validate exact upstream tokenizer commit `f4160c7…`. RED mutation evidence proved f416 still drops one outer `parse_line` error, so local candidate scope adds only the bound non-breaking two-file funnel/test diff `f9c64bba…`; a665/5ac remain excluded.
- **Before remote candidate mutation:** present exact `APPROVE_PUSH`/`REFUSE_PUSH` at the Plan 05-02 blocking-human gate. The human approved doing the fix on a branch and upstreaming it: this permits only one signed commit with parent f416, exact message and approved diff/file hashes, protected `refs/heads/ratemypcb/gerber-parser-accounting-fix`, one non-force push, and—after independent ACCEPT—one unmerged PR to `MakerPnP/gerber-parser:master`; any other commit, merge, tag, release, and publication remain prohibited.
- **Before dependency adoption:** pin the approved fork URL plus full SHA in core dev-dependencies only, run the negative authority-verifier matrix, immutable packet, independent review, sanitized/official/resource/error evidence, and present exact `PASS_F416`/`STOP_F416`. Production re-verification is read-only: remote `ls-remote`/governance plus exact local object/tree/parent/diff/archive/signature/Cargo checks, with no fetch or ref/worktree mutation in the sibling clone. PASS alone allows Plan 05-03 to promote that identical source; STOP or every malformed/stale/mismatched state cleans or aborts and cannot unlock production.
- **Before phase closure:** run generated-schema byte equality, official local Gerber/XNC corpus closure, sanitized fixture manifests, full fmt/clippy/Rust/Node/diff gates, GSD checks, and a fresh independent review.
- **No watch mode:** every command terminates.
- **No network in tests:** ordinary CI uses only repository fixtures. Official corpora and any differential oracle are explicit local-only commands.

## Fixture contract

Every repository fabrication fixture manifest records:

- project-authored/sanitized origin and redistribution license;
- exact bytes SHA-256 and intended dialect/spec subset;
- expected syntax, semantic, capability, omission, and completeness states;
- expected canonical counts, bounds, IDs, and model digest;
- every deliberate mutation and its expected failure class;
- no customer, provider, credential, official-corpus, or ambiguous third-party payload.

Official Ucamco archives remain outside the repository. The local checkpoint verifies these recorded archive SHA-256 values before use:

- fabrication test 1: `16329fda234b7f3e95651c29e8f381f445ab00ca4872d4e40eb072122d1d7625`;
- fabrication test 2: `28ca6f3b42931d7312d3229de07350fedacea1a785e32670a21f06817db6b007`;
- XNC tests: `9ad73e43cec479235ace152d8885ac8fbded4dc6c376e9afeb1b734b25b04e84`.

The advertised 2026 layer ZIP is recorded as unavailable because its endpoint returned one byte `0` on 2026-08-27. Missing local corpora or digest mismatch fails the adoption/closure checkpoint; it is not a skipped pass.

## Per-task verification map

| Task ID | Plan | Wave | Requirements | Threat refs | Secure behavior | Test type | Automated command | Status |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 05-01-01 | 05-01 | 1 | FAB-01 | T5-01, T5-04 | Fixed-point model, transforms, provenance, stable IDs/digests, limits, omissions, and conflicts validate deterministically | unit + model contract | `cargo test -p ratemypcb-core --test fabrication_release --locked model_` | ✅ green; independently accepted |
| 05-01-02 | 05-01 | 1 | FAB-02, FAB-07 | T5-04, T5-05 | Adapters cannot decide approval; analyzers fail closed on incomplete prerequisites; legacy token/filename/browser checks never pass semantic coverage | integration + gate mutation | `cargo test -p ratemypcb-core --test fabrication_release --locked capability_ && cargo test -p ratemypcb-core --locked legacy_fabrication_` | ✅ green; independently accepted |
| 05-02-01 | 05-02 | 2 | FAB-03 support | T5-01, T5-05, T5-07 | Authorized public fork and absent-path sibling clone verified; exact f416 tokenizer plus bound non-breaking outer-error funnel preserves API/dependencies/licenses and passes local gates | fork provenance + local path spike | exact Git/diff/file hashes; upstream fmt/Clippy-with-one-base-lint-allow/tests; RateMyPCB focused/official/Python gates | ✅ green |
| 05-02-02 | 05-02 | 2 | FAB-03 support | T5-05, T5-07 | Human approved branch/upstream path; normalize to `APPROVE_PUSH` for one signed bound commit, protected exact branch, one non-force push, and one independently accepted upstream PR | `checkpoint:decision`, `gate="blocking-human"` | recorded human statement plus bound diff SHA-256 | ✅ approved and executed exactly |
| 05-02-03 | 05-02 | 2 | FAB-03 support | T5-01..T5-07 | Signed head `54004bc…` is locked/protected, pinned as full-SHA dev git, archived deterministically, corpus/resource checked, independently ACCEPTed, and opened upstream as unmerged PR #26 | dependency/supply-chain + corpus + independent review | 57 fork, 145 Rust, 29 Node, 11 verifier mutations; direct 32-file verified-ZIP corpus; candidate verifier | ✅ green; no P0/P1/P2 |
| 05-02-04 | 05-02 | 2 | FAB-03 support | T5-05, T5-07 | Human selected PASS for immutable head `54004bc…` only; normalized to exact plan token `PASS_F416` | `checkpoint:decision`, `gate="blocking-human"` | candidate/review/remote/Cargo verifier passed immediately before display | ✅ human PASS |
| 05-02-05 | 05-02 | 2 | FAB-03 support | T5-05 | Duplicate-free decision artifact binds human statement, exact token, identities, timestamps, and candidate/review hashes without production promotion | authority binding | `python3 tests/verify_gerber_fork_candidate.py --mode decision` | ✅ green |
| 05-03-01 | 05-03 | 3 | FAB-03 | T5-01..T5-07 | Pre-cleanup attestation then exact STOP cleanup or identical PASS promotion/tracer; every other state aborts | fail-closed dispatch + tracer | production verifier; `gerber_semantics_tracer_` | ✅ authority/promotion green; independently accepted |
| 05-03-02 | 05-03 | 3 | FAB-03 | T5-01..T5-07 | PASS-only fixed-point Gerber semantics, hostile/resource/mutation/sanitized/direct official corpus and publication-source proof close FAB-03 | semantic + hostile + corpus + independent review | Read-only authority + 6 Python, 6 internal, 10 semantics, 8 hostile, 2 corpus, three parent and one independent direct 32-file official runs, 172 Rust, 29 Node, fmt/Clippy/schema/summary/diff/index | ✅ fresh independent ACCEPT; empty P0/P1/P2; FAB-03 complete |
| 05-04-01 | 05-04 | 4 | FAB-04, FAB-05 | T5-01..T5-06 | Exact completed PASS chain gates complete-only X2/Job authority, strict/named-legacy XNC, virtual Job paths, and package foundations | adapter + fixtures | production/summary verifiers; `x2_job_`, `xnc_`, `job_`, `package_completeness_` | ⬜ pending |
| 05-05-01 | 05-05 | 5 | FAB-06 | T5-03..T5-06 | Exact structured prerequisites gate native/package reconciliation and report/CLI/viewer/skill/schema integration | reconciliation + product surfaces | production/summary verifiers; core/CLI/Node/schema suites | ⬜ pending |
| 05-06-01 | 05-06 | 6 | FAB-08 | T5-01..T5-07 | Exact structured prerequisites, aggregate hostile/resource/corpus/full GSD gates, machine-readable independent ACCEPT review, and terminal traceability close Phase 5 | closure gate | full Rust/Node/schema/corpus/GSD checks plus review/summary verifier | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

### Plan 05-03 accepted evidence

Fresh independent verdict: **ACCEPT**, empty P0/P1/P2. Byte-preserved artifact SHA-256: `c5aeeb11ba555da285e380f399d30dff73312a4a7a7c668dc2020f6bf9108e02`.

Accepted hashes are `fabrication.rs` `65e9021643a9ef69b2168c0d91d12667e1c376db2e66c2d9067b84e403d8822e`, `fabrication_release.rs` `50a2d17591b3d69397ddca01304f1e53c32b969e3983e2d62db24c86113d8dd2`, schema `48c6ac1efc78aa411a51ffcd6d09938aaf378e6ff50b661907942ee02cbf5266`, and dependency `54004bc52c11699b49cd287a49135380feee86b3`.

Three consecutive parent official-corpus runs and one independent run produced identical totals: 32 files, 102,909 parser results, 102,908 successes, one parser error, one resolved Route, zero unaccounted errors, 32 normalization warnings, 83,570 features, 54,578 lines, 78 arcs, 23 regions, 28,891 flashes, and 6 macros. Authority/read-only checks, 6 Python verifier regressions, 6 internal, 10 semantics, 8 hostile, 2 corpus, fmt, Clippy `-D warnings`, schema equality, summary verification, diff/index checks, 29 Node, and 172 full locked Rust tests passed. The immutable independent JSON retains its reviewer-authored 169 count; executable full-suite logs total 172.

Only FAB-03 closes here. The Phase 5 critical path remains `05-04 -> 05-05 -> 05-06`; publication remains blocked by the Git-only production dependency.

## Requirement acceptance matrix

| Requirement | Owning plan | Required automated evidence |
| --- | --- | --- |
| FAB-01 | 05-01 | Canonical product/layer/geometry/transform/tool/profile/connectivity/assembly/construction/constraint records; fixed-point conversion and overflow tests; provenance/omission/conflict validation; stable IDs/digests. |
| FAB-02 | 05-01 | Adapter output cannot contain approval; analyzers declare prerequisites; complete/partial/not-provided/unsupported/failed truth table emits pass versus not checked correctly. |
| FAB-03 | 05-03 (05-02 gate) | Exact fork candidate adoption is supporting evidence; only PASS-only production parsing covers units/formats/apertures/macros/modal draws/arcs/regions/polarity/transforms/blocks/step-repeat with no unaccounted parser error or unsupported semantic loss. |
| FAB-04 | 05-04 | X2 file/aperture/object attributes plus bounded Gerber Job establish complete-only roles and explicit semantics; exact Route works; sparse/conflicting net/component/pin evidence remains partial. |
| FAB-05 | 05-04 | Strict XNC plus named exporter allowances preserve units/tools/diameters/plating/spans/drills/slots/routes and explicit unknowns under all budgets. |
| FAB-06 | 05-05 | Package completeness reconciles layer/profile/drill/extent/available connectivity with native KiCad facts; every mismatch links both provenances and closes approval where required. |
| FAB-07 | 05-01 | Current filename/token/stackup/browser screening is labeled partial/non-approval and cannot emit a passed semantic required check. Gerber baseline support remains explicit. |
| FAB-08 | 05-06 | Project-authored sanitized corpus, local official checkpoints, mutation/malformed/truncation/security/resource tests, deterministic output, legal manifests, GSD gates, and fresh independent review close successfully. |

Each FAB requirement has exactly one owning plan. Supporting tests may begin earlier, but only the owning plan claims requirement closure.

## Threat boundaries

| Ref | Boundary | Required regression |
| --- | --- | --- |
| T5-01 | Raw bytes/parser | Invalid UTF-8 only in bounded ordinary G04 comments; no silent parser errors; truncation/terminator/unsupported record failures. |
| T5-02 | Numeric/geometry | Decimal-only checked conversion, source resolution, coordinate/transform overflow, modal state, arc/region validity, block/SR expansion bounds. |
| T5-03 | Archive/Job paths | Existing virtual paths only; no traversal/extraction; duplicate/case/Unicode ambiguity and dangling references reject. |
| T5-04 | Semantic authority | Parse success differs from capability/package completeness; X2/Job conflicts preserved; filename presence and format badges never pass semantic coverage. |
| T5-05 | Identity/provenance/gate | Original digest plus canonical digest, stable structural IDs, duplicate rejection, prerequisite-gated analyzers, missing/partial/stale/unsupported cannot improve approval. |
| T5-06 | Resource/time | Exact file/aggregate/token/record/depth/text/numeric/geometry/allocation/deadline limits; no truncation to success, panic, hang, or unbounded expansion. |
| T5-07 | Corpus/legal/supply chain | Official assets local-only, repository fixtures redistribution-safe, dependency tree reviewed, `gerberx2` absent from production/lock/CI. |

## Wave 0 requirements

Existing Rust/Node infrastructure is sufficient. `crates/ratemypcb-core/tests/fabrication_release.rs` is created by 05-01 before its focused command. Sanitized fixture directories are created by the first task that consumes them. No test framework or production dependency is added in Wave 0.

`gerber_parser` is not a Wave 0 or production assumption. Historical crates.io 0.5.0 is human-STOP and absent. Plan 05-02 evaluates exact upstream tokenizer `f4160c7…` plus the bound non-breaking error-funnel commit `54004bc…` through the authorized fork. Its full-SHA dev git lock inclusion and open upstream PR are evidence, not adoption. Only exact eligible `PASS_F416` permits Plan 05-03 promotion; STOP/missing/stale/mismatch cleans or aborts, token screening stays downgraded, and no substitute parser is selected.

## Review-only gates

- User authorization covers only creation of the public `ratemypcb` fork and absent-path sibling clone.
- A human must separately select exact `APPROVE_PUSH`/`REFUSE_PUSH` before branch/protection/non-force push, then exact `PASS_F416`/`STOP_F416` after immutable evidence and independent review; auto-mode cannot approve either.
- Structured independent review informs adoption and final Phase 5 closure but never replaces either human decision.
- Official-corpus inputs remain local-only; their digests and non-redistribution handling are part of the displayed packet and automated local run.
- No reviewer decision can waive a parser error, unsupported semantic record, failed resource bound, or missing capability into a pass.

## Validation sign-off checklist

- [x] Every focused task command through 05-02-03 passes.
- [x] Exact `gerber_parser = "=0.5.0"` is first dev-only, the candidate lock graph and spike execute under `--locked`, production source/normal edges remain absent, and the compact valid-stream regression fails closed to a STOP recommendation.
- [x] The historical registry checkpoint recorded human STOP; the replanned fork commit/push checkpoint separately recorded explicit human branch/upstream approval.
- [x] STOP removed the exact crates.io dev pin and candidate-only lock entries and blocked the old 05-02-03/04 path; the reusable regression harness remains non-production input to the separately reviewed fork replan.
- [x] Six canonical numeric Phase 5 plans materialize the fork gate, PASS-only Gerber, foundation, integration, and closure sequence with unique requirement ownership.
- [x] Authorized fork/local clone and exact f416-based two-file candidate pass local upstream/focused/official/verifier gates with no branch, commit, staging, or push.
- [x] Exact signed-commit/push authority is recorded and executed once; upstream PR #26 is open and unmerged.
- [x] Exact fork production-adoption gate records human `PASS_F416` for immutable head `54004bc…`; production still requires Plan 05-03 re-verification.
- [x] Every source frame is reconciled to its exact ordered parser-result group; exact Route FileFunction semantics retain siblings.
- [x] Fixed-point conversion/modal/geometry/load-transform tests prove aperture-only LM/LR/LS semantics without moving operation coordinates.
- [ ] Strict XNC and each KiCad/LibrePCB allowance has a named fixture and hard-negative sibling.
- [x] Official local Gerber corpus digests/results and the unavailable 2026 ZIP are recorded honestly; XNC remains Plan 05-04.
- [x] Repository Gerber fixture manifests prove redistribution safety and exact hashes.
- [x] Required fabrication coverage cannot pass from filename/token/browser evidence.
- [x] Final aperture/performance remediation, generated/checked-in schema equality, and full fmt/Clippy/Rust/Node gates passed; fresh independent ACCEPT closes FAB-03 only.
- [x] Production authority verification is non-mutating and proves exact dependency `54004bc…`; a recorded-command regression and unchanged sibling snapshot prohibit fetch/ref/worktree mutation.
- [x] GSD phase index, frontmatter, structure, references, requirement coverage, and consistency pass for Plan 05-03 closure.
- [x] Fresh independent `05-03-INDEPENDENT-REVIEW.json` reports ACCEPT with empty P0/P1/P2; no files are staged.

**Approval:** The historical crates.io candidate remains human-STOP. Immutable fork head `54004bc…` retains human `PASS_F416` authority for this production work. Fresh final Plan 05-03 review is **ACCEPT** with empty P0/P1/P2, so FAB-03 and Plan 05-03 are complete. Plans 05-04, 05-05, and 05-06 remain pending; PR merge, release, publication, and further fork mutation remain unauthorized.
