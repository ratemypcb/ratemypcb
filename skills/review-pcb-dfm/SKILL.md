---
name: review-pcb-dfm
description: Run an evidence-backed PCB manufacturing and assembly review with RateMyPCB, then produce a reasoned engineering rating and self-contained HTML report. Use for KiCad `.kicad_pcb` projects, fabrication ZIPs, Gerber/drill packages, BOMs, placement files, supply-chain checks, BoardRepo projects, DFM readiness, or PCB release gates. Recognize Altium artifacts but do not claim source-aware Altium DRC.
---

# Review PCB DFM

Use RateMyPCB as the deterministic evidence extractor. Apply engineering judgment in a separate assessment; never rewrite deterministic findings or claim checks marked `not_run`/`not_provided`.

## Prepare the review

1. Resolve `ratemypcb` from an explicit user path, the current repository's `target/release`, or `PATH`.
2. Run `ratemypcb doctor --json`. Require report schema 2.0 and assessment schema 2.0. If stale, build the current checkout with `cargo build --release --locked`; otherwise provide the exact update command. Never silently use an older binary or download without consent.
3. Inventory the complete project, Gerbers, drill files, BOM, and placement data without modifying them. Prefer the project or repository directory over a lone `.kicad_pcb`; retain `.kicad_pro`, `.kicad_dru`, library tables, custom libraries, variables, and the README/release revision. When BoardRepo MCP supplied the project, record repository identity, ranking basis, saves/popularity, and acquisition time, then acquire the matching repository/release files when BoardRepo omits project context or fabrication archives.
4. Use `full` scope unless the user explicitly requests `design`, `fabrication`, or `assembly`. For fabrication or broader, select `eurocircuits`, `aisler`, `jlcpcb`, or `pcbway`; ask when the target is unknown. Never apply a 2-layer profile to a known multilayer board as a generic producibility test.
5. If several boards or BOM revisions are plausible, ask. If automatic KiCad schematic-root selection alone is ambiguous, pass `--schematic PROJECT_RELATIVE_ROOT.kicad_sch`; it cannot select a child/non-project/external path or enable ZIP native checks. Never use a repository-wide BOM for one sub-board when a board/revision BOM exists. Use exported CSV/TSV for Altium `.BomDoc` data.

## Extract evidence

Run one review and preserve its JSON:

```sh
ratemypcb review COMPLETE_PROJECT_OR_PACKAGE \
  --board BOARD.kicad_pcb --schematic ROOT.kicad_sch \
  --scope full --profile FABRICATOR \
  --bom BOM.csv --placement positions.csv \
  --supply-snapshot supply.json \
  --native auto --format json --output report.json
```

Omit flags only when the artifact is genuinely unavailable. Read `references/supply-snapshots.md` before using supply evidence. `scripts/enrich_bom.py BOM.csv --output supply.json` creates an offline v2 request template with every named provider `not-checked`; it never performs provider calls. Do not use BoardRepo pricing or add credentials/provider payloads until that exact provider has written approval for query, retention, embedding, and sharing.

Omit `--board` or `--schematic` when automatic selection is unambiguous. Native KiCad checks support majors 8, 9, and 10; `.SchDoc` and generic netlist inputs never gain Altium/native semantics. Gerber/X2, the supported Gerber Job 2023.06 subset, strict XNC, and exact named KiCad/LibrePCB Excellon profiles are core evidence; official corpora remain local-only. Browser Gerber rendering is presentation-only, and ODB++/IPC-2581 remain unsupported. Native KiCad tool exits 0 and 5 both mean the analysis completed; the RateMyPCB CLI still returns its documented review exit. Treat CLI exits `0` and `1` as completed reports, `2` as invalid/ambiguous input, and `3` as required-native or other execution failure. Auto native `not_run` still returns a report. For CI, use the user's explicit `--fail-on` threshold; do not invent policy.

## Reason over the evidence

Read `references/report-contract.md`. Review every category:

- Design Integrity: source structure, rules, connectivity, native DRC, zones, return paths, native schematic ERC, coherent-project parity, bounded hierarchy identity, and exact-first reconciliation. Keep ERC, ordinary DRC, unconnected, and schematic-parity channels distinct.
- Fabrication: profile limits, outline, copper, drills, mask, paste, and core Gerber/X2+XNC evidence. Inspect `fabrication.capabilities`, omissions/conflicts, exact artifact/model digests and locations, `sourcePair`, and every reconciliation. A match is valid only when both prerequisites are complete; missing/partial/stale/unsupported/failed evidence is `not_checked`, and a mismatch retains both sources without a source-wins merge.
- BOM: judge every parsed line for reference/quantity correlation, fitted/DNP state, value, footprint, and exact manufacturer+MPN. Join only validated supply-v2 evidence. State `not checked` when lifecycle, named-provider, demand, commercial, or alternate-authority evidence is absent.
- Assembly: placement correlation, orientation risks, paste evidence, and assembly outputs.
- Supply Chain: build demand plus attrition/spares, lifecycle conflicts, independent Mouser/DigiKey/LCSC states, seller authorization, stock, MOQ, order multiple, packaging, region/currency applicability, lead time, and explicitly approved alternates.
- Evidence & Coverage: missing, stale, ambiguous, or unsupported checks.

Label conclusions as deterministic evidence, engineering inference, or unanswered question. Every verdict, category summary, action, and structured question must cite provenance-backed report evidence IDs. Join schematic occurrences by UUID path/item identity first; reference fallback is weaker and only valid when unique. Preserve exclusion `true`, `false`, and unknown; excluded-only or unknown-exclusion markers cannot be promoted into active findings. Treat schematic/ERC/parity/reconciliation families as `evidence_only`: they are neither required nor blocking until a separate approved corpus checkpoint changes the core registry. Treat risk, coverage, confidence, freshness, and approval as separate states; unknown, missing, failed, stale, unsupported, `not_run`, `not_provided`, and attention-required evidence cannot support approval or pass-like wording.

Do not describe a cheaper search result as an alternate without evidence of functional, electrical, package, temperature, qualification, and lifecycle compatibility. Price breaks may identify a cost-review candidate; only a declared and reviewed substitute may be called an alternate.

## Write the assessment

Get the exact report-byte digest with `ratemypcb digest report.json`, then write assessment schema 2.0. Preserve the report bytes after digesting them.

- Lead with one `disposition` (`approve`, `revise`, or `blocked`), then the bounded `verdict`, no more than three evidence-linked `actions`, required-evidence completeness/freshness, and only then the secondary rating and score.
- Choose an independent integer `rating` from 0–10; it need not equal the CLI score and cannot override disposition.
- Write `verdict` as a free-form action plus the dominant issue, at most 60 characters, with no markdown or trailing period.
- Use `approve` only when `approvalEligible` is true and every required check is complete, current, and passing. Otherwise fail closed with `revise` or `blocked`.
- When not approving, make the first action the shortest concrete step that unlocks the review or resolves the highest-impact risk.
- Keep questions structured as `{question, evidenceRefs}` and reference only stable public evidence IDs from the bound report.

The renderer leads with disposition and verdict and displays the assessment rating and deterministic score separately afterward.

## Render and deliver

```sh
ratemypcb render --report report.json --assessment assessment.json --output report.html
```

Open the resulting self-contained HTML when requested. Report the title, strongest action, missing coverage, native KiCad version/status, schematic root/project pair digests, exclusion state, evidence-only gate impact, selected fabricator profile, and manufacturing disclaimer. Never call a board DRC-clean, fab-ready, or safe to manufacture when the approval gate is closed.

The HTML includes a dedicated BOM tab. Inspect its release-impact sort, filters, and progressive row batches before delivery: every source BOM row must remain reachable, and unavailable live fields must say `not checked` rather than implying a pass. Confirm visible limitation IDs resolve to provenance details.
