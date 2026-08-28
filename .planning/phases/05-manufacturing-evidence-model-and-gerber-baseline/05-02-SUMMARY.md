---
phase: 05-manufacturing-evidence-model-and-gerber-baseline
plan: 05-02
status: complete
decision: PASS_F416
requirements_completed: []
completed: 2026-08-27
---

# Plan 05-02 Summary

The historical crates.io `gerber_parser = "=0.5.0"` STOP remains unchanged. The human separately approved a RateMyPCB fork branch, signed commit, protected non-force push, upstream PR, and then production adoption of the exact reviewed fork candidate.

## Immutable candidate

- Fork: `https://github.com/ratemypcb/gerber-parser.git`
- Ref: `refs/heads/ratemypcb/gerber-parser-accounting-fix`
- Release base: `8a07cc6064894cbf63978012969af5c1f656a30b`
- Tokenizer foundation: `f4160c7c6ca1b4cdd9c5273a3916b4fd087b5e34`
- Signed candidate: `54004bc52c11699b49cd287a49135380feee86b3`
- Tree: `5a8bddf91cd77b7e6700df0eb1027a4fc231c9a6`
- Candidate packet SHA-256: `e01d72572ed23f461a48e85433217e0dbc030345424fc3e1c1fccdb803da76e3`
- Independent review SHA-256: `2cb8e0b50f2a372eb981db6f4f62eadfab08e582cc05c397dd1748a10e448772`
- Upstream PR: [MakerPnP/gerber-parser#26](https://github.com/MakerPnP/gerber-parser/pull/26), open and unmerged

The protected branch is locked, requires signed commits and linear history, disallows force pushes/deletion, and enforces administrators. RateMyPCB currently consumes the full SHA only as a dev dependency; no production import or FAB-03 claim was added by this plan.

## Verification

- Fork: 57 tests passed; formatting passed; Clippy passed with only the exact pre-existing `blocks_in_conditions` test lint allowed.
- RateMyPCB: 145 Rust tests and 29 Node tests passed.
- Focused adoption spike: 7 tests passed.
- Official local corpus: 32 checksum-verified direct-ZIP Gerbers, 102,909 parser records, one exact Route resolution, zero unaccounted errors, 32 bounded ordinary-comment normalizations.
- Authority verifier and 11-case fail-closed mutation matrix passed.
- Independent review: **ACCEPT**, empty P0/P1/P2. The remaining parser-buffer P3 is bounded by the required caller-side production byte/resource contract; the governance-verifier P3 was hardened before this decision.

## Decision

The human response `pass` is recorded verbatim and normalized to the plan token `PASS_F416`. In this plan, `PASS_F416` binds only exact candidate head `54004bc52c11699b49cd287a49135380feee86b3`; it does not adopt bare f416 or authorize another SHA, PR merge, release, or publication.

FAB-03 remains incomplete here. Plan 05-03 must reverify the decision and remote/Cargo identity before promotion or production-source edits.
