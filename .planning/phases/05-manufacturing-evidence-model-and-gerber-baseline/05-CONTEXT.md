# Phase 5 Context

## Goal

Replace filename/token fabrication screening with bounded, geometry-backed Gerber/X2+Excellon evidence in one provenance-aware manufacturing model, then reconcile the release package with native KiCad source without claiming semantics that were not supplied.

## Phase boundary

Phase 5 owns the canonical manufacturing model, production Gerber/X2 and XNC/Excellon baseline adapters, Gerber Job/package completeness, native-KiCad/package reconciliation, and their report/CLI/viewer/skill evidence surfaces. It establishes capability contracts needed by later analyzers but does not add calibrated Phase 7 DFM policy.

Gerber/X2+Excellon remains the supported fabrication baseline. ODB++ and IPC-2581 product adapters belong to Phase 6. Release publication and final release hardening belong to Phase 8.

<decisions>
## Decisions

- **D-01:** One canonical model — Add one focused `fabrication.rs` model. Gerber, XNC, Gerber Job, package, and native KiCad adapters emit canonical facts plus capabilities, omissions, warnings, conflicts, and provenance. They do not emit approval decisions.
- **D-02:** Adapter/analyzer separation — Analyzers name exact capability prerequisites. Any missing, partial, stale, failed, unsupported, or omitted prerequisite yields `not_checked`/non-pass evidence. Format presence alone never satisfies semantic coverage.
- **D-03:** Fixed-point geometry — Canonical coordinates use checked signed integer picometres with explicit source unit and resolution. Decimal input is parsed without floating point. Inputs finer than the supported picometre contract, out-of-range coordinates, non-finite/overflowing transforms, and hidden quantization fail closed. Source transforms remain ordered fixed/rational operations; any materialization records deterministic rounding and its error bound.
- **D-04:** Stable identity — Artifact digests are SHA-256 over original bytes. Document, layer, tool, and feature IDs derive from versioned canonical identity fields and structural source locations, never prose, package ordering, local absolute paths, or parser warning text. Canonical model digests sort normalized records and exclude volatile diagnostics.
- **D-05:** Explicit parser completion — Syntax acceptance, semantic completion, package completeness, and approval capability are separate. No parser error or unsupported semantic record may be silently discarded. A successful parse with a missing capability remains partial/not checked.
- **D-06:** Conditional dependency — Stage exactly `gerber_parser = "=0.5.0"` in core `[dev-dependencies]` so a bounded spike harness and its candidate lock graph compile under `--locked` without any production import. This dev/lock inclusion is not adoption. Promote the exact candidate to normal workspace/core production dependencies only after dependency, license, error-accounting, route-attribute, resource, hostile-input, local-corpus evidence, and a `blocking-human` PASS checkpoint. STOP removes the spike dependency/candidate-only lock entries and triggers replanning without a fallback. Every `GerberDoc::errors()` item is inspected before command consumption. `gerberx2` is not a production or Cargo dependency; it may be used only as a local differential oracle.
- **D-07:** Byte normalization boundary — Original manufacturing bytes and digest are preserved. Invalid bytes may be normalized only inside a lexically complete, non-semantic `G04` comment, never in extended commands, attributes, syntax, or legacy `G04 #@!` semantic attributes. Every replacement records bounded byte spans/counts; any ambiguity rejects the file.
- **D-08:** Route FileFunction gap — The valid `%TF.FileFunction,NonPlated,1,4,NPTH,Route*%` record is parsed and preserved through an upstream fix or one exact local semantic adapter. It is never ignored, generalized, or used to suppress another parser error.
- **D-09:** X2/Job authority — X2 and Gerber Job roles outrank filename inference, but disagreement is a conflict rather than overwrite. Net/component/pin capability is complete only when every eligible object is explicitly covered and internally consistent. Attribute presence alone is partial.
- **D-10:** Strict XNC — Implement a bounded stdlib XNC state machine. Strict XNC is the default. KiCad and LibrePCB legacy allowances are separate named dialect profiles selected only by exact tested signatures and command allowlists; unknown commands, ambiguous decimal placement, mixed plating, or missing semantic state fail closed.
- **D-11:** Honest screening retirement — Current `gerber_syntax_valid`, filename-based package completion/mask checks, token-only Excellon checks, `Stackup::from_gerbers`, and browser `parseGerber`/`inspectGerberSet` cannot provide approval evidence. Until production adapters complete, their results are visibly `partial`, filename-inferred, and non-approval evidence.
- **D-12:** Corpus/legal boundary — Ucamco PDFs, schemas, ZIPs, and ambiguous third-party assets are local-only checkpoint inputs. Repository fixtures are project-authored, sanitized, digest-manifested, and redistribution-safe. The advertised 2026 Gerber layer ZIP returned one byte `0` on 2026-08-27 and is recorded as unavailable, not assumed present.
- **D-13:** Reconciliation — Package layer/profile/drill/extent/connectivity claims are compared with native KiCad facts only when both sides declare the prerequisite complete. Missing or incompatible evidence closes fabrication approval and links both sources; facts are never silently merged.
- **D-14:** Existing truth contract — Phase 1 risk/coverage/confidence/freshness/approval separation remains authoritative. Unsupported, omitted, partial, or stale parser capability cannot lower observed risk or improve approval.
- **D-15:** Proof-less geometry is conservative — `FabricationReview` does not retain parser bytes, so block membership and mutable source ranges never authorize definition-only exclusion. Small definition sets contribute their geometry directly; large sets use the full coordinate-contract extent. Precision may return only after validation-time byte-anchored reparsing.
- **D-16:** One carried manufacturing deadline — The top-level manufacturing operation creates one absolute deadline. Inventory validation, semantic and legacy fallback, native/XNC scans, canonical JSON parsing/equality, hashing, refresh, reconciliation, and final validation receive that same deadline; convenience wrappers exist only at true public boundaries.
- **D-17:** Product-focused review — By explicit human direction, Phase 5 has no custom parent packet, working-tree freeze, detached GPG authority, canonical review JSON, or cryptographic zero-findings protocol. Ordinary product tests plus one bounded independent code review accepted Phase 5.

</decisions>

## Resource contract

These are product limits, not test-only values. Adapters share the existing archive limits and add manufacturing-specific limits.

| Resource | Limit |
| --- | --- |
| Recognized manufacturing files | 256 per review |
| Raw manufacturing bytes | 4 MiB per file; 20 MiB aggregate |
| Lines / commands / records | 400,000 per file; 1,000,000 aggregate |
| Lexical tokens | 1,000,000 per file; 2,000,000 aggregate |
| Command or line text | 16 KiB |
| Attribute/comment/name text | 4 KiB each; 64 KiB aggregate metadata per file |
| Numeric token | 64 bytes; at most 9 decimal places; checked coordinate extent ±10 m |
| Nesting | 32 expression/JSON levels; 16 aperture-block/macro-call levels |
| Apertures/macros/tools | 10,000 apertures, 1,024 macros, 1,024 macro variables, 4,096 operations per macro; XNC strict tools T01..T99 |
| Geometry | 1,000,000 canonical features, 1,000,000 contour vertices, 100,000 drill/route features aggregate |
| Step-repeat | each factor 1..=1,000; checked product and total expanded-feature budget |
| Output model | 256 MiB estimated canonical allocation |
| Time | 5 s per file; 30 s aggregate, checked during lexing, interpretation, and expansion |

Existing archive limits remain 90 MiB compressed, 256 MiB expanded, 2,000 entries, 512-byte normalized paths, and 12 directory levels. A limit breach is a typed failed/partial result and never a truncated success.

## Phase continuity

- Preserve Phase 2 accessibility/usability deferrals.
- Preserve KiCad 8/9 documentation-attested versus KiCad 10 locally executed distinctions and the closed schematic promotion checkpoint.
- Preserve provider legal/account deferrals and all Phase 3 retention restrictions.
- Preserve source-aware Altium and broader EDA deferrals.
- Do not install, publish, release, or add a hosted path.

## Honest deferrals

- ODB++ and IPC-2581 legal/corpus/adoption decisions — Phase 6.
- Calibrated clearance, annular-ring, copper-edge, mask/paste, return-path, thermal, creepage, and assembly policy — Phase 7.
- Broad fuzzing, performance certification, final privacy/version alignment, release-candidate review, publication, and skill adoption — Phase 8.
- Phase 2 human accessibility, cross-browser, and comprehension verification remains open.
