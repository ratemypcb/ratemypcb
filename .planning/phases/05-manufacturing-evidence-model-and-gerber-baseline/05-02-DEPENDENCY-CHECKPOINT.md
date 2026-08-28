---
phase: 05-manufacturing-evidence-model-and-gerber-baseline
plan: 02
task: 05-02-01
status: stopped
checkpoint: 05-02-02
gate: blocking-human
candidate: 'gerber_parser = "=0.5.0"'
recommendation: STOP
decision: STOP
decided_by: human
decided: 2026-08-27
updated: 2026-08-27
---

# Gerber parser dependency checkpoint

## Decision recorded

The human owner selected **STOP** on 2026-08-27 for production promotion of exactly `gerber_parser = "=0.5.0"`. The exact crates.io dev pin and its candidate-only lock entries were removed before replanning. No production dependency, adapter, byte boundary, interpreter, or Gerber capability was added by the stopped candidate.

The authorized follow-up is a new plan for a minimally changed RateMyPCB GitHub fork pinned by immutable commit, with a fresh dependency packet and blocking-human adoption decision. STOP does not pre-approve that fork, restore legacy screening authority, or select ODB++/another parser.

## Manifest and production isolation

The complete core manifest addition is:

```toml
[dev-dependencies]
gerber_parser = "=0.5.0"
```

- Root/workspace `Cargo.toml`: unchanged by this task and contains no `gerber_parser`.
- Core normal/build dependencies: unchanged and contain no `gerber_parser`.
- Core production `src/`: no `gerber_parser` or `gerberx2` import/reference.
- `Cargo.lock`: contains `gerber_parser` 0.5.0 with crates.io checksum `73f675010c0dda2e38b3f276c45d96e743758929d4e6ae81052dcde35daf8dc1`; contains no `gerberx2`, `pcb-ir`, `resvg`, `usvg`, `fontdb`, or image/rendering stack.
- `cargo tree --locked -p ratemypcb-core -e normal,build` shows only the existing serde/JSON/SHA-256/ZIP production graph.
- `cargo tree --locked -p ratemypcb-core -e dev` shows one dev edge: `ratemypcb-core -> gerber_parser v0.5.0`.
- `cargo check -p ratemypcb-core --lib --locked` and the integration spike compile separately under the candidate lock graph.

## Exact candidate graph and features

Cargo reported 46 packages newly locked when the dev pin entered this working tree. `cargo metadata --locked` traversal from `gerber_parser@0.5.0` across all target conditions reaches these 56 exact registry packages:

```text
aho-corasick@1.1.5, android_system_properties@0.1.6, anyhow@1.0.104,
autocfg@1.5.1, bumpalo@3.20.3, cc@1.4.4, cfg-if@1.0.4,
chrono@0.4.45, core-foundation-sys@0.8.7, find-msvc-tools@0.1.11,
futures-core@0.3.34, futures-task@0.3.34, futures-util@0.3.34,
gerber-types@0.7.0, gerber_parser@0.5.0, heck@0.5.0,
iana-time-zone@0.1.65, iana-time-zone-haiku@0.1.2, js-sys@0.3.104,
lazy-regex@3.6.1, lazy-regex-proc_macros@3.6.1, libc@0.2.189,
log@0.4.33, memchr@2.8.3, num-bigint@0.4.8, num-integer@0.1.47,
num-rational@0.4.2, num-traits@0.2.19, once_cell@1.21.4,
pin-project-lite@0.2.17, proc-macro2@1.0.107, quote@1.0.47,
regex@1.13.1, regex-automata@0.4.18, regex-syntax@0.8.11,
rustversion@1.0.23, shlex@2.0.1, slab@0.4.12, strum@0.27.2,
strum_macros@0.27.2, syn@2.0.119, syn@3.0.3, thiserror@2.0.20,
thiserror-impl@2.0.20, unicode-ident@1.0.24, uuid@1.26.0,
wasm-bindgen@0.2.127, wasm-bindgen-macro@0.2.127,
wasm-bindgen-macro-support@0.2.127, wasm-bindgen-shared@0.2.127,
windows-core@0.62.2, windows-implement@0.60.2,
windows-interface@0.59.3, windows-link@0.2.1,
windows-result@0.4.1, windows-strings@0.5.1
```

Direct crate dependencies are `anyhow`, `gerber-types`, `lazy-regex`, `log`, `regex`, `strum`, and `thiserror`. Their resolved default features and transitive feature graph were inspected with `cargo tree --locked -p gerber_parser -e features`. The crate's optional `env_logger` feature is not enabled. No rendering, font, SVG, image, or geometry-overlay feature is present.

## License, ownership, publication, and maintenance snapshot

Evidence fetched/verified 2026-08-27:

- Crate license declaration: `MIT OR Apache-2.0`.
- Packaged `LICENSE-APACHE` SHA-256: `6d1d968fb225eca367cb7f0b8831ab012a35d92b547e945e17ef8e7b05c3e5cc`.
- Packaged `LICENSE-MIT` SHA-256: `67f08344165986d19e5742591c4527d137e6435d763b4bf6448fb85bd69d0042`.
- crates.io 0.5.0: published 2026-05-20 by Dominic Clifton (`hydra`), not yanked, 5,237 downloads at fetch, package checksum matches the lockfile.
- crates.io owners: `NemoAndrea` and `hydra`.
- Repository: public, non-archived `MakerPnP/gerber-parser`; created 2022-06-27, last pushed 2026-05-20, 16 stars, 7 forks, 1 open issue at fetch.
- The packaged `.cargo_vcs_info.json` identifies Git commit `8a07cc6064894cbf63978012969af5c1f656a30b` (`Release v0.5.0`). GitHub's tags endpoint listed versions through v0.4.0 but no v0.5.0 tag. The immutable crates.io package/checksum is therefore the reviewed identity; the missing matching Git tag remains a maintenance/provenance caveat.
- Upstream PR [MakerPnP/gerber-parser#25](https://github.com/MakerPnP/gerber-parser/pull/25), opened 2026-05-21 and still unmerged at review time, replaces the physical-line tokenizer with a `*`/`%`-aware tokenizer. Its report describes real ViewMate inputs producing approximately 597,000 errors and says it fixes a path where errors were silently dropped. The proposed API change is breaking.
- Primary metadata sources: crates.io API `/api/v1/crates/gerber_parser/0.5.0` and `/owners`; GitHub API repository, tags, commits, and open-issues endpoints.

Neither `cargo-audit` nor `cargo-deny` is installed, and none was installed for this task. A bounded OSV.dev `POST /v1/querybatch` checked all 56 exact registry package/version pairs at `2026-08-27T11:52Z`: 56 results, zero reported vulnerabilities. Request SHA-256: `116f5f511efd6ee9d20e297909e2ca2992ee907f892d42efc926844667216f97`; response SHA-256: `905b0961eacc263c072d34f8bea2d9bad6f51da7e40b0f0f2232702f70ed2995`. No design bytes or repository content was sent. This is a point-in-time OSV result, not a RustSec/cargo-audit attestation.

## Parser API and error-accounting evidence

`GerberDoc.commands` is `Vec<Result<Command, GerberParserErrorWithContext>>`. The crate's `commands()` and `into_commands()` helpers explicitly filter failed records. The spike therefore:

1. preflights original bytes and creates only a bounded parser copy;
2. calls `GerberDoc::errors()` and classifies every returned error;
3. rejects any fatal or unaccounted error;
4. only then directly counts successful `commands` records;
5. never calls either filtering helper as a success signal.

Project source regression checks enforce this ordering/absence. Parser acceptance is not geometry, semantic, package, or approval capability; production interpretation remains blocked behind PASS.

## Blocking newline-independent command-stream result

Gerber 2026.05 sections 3.3 and 3.5 define a file as a stream of word and extended commands delimited by `*` and `%`; physical newlines are insignificant grammar whitespace, not command boundaries. A project-authored mutation removed only CR/LF bytes from `simple-x2.gbr`, retaining the same nine ordered command delimiters and semantics.

- Newline-separated control: **9 parser records, 9 successful records, zero errors**.
- Same nine commands on one physical line: **2 parser records, 1 successful record, 1 `Missing M02` error**.
- Seven input commands disappear without seven corresponding parser errors. Inspecting `GerberDoc::errors()` therefore cannot provide complete record accounting for this valid layout.
- The bounded preflight intentionally does not insert synthetic newlines or replace the candidate tokenizer. Doing so would be a second generalized parser repair beyond the plan's approved ordinary-G04 byte handling and exact Route compatibility exception.

Regression: `gerber_adoption_spike_newline_insignificant_stream_exposes_silent_record_loss`. This reproduces the upstream PR #25 tokenizer defect against exact crate 0.5.0 and makes the overall recommendation fail closed to STOP even though the available official corpus is newline-separated and passes.

Smallest unblock: wait for an upstream release containing a reviewed `*`/`%`-aware tokenizer and repeat the complete dependency/corpus gate, or explicitly replan and review a RateMyPCB-owned bounded tokenizer strategy. Exact 0.5.0 cannot be promoted under the current plan.

## Byte boundary and hostile/resource results

The test-only preflight retains the original SHA-256 and enforces 4 MiB/file, at most 400,000 LF-delimited physical lines and `*` occurrences, 16 KiB per physical line, prohibited control bytes, a bounded preflight deadline, and one narrow physical-line terminal check before parser use. It is not a complete Gerber command-framing validator. Boundary and over-bound tests cover:

- exact 4 MiB byte input passes preflight; 4 MiB + 1 rejects;
- exact 16 KiB physical line passes; an over-bound physical line rejects;
- exactly 400,000 `*` occurrences in the tested physical-line layout pass; 400,002 reject;
- a nonzero preflight deadline passes; zero deterministically rejects before parser use;
- NUL/control bytes and a tested nonempty physical line lacking terminal `*` or `*%` reject; an unmatched opening `%` is left for parser/error accounting rather than claimed as preflight framing coverage;
- invalid bytes in `%TF`, `%TA`, `%TO`, image names, `G04 #@!` legacy semantic comments, and ambiguous/multiple-command comment lines reject;
- one CP-1252 `0x96` byte inside a complete ordinary human `G04` comment is replaced only in the parser copy, with original digest and exact `[start,end)` warning retained;
- repeated runs produce identical digest, warnings, counts, dispositions, and result.

These are spike safeguards only. The spike preflight's physical-line checks are not a production tokenizer and do not repair the candidate's newline-independent command-stream failure. The candidate itself has no application resource budgets; a future replanned or upstream-fixed candidate would require equivalent or stronger production checks during byte lexing, parser-record accounting, interpretation, macro/block handling, and repetition.

## Sanitized project-authored fixtures

`tests/fixtures/fabrication/gerber/manifest.json` declares every committed byte project-authored under `MIT OR Apache-2.0`; it explicitly excludes Ucamco/customer/provider/ambiguous third-party bytes. Hashes and executable expected counts are:

| Fixture | SHA-256 | Parser records | Raw parser errors | Resolution/result |
| --- | --- | ---: | ---: | --- |
| `simple-x2.gbr` | `d0bb714b7a5482f517db3025c90706e83854d246c1b14f59685205c618b56618` | 9 | 0 | accepted by spike |
| `route-file-function.gbr` | `d70f820ca17801d7f7822adbfccbe9cd280cbe2c898d2370e0de9eefec53cb27` | 9 | 1 | exact Route error resolved; six fields retained |
| `invalid-comment-bytes.gbr` | `88dc39dd905ed4849c0bcd8254fec8a8e7e5993f4fcff24b4550f494cd4d44a4` | 8 | 0 | accepted with one original-byte warning `[39,40)` |
| `unsupported-semantic.gbr` | `c759ee3480c5af7cb540ab759bcbac5cd990b5074be2d438e21fb8e60587c77b` | 5 | 1 | rejected as unaccounted unsupported FileFunction |

The exact compatibility record is `%TF.FileFunction,NonPlated,1,4,NPTH,Route*%`. The sole accepted parser failure must be `InvalidParameter("Route")` on that exact raw line. Retained fields are `["TF.FileFunction", "NonPlated", "1", "4", "NPTH", "Route"]`. A second, adjacent, duplicate, generalized, or unrelated error remains unaccounted and rejects the file.

The test-generated compact-stream mutation is also project-authored: it removes only CR/LF from `simple-x2.gbr` and commits no additional or third-party bytes. It is intentionally rejected because exact 0.5.0 accounts for only 2 of its 9 commands.

## Official local-only corpus checkpoint

Inputs stayed outside the repository and were not modified. The test hashes each archive into an in-memory byte buffer, opens and bounds that same verified buffer with `ZipArchive`, and parses the `.gbr` member bytes directly; no independently supplied extracted tree participates in the evidence:

| Archive | Verified SHA-256 | Gerber files |
| --- | --- | ---: |
| PCB fabrication test 1 | `16329fda234b7f3e95651c29e8f381f445ab00ca4872d4e40eb072122d1d7625` | 12 |
| PCB fabrication test 2 | `28ca6f3b42931d7312d3229de07350fedacea1a785e32670a21f06817db6b007` | 20 |

Command:

```sh
RATEMYPCB_UCAMCO_CORPUS=/private/tmp/ratemypcb-phase5-corpus \
  cargo test -p ratemypcb-core --test fabrication_release --locked \
  gerber_adoption_spike_official_local_checkpoint -- --nocapture
```

Corpus sub-result: **PASS**; the two checksum-verified in-memory ZIPs yielded exactly 12 and 20 bounded `.gbr` members, respectively, then those exact member bytes produced pre-resolution 31/32, 102,909 parser records, one exact Route parser error resolved, zero unaccounted errors, 32 ordinary-comment normalizations, and one retained Route record. Missing env input or either archive digest mismatch is an explicit STOP condition, not successful parser evidence. This newline-separated corpus result does not override the compact valid-stream blocker; the overall recommendation is STOP.

All normalization warnings are accounted:

- Test 1 `[35,36)` in each of 12 files: Copper L1-L4; Legend Bot/Top; NonPlated 1-4 NPTH Drill/Route; Plated 1-4 PTH Drill; Profile NP; Soldermask Bot/Top.
- Test 2 `[42,43)` in each of 20 files: Copper L1 Top, L2-L9 Inner, L10 Bottom; Legend Bot/Top; NonPlated 1-10 NPTH Drill; Plated 1-10 PTH, 1-2 Blind, 2-9 Buried, 9-10 Blind drills; Profile NP; Soldermask Bot/Top.

Each warning replaces one original CP-1252 `0x96` en dash only in an ordinary G04 parser-copy payload. The only parser error was line 7 of Test 1's NonPlated Route file: `InvalidParameter("Route")` for the exact compatibility record. No official byte is committed.

The advertised 2026 Gerber layer ZIP remains unavailable: its published endpoint returned one byte `0` on 2026-08-27. It was not redownloaded or treated as checked. The 2024.05-targeted candidate was evaluated against the available corpus under the 2026.05 specification context; this is not blanket conformance.

## Differential oracle

No differential oracle was run for this task. Historical readiness used external/untracked `gerberx2`, but it is not authoritative and is absent from this repository's manifests, lockfile, and source.

## Gates and unresolved obligations

Green task gates: exact dev-only manifest placement, locked library check, seven focused spike tests, direct parsing of 32 members from the checksum-verified ZIP buffers, normal/build and dev trees, feature tree, metadata, sanitized manifest hashes/counts, source-import scan, formatting, Clippy, 145 full locked Rust tests, 29 Node tests, schema equality, diff check, and empty staged index. The spike itself completed successfully by producing a fail-closed recommendation.

Blocking and residual evidence:

1. exact 0.5.0 silently loses seven of nine commands in the compact valid-stream regression, so complete parser-error accounting is impossible for the current candidate;
2. upstream PR #25's generalized tokenizer/error-context repair is unmerged and breaking, and the current plan authorizes no equivalent local patch;
3. the missing v0.5.0 Git tag, recent release age, no built-in parser budgets, point-in-time OSV coverage, and unavailable advertised 2026 corpus remain maintenance/evidence caveats;
4. local official corpus remains non-redistributable and cannot become an ordinary CI fixture;
5. fixed-point modal/geometry/X2 interpretation and production hostile-expansion work were correctly not started.

## Independent review

A fresh read-only review accepted this corrected packet with no P0/P1/P2 findings after verifying direct parsing from the checksum-verified ZIP buffers, the narrowly stated preflight coverage, the compact-stream STOP condition, exact Route handling, and dependency isolation. The review is supporting evidence only and does not decide checkpoint 05-02-02.

## Binary recommendation

**STOP** — do not promote exact `gerber_parser = "=0.5.0"`. Although the candidate passed the supplied newline-separated official corpus and exact Route compatibility checks, it silently drops records from a specification-valid newline-independent stream and therefore fails the no-silent-critical-record-loss adoption requirement. A human STOP selection triggers removal of the dev pin and candidate-only lock entries and replanning; no fallback parser or legacy authority is selected.

**Checkpoint 05-02-02 is resolved as human STOP. The exact crates.io candidate is not approved; work may continue only through the separately planned and reviewed fork candidate.**

## Separate fork candidate follow-up

The crates.io STOP above remains unchanged. Under the replanned fork gate, the human subsequently authorized a bound fork branch and upstream PR. The immutable dev-only candidate is:

- fork ref: `ratemypcb/gerber-parser-accounting-fix`;
- tokenizer foundation: `f4160c7c6ca1b4cdd9c5273a3916b4fd087b5e34`;
- signed repair commit: `54004bc52c11699b49cd287a49135380feee86b3`;
- tree: `5a8bddf91cd77b7e6700df0eb1027a4fc231c9a6`;
- branch: protected, locked, signed-only, linear, non-force, and non-delete;
- upstream PR: [MakerPnP/gerber-parser#26](https://github.com/MakerPnP/gerber-parser/pull/26), open and unmerged;
- independent verdict: **ACCEPT**, with no P0/P1/P2 findings and two nonblocking P3 notes.

The candidate passes 57 fork tests, 145 RateMyPCB Rust tests, 29 Node tests, ten fail-closed verifier mutations, and the exact 32-file local corpus with 102,909 records and zero unaccounted errors. It remains a dev-only Git dependency pending the separate human `PASS_F416` or `STOP_F416` production-adoption decision.
