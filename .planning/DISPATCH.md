# First-Wave Paseo Dispatch Brief

## Safety Gate

All first-wave lanes are **read-only spikes** against `/Users/mattiafiumara/repos/ratemypcb/ratemypcb`. They may inspect files, run read-only queries/tests, and return findings to the orchestrator, but must not create/edit/delete files, stage, commit, reset, stash, clean, switch branches, create worktrees, push, merge, or contact provider APIs.

Implementation worktrees must wait. The only tracked base is HEAD `071c591`, while the current baseline contains a large uncommitted 0.2 product change set across core, CLI, viewer, schemas, skill, tests, Cargo files, README, and CI. A Git worktree created from HEAD cannot inherit those uncommitted changes; reconstructing them by patch/copy would be unsafe and would violate the preserve-exactly authority. Until the 0.2 baseline has a user-approved safe tracked commit or another explicit snapshot mechanism, implementation must remain single-writer in this checkout.

## Lane A — Report Contract and Evaluation Spike

**Mode:** Read-only, independent.  
**Focus:** Phase 1/2 report truth and measurable comprehension.

Read:

- `.planning/phases/01-decision-first-evidence-contract/{01-CONTEXT.md,01-RESEARCH.md,01-01-PLAN.md,01-02-PLAN.md,01-03-PLAN.md}`
- `crates/ratemypcb-core/src/lib.rs` report/evidence/gate/assessment/schema seams
- `crates/ratemypcb-cli/src/{main.rs,viewer.rs}`
- `crates/ratemypcb-cli/assets/local-viewer.{html,css,js}`
- current schemas, skill report contract, and viewer tests

Return:

1. A field-by-field compatibility review for report/assessment 2.0, identifying accidental ambiguity or viewer-owned policy.
2. A minimal golden/mutation matrix proving disposition uniqueness, fail-closed unknowns, stable IDs, provenance, reference integrity, and ≤3-action information load.
3. A concrete dependency-free evaluation protocol for 10-second disposition/action comprehension, unknown-state interpretation, deep-link integrity, and accessibility handoff.
4. Any exact current path/symbol corrections needed in Phase 1 plans.

Do not redesign the UI, write fixtures, or edit plans.

## Lane B — Supply Provider and Legal Contract Spike

**Mode:** Read-only, independent; no authenticated calls and no provider-native subagents.  
**Focus:** Phase 3 go/no-go evidence for official providers and durable report policy.

Read:

- `.planning/research/{STACK.md,FEATURES.md,ARCHITECTURE.md,PITFALLS.md,SUMMARY.md}`
- `skills/review-pcb-dfm/scripts/enrich_bom.py`
- `skills/review-pcb-dfm/references/supply-snapshots.md`
- `crates/ratemypcb-core/src/lib.rs` `supply_review`, `supply_price`, `supply_alternates`, `enrich_bom_lines`
- Official URLs already cited in the supplied supply research

Return:

1. A provider-by-provider matrix for Nexar, Mouser, DigiKey, and LCSC: official endpoint/program, observed published fields, credentials/quota unknowns, authorization semantics, and explicit query/cache/embed/share/retention gates.
2. Snapshot v2 contract edge cases and which fields must remain `not checked` without authenticated account verification.
3. A no-scrape adapter sequence and test-fixture acquisition/sanitization plan.
4. Exact questions requiring written provider/legal decisions before any direct adapter ships.

Do not call APIs, use credentials, infer permissions, save provider payloads, or approve alternates.

## Lane C — Fabrication Formats and Schematic Architecture Spike

**Mode:** Read-only, independent.  
**Focus:** Phase 4-6 architecture and decision gates.

Read:

- `.planning/research/{STACK.md,FEATURES.md,ARCHITECTURE.md,PITFALLS.md,SUMMARY.md}`
- `crates/ratemypcb-core/src/lib.rs` classification/loading/native DRC/manufacturing/review seams
- `crates/ratemypcb-core/src/stackup.rs`
- current Gerber viewer parser/tests
- Official KiCad, Ucamco, ODB++, and IPC-2581 URLs cited in supplied research

Return:

1. A canonical-capability matrix mapping native KiCad, Gerber/X2+Excellon, ODB++, and IPC-2581 to geometry, layers, drills, connectivity, assembly, construction, provenance, omissions, and reconciliation.
2. A native-first KiCad schematic/ERC/parity execution and fixture matrix covering hierarchy/reused sheets/occurrences/tool failures and bounded Altium/netlist claims.
3. Equivalent ODB++ and IPC-2581 legal/corpus/conformance/security/performance/maintenance spike exit criteria, including explicit no-go outcomes.
4. Parser threat boundaries and corpus gaps that must be resolved before dependency/adoption checkpoints.

Do not download gated corpora, accept licenses, install parsers, create archives, or recommend removing Gerber baseline support.

## Fan-In Rule

The orchestrator compares all three read-only results against `.planning/REQUIREMENTS.md` and `.planning/ROADMAP.md`. Findings may refine later phase research/CONTEXT/PLAN artifacts only in the single-writer checkout. No implementation dispatch occurs until the dirty-baseline safety gate is explicitly resolved by the user.
