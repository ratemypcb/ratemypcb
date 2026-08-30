# Generic-agent workaround — Phase 5 fork replan, revision 3

> Proposal only. No RateMyPCB repository or GitHub resource was modified. Evidence snapshot: 2026-08-27.

## Canonical six-plan shape

| Plan | Wave | Requirements | Preserved scope |
| --- | ---: | --- | --- |
| 05-01 | 1 | FAB-01, FAB-02, FAB-07 | Existing completed canonical model and honest legacy downgrade. |
| 05-02 | 2 | FAB-03 supporting gate | Authorized fork/local f416 candidate, exact push dispatch, verifier/negative matrix, packet/review/human decision. |
| 05-03 | 3 | FAB-03 owner | Full decision dispatch, attested STOP cleanup or PASS-only production Gerber semantics/hostile/corpus. |
| 05-04 | 4 | FAB-04, FAB-05 | Existing old task 05-03-01 only: strict XNC, bounded Job, D-09 complete-only X2/Job authority and package foundations. |
| 05-05 | 5 | FAB-06 | Existing old task 05-03-02 only: native/package reconciliation and report/CLI/viewer/skill integration. |
| 05-06 | 6 | FAB-08 | Existing old task 05-03-03 only: hostile/resource/corpus/full gates/independent review/Phase closure. |

The split distributes the old canonical 05-03 tasks one-to-one. It does not add product scope.

## Authority and exact tokens

Existing user authorization is sufficient for only:

```sh
test ! -e ../gerber-parser && test ! -L ../gerber-parser
gh repo fork MakerPnP/gerber-parser --org ratemypcb --clone=false
```

Then the fork may be cloned into the proven-absent sibling. No fork-creation checkpoint is added.

The separate remote-mutation checkpoint uses one token per outcome everywhere:

- option ID/name/response/dispatch `APPROVE_PUSH`;
- option ID/name/response/dispatch `REFUSE_PUSH`.

The post-checkpoint task writes `05-02-PUSH-DECISION.json` and dispatches both paths. `REFUSE_PUSH` removes the temporary path dev edge and candidate lock graph, proves parser absent from production/manifests/lock, records STOP, and terminates before adoption. After local RED→green evidence exposed one additional silent-error path, the human approved doing the work on a branch and upstreaming it. This is recorded as `APPROVE_PUSH` and authorizes exactly one signed commit with parent f416, message `fix: retain outer parse errors`, two-file diff SHA-256 `f9c64bbabd9731ccb68ce8708c64048fe8ae4fe7aff20931062504448d3c1787`, exact `refs/heads/ratemypcb/gerber-parser-accounting-fix`, targeted active protection/ruleset, one non-force push, and—only after independent ACCEPT—one unmerged PR to `MakerPnP/gerber-parser:master`. It authorizes no other source change or remote operation.

Final adoption similarly uses exact `PASS_F416` and `STOP_F416` in option IDs/names/resume/decision JSON/Plan 05-03 logic.

## Exact candidate and fail-closed accounting

- release base: `8a07cc6064894cbf63978012969af5c1f656a30b`;
- tokenizer foundation: `f4160c7c6ca1b4cdd9c5273a3916b4fd087b5e34`, tree `e7fb208975130e9c1f41019ba75cb7c960dbf02f`;
- local repair diff SHA-256: `f9c64bbabd9731ccb68ce8708c64048fe8ae4fe7aff20931062504448d3c1787`;
- repaired source SHA-256: `170003700d3fe343667e00b4c7ad225ccb0b71f6c5a35fb170cc5a128080f366`;
- repaired test SHA-256: `29bb344de8ff7e6c741b861479d11a3891802aa5f320a6cd4d4d801941fa6980`;
- changed paths across base→candidate remain only `src/parser.rs` and `tests/component_tests.rs`;
- excluded: a665's breaking `ErrorContext` API and `5ac302c1fbd382c073fe81f8a2c711f66c2dcc7b` grammar expansion.

Temporary path staging changed compact input from 2 records/1 success/1 error to 9/9/0, but inserting a standalone validly framed invalid `Q*` before each command then proved unmodified f416 still dropped the outer `parse_line` `Err`. The minimal local repair converts that outer error to `vec![Err(error)]` and removes the outer `flatten`, preserving the public error type. Upstream tests now pass 57 total; RateMyPCB's seven focused and direct verified-ZIP 32-file corpus tests pass with 102,909 records, exact Route handling, 32 normalizations, zero unaccounted errors, and automated candidate recommendation PASS. Rust 1.96's one pre-existing `blocks_in_conditions` lint at unchanged base test line 850 is narrowly allowed during Clippy; no source suppression is added.

## Authority verifier and negative matrix

Canonical path is `tests/verify_gerber_fork_candidate.py`. It uses Python stdlib only and duplicate-safe JSON parsing (`object_pairs_hook` rejects duplicate keys). Modes:

- `candidate`: exact packet/review/archive/ref/Cargo dev placement before PASS_F416 checkpoint;
- `decision`: additionally exact human decision/hash/timestamp before Plan 05-03 dispatch;
- `production`: exact PASS_F416 plus normal-only Cargo placement before production/downstream work;
- `cleaned-stop`: exact STOP_F416 after cleanup, parser absence, pre-cleanup source hash equality and exact STOP traceability.

`tests/test_verify_gerber_fork_candidate.py` is table-driven. A valid baseline passes; each mutation must exit nonzero with a named diagnostic:

1. duplicate JSON keys;
2. stale timestamp;
3. candidate packet hash drift;
4. nonempty P0;
5. nonempty P1;
6. nonempty P2;
7. wrong remote ref;
8. wrong fetched tree;
9. wrong Cargo placement.

Test probes are import-only dependency injection and no CLI bypass/mock flag exists.

## STOP attestation and cleaned-stop proof

Plan 05-03's literal first command is full decision verification. Only after success it writes `05-03-PRECLEAN-ATTESTATION.json` containing:

- decision/packet/review SHA-256 bindings;
- ordered UTC timestamp;
- sorted relative paths and SHA-256 for every regular non-symlink Rust source below `crates/ratemypcb-core/src`;
- no absolute paths.

On STOP_F416, cleanup removes the dev edge/lock graph and edits only exact traceability:

- `05-03-SUMMARY.md`: duplicate-free frontmatter `status: stopped`, `decision: STOP_F416`, `requirements_completed: []`;
- `05-VALIDATION.md`: verified STOP cleanup, production blocked;
- ROADMAP: 05-03 incomplete/blocked;
- STATE: stopped at verified fork STOP;
- REQUIREMENTS: FAB-03 remains unchecked.

Then cleaned-stop mode revalidates authority, parser absence, every attested source byte, and exact traceability. Thus post-cleanup verification does not incorrectly expect the old dev Cargo edge, and STOP cannot change production source or unlock downstream work.

## PASS and downstream hard prerequisites

PASS_F416 moves the identical literal git URL/full rev from core dev to core normal dependencies. Production verifier passes after promotion and before source edits.

Every downstream plan 05-04, 05-05 and 05-06 has both:

1. literal first automated command `python3 tests/verify_gerber_fork_candidate.py --mode production`;
2. exact completed 05-03 checks: `status: complete`, `decision: PASS_F416`, `requirements_completed: [FAB-03]`, plus checked FAB-03 requirement.

A shared stdlib summary verifier rejects duplicate frontmatter keys and requires exact status, decision, ordered requirements array, and matching checked REQUIREMENTS rows; prose mentions never count. Later plans require that verifier for each preceding summary. A completed STOP Plan 05-03 therefore cannot unlock implementation through dependency completion alone.

## Preserved old package behavior

### Plan 05-04 — old task 05-03-01

Preserves strict XNC 2021.11, signature-selected KiCad/LibrePCB allowlists and hard negatives, original-byte/resource limits, explicit unknown plating/span, bounded Gerber Job 2023.06 and virtual paths, deterministic grouping and package completeness prerequisites.

Adds only the checker-required explicit ownership already implied by D-09/FAB-04: X2/Job outrank filenames; disagreement is conflict; role completeness requires all intended documents; net/component/pin completeness requires all eligible objects; exact Route fields persist and cannot mask errors. Requirements `[FAB-04, FAB-05]`.

### Plan 05-05 — old task 05-03-02

Preserves bounded native KiCad facts, separate source/package models, complete-prerequisite layer/profile/drill/extent/connectivity reconciliation, stable required-check IDs, additive report/schema, actual CLI digest/render tracer, safe core-owned viewer, skill/reference/README/CI alignment and all deferrals. Requirement `[FAB-06]`.

### Plan 05-06 — old task 05-03-03

Preserves aggregate XNC/Job/package and rerun Gerber hostile/resource/mutation matrix, sanitized manifest legality, official Gerber+XNC exact-buffer corpus, determinism, full fmt/Clippy/Rust/Node/schema/JS/dependency/diff/staged gates, known-gap preservation, and terminal Phase 5 closure. Closure requires one ordinary bounded independent product review. Requirement `[FAB-08]`.

## Safe shell/publication behavior

Plan 05-03 uses `set -euo pipefail` and sequential commands so any verifier/check/test failure terminates immediately. Publication capture is last, after successful focused/full/corpus gates. Unexpected publish success fails; expected failure must name `gerber_parser` and git/repository source. No grouped capture can run after or mask earlier failures.

Official corpus always uses externally supplied nonempty `RATEMYPCB_UCAMCO_CORPUS`; no hard-coded path. The two Gerber and one XNC archive digests are verified before direct member parsing from the same buffers. Missing/mismatch is failure, not skip.

## Atomic planning materialization

Before any GSD discovery/execution, the parent materializer must perform one atomic planning update:

1. overwrite canonical `05-02-PLAN.md` with `/tmp/ratemypcb-05-02-PLAN.md`;
2. overwrite canonical old `05-03-PLAN.md` exactly with `/tmp/ratemypcb-05-03-PLAN.md` so no old task IDs/content remain;
3. create canonical numeric `05-04-PLAN.md`, `05-05-PLAN.md`, `05-06-PLAN.md` from matching `/tmp` files;
4. ensure exactly one canonical plan for each ID 05-01 through 05-06 and no slug/duplicate old 05-03 plan;
5. update ROADMAP to six Phase 5 plans, waves/dependencies/descriptions, with 1/6 complete before execution;
6. update STATE total plan count from 15 to 18, current position/fork replan, and dependencies;
7. update REQUIREMENTS ownership: FAB-01/02/07→05-01, FAB-03→05-03 (05-02 supporting gate), FAB-04/05→05-04, FAB-06→05-05, FAB-08→05-06;
8. replace VALIDATION task matrix/waves/status/review gates and acceptance ownership for 05-02 through 05-06;
9. preserve `05-02-DEPENDENCY-CHECKPOINT.md` unchanged;
10. run GSD discovery/frontmatter/structure/reference/requirement/consistency only after all above writes commit atomically.

Execution plans later update planning files only for actual terminal results, not to repair stale discovery.

## Residual fail-closed questions

| Question | Gate |
| --- | --- |
| f416 complete accounting? | RED evidence found one dropped outer error; the only candidate repair is the bound non-breaking two-file diff, gated before signed commit/push. |
| Candidate push allowed? | Exact APPROVE_PUSH/REFUSE_PUSH dispatch. |
| Candidate current/clean? | Candidate verifier plus negative matrix before PASS_F416. |
| STOP cleanup safe? | Pre-cleanup hashes plus cleaned-stop authority/source/traceability proof. |
| Production/downstream exact PASS? | Production verifier first plus completed PASS_F416/FAB-03 summary. |
| Corpus present and exact? | Nonempty env and exact-buffer SHA/direct parse. |
| Publication blocked correctly? | Last-only diagnostic naming git parser dependency. |
| Package scope preserved? | One old task per 05-04/05/06 with unchanged acceptance. |

## Outcome boundary

05-02 evaluates authority, 05-03 alone closes FAB-03 on PASS, 05-04 closes FAB-04/05 foundations, 05-05 closes FAB-06 integration, and 05-06 closes FAB-08/Phase 5. None authorizes crates.io publication, release/product approval, ODB++/IPC-2581, or erasure of historical STOP and unavailable-corpus evidence.
