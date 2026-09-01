# Phase 7 Promotion Checkpoint

## Phase 6 dependency

**Status:** RESOLVED — no-go for both ODB++ and IPC-2581 this release.

Exact external receipts `06-01-SUMMARY.md` and `06-08-SUMMARY.md` record FMT-03/FMT-05 complete, FMT-04 not applicable, and Phase 6 complete at 8/8. Phase 7 uses the accepted native KiCad plus Gerber/X2+Excellon baseline only; no ODB++/IPC-2581 adapter, parity fixture, private parser/corpus input, or support claim is authorized. Plan 07-03 is unblocked.

This resolves only the Phase 6 dependency. Required coverage, Blocking findings, and inference promotion still require their own qualification and human gates.

## Inference-family promotion

**Status:** CLOSED
**Approved inference families:** None

A human may approve one named family/version only after reviewing:

- exact capability prerequisites and fail-closed `not_checked` behavior;
- adjudicated positive, hard-negative, and mutation cases;
- TP, FP, FN, TN, precision, and recall;
- non-undefined precision meeting the blocking policy and all prerequisite mutations remaining non-blocking;
- source-linked assumptions and residual false-positive/false-negative risk;
- an independent plan/code review finding no unresolved blocker.

The decision is plain Markdown: family ID/version, approve or decline, reviewer, date, rationale, and any limits. No custom token, hash, signature, or blanket promotion is required or accepted. Removing or omitting approval must downgrade the family to `EvidenceOnly`; it must never create a pass.
