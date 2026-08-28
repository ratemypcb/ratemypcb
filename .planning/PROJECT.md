# RateMyPCB Decision-Grade Release Review

## What This Is

RateMyPCB is a local-first Rust CLI and `review-pcb-dfm` skill that inspect PCB design and manufacturing artifacts, emit deterministic evidence, bind a separate engineering assessment to the evidence digest, and render a self-contained HTML review. This brownfield milestone turns the current dense preflight into a decision-grade release review that tells an engineer whether the board can be manufactured, why not, what to do next, and how complete and fresh the evidence is.

## Core Value

A release decision must be honest, actionable, and traceable: missing evidence blocks approval and is never mistaken for low risk or a pass.

## Requirements

### Validated — shipped HEAD `071c591` (0.1-era)

- ✓ Safe local directory/ZIP inventory with bounded reads and no cloud upload.
- ✓ KiCad board parsing plus optional native `kicad-cli` PCB DRC.
- ✓ Deterministic score, confidence, coverage, findings, limitations, and manufacturing disclaimer in report schema 1.0.
- ✓ Terminal/JSON review, schema output, CI threshold exits, and private loopback viewer.
- ✓ Bounded recognition of KiCad, Altium PCB, Gerber, Excellon, BOM-like, and placement artifacts without source-aware Altium claims.

### Active — current milestone

- [ ] Make release disposition, blockers, next actions, and evidence completeness/freshness understandable in the first screen.
- [ ] Establish stable provenance-aware evidence IDs and a measurable report/assessment contract in which risk, confidence, coverage, and approval are separate.
- [ ] Deliver demand-aware exact manufacturer+MPN supply risk with official-provider provenance, legal/retention controls, conflicts, freshness, and explicit `not checked`.
- [ ] Add KiCad-native schematic ERC, hierarchy evidence, and schematic↔PCB↔BOM↔placement consistency while bounding Altium/netlist claims.
- [ ] Deepen fabrication evidence through one canonical manufacturing model and real Gerber/X2+Excellon parsing.
- [ ] Evaluate ODB++ and IPC-2581 through legal, corpus, conformance, security, and maintenance gates before adopting either.
- [ ] Add high-value DFM/assembly analyzers, adversarial validation, performance bounds, and skill/release adoption evidence.

### Uncommitted 0.2 candidate — present but not validated or shipped

The dirty working tree adds review scopes/categories/gates, profile DRC, line-level BOM and supply joins, stackup, report schema 1.2, digest-bound assessment 1.0, `digest`/`profiles`/`render`, self-contained HTML, and a richer viewer. These are implementation inputs and hypotheses, not shipped capabilities. Planning must preserve and build on this exact working tree; isolated implementation worktrees cannot inherit it safely.

### Out of Scope

- Automatic approval of alternates or provider suggestions — only explicit engineering approval can qualify a substitute.
- Web scraping or storage/display uses not authorized by provider terms — unavailable permission yields `not checked`.
- Requiring ODB++ or removing Gerber support before the format checkpoint passes — Gerber/X2+Excellon remains the baseline.
- Claims of source-aware Altium ERC/DRC without licensed native automation — exported evidence is bounded by what it contains.
- Universal circuit-correctness or compliance certification — this remains evidence-backed preflight plus engineering judgment.
- A hosted upload/service architecture — local/offline operation and sensitive-design containment remain the default.

## Context

The current product's presentation moved faster than its evidence. The 0.2 viewer is polished but score-first; schematic handling is absent; fabrication checks mostly inspect filenames/tokens; supply evidence aggregates sellers and can blur identity, unknowns, and alternate provenance. The strongest existing seam is the immutable deterministic `Report` followed by an independently authored, digest-bound `Assessment`; the viewer should remain a consumer, not an analysis engine.

The implementation is intentionally monolithic today (`crates/ratemypcb-core/src/lib.rs`), with existing seams at `review`, `validate_assessment`, BOM/supply functions, `manufacturing_review`, CLI render/digest, and viewer snapshot/rendering. Prefer extracting plain modules only when evidence depth requires it; do not create a plugin framework.

## Constraints

- **Workspace**: Only this checkout; planning work may modify only `.planning/`.
- **Dirty baseline**: Preserve all uncommitted 0.2 changes exactly; no staging, commits, resets, stashes, cleaning, branch changes, worktrees, or destructive Git operations.
- **Local-first/security**: Untrusted archives, EDA files, provider responses, and self-contained reports need explicit size/path/allocation/privacy controls.
- **Evidence semantics**: `not checked` is first-class; absence and unknown differ from false and zero; missing required coverage closes approval without lowering risk.
- **Supply**: Exact identity is manufacturer+MPN; named-provider availability must be observed, not inferred; restricted live data is not durably embedded without permission.
- **EDA authority**: Prefer supported native `kicad-cli` ERC/DRC/parity before recreating KiCad semantics.
- **Manufacturing formats**: One canonical provenance-aware model; adapters declare supplied capabilities and analyzers declare required capabilities.
- **Compatibility**: Gerber/X2+Excellon remains supported; ODB++ and IPC-2581 are gated decisions.
- **Quality**: Deterministic goldens, mutation/adversarial fixtures, accessibility checks, fuzzing, and bounded performance are release evidence, not optional polish.
- **Workflow**: Fine-grained GSD planning, parallel planning enabled, `commit_docs=false`, no time estimates.

## Key Decisions

| Decision | Rationale | Outcome |
| ---------- | ----------- | --------- |
| Keep deterministic report and digest-bound assessment separate | Prevent judgment from rewriting measured evidence | ✓ Preserve |
| Put disposition/actions/coverage before scores | Users need a release decision, not a report-card puzzle | — Pending validation |
| Keep risk, confidence, coverage, freshness, and approval as separate concepts | Unknown evidence must not masquerade as low risk | ✓ Locked |
| Use stable check IDs plus deterministic occurrence IDs with provenance | Assessments and deep links depend on IDs surviving ordering/prose changes | — Pending implementation |
| Treat manufacturer+MPN as supply identity and provider alternates as unapproved candidates | MPN-only joins and suggestion promotion are unsafe | ✓ Locked |
| Use official provider APIs only under reviewed terms; otherwise `not checked` | Terms and retention are product requirements | ✓ Locked |
| Keep Gerber/X2+Excellon baseline and build one canonical manufacturing model | Interoperability without analyzer duplication | ✓ Locked |
| Gate ODB++ and IPC-2581 equally on legal/corpus/conformance/security/maintenance evidence | Format presence and marketing do not prove safe, complete evidence | — Open checkpoint |
| Use native KiCad ERC/DRC/parity first | Native tooling has the strongest supported semantics | ✓ Locked |
| Keep Altium/exported-netlist claims bounded to observed exports | Avoid false source-aware conclusions | ✓ Locked |

## Hypotheses to Validate

- A one-screen decision summary with at most three evidence-linked actions will let at least 90% of representative users identify disposition and first action within 10 seconds.
- Stable canonical occurrence identities can survive ordering and prose changes without hiding genuinely different findings.
- A typed supply snapshot can expose Mouser, DigiKey, and LCSC observations without violating provider-specific retention/display terms; each adapter remains gated until proven.
- Real Gerber/X2+Excellon parsing plus native KiCad reconciliation can provide strong fabrication evidence without requiring ODB++.
- IPC-2581 may be the lower-cost intelligent adapter, but legal/XSD redistribution, crate maturity, hostile XML behavior, and corpus coverage remain unproven.
- ODB++ may provide richer evidence, but partner licensing, corpus scarcity, parser/security complexity, and maintenance may make adoption a no-go.

---
*Last updated: 2026-08-26 after initial brownfield milestone planning*
