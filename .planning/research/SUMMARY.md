# Research Summary

## Conclusion

RateMyPCB should not solve its “meh” output with more prose or a new format badge. The root problem is uneven evidence hidden behind a score-first report. Preserve the current deterministic-report/digest-bound-assessment seam, make the decision surface measurable, deepen evidence through native KiCad and real manufacturing parsers, and keep every unknown explicit.

**Overall confidence:** High in the roadmap direction and current-code diagnosis; medium for live provider integrations and intelligent-format adoption until legal/corpus/feasibility checkpoints complete.

## Recommended Product Shape

1. A versioned evidence/report contract separates risk, coverage, evidence confidence, freshness, and approval.
2. A decision-first HTML summary renders disposition, why, ≤3 evidence-linked actions, and required evidence completeness before scores.
3. A typed supply snapshot preserves exact manufacturer+MPN identity and per-provider/per-offer observations; terms policy governs collection and persistence.
4. KiCad schematic inspection is native-first: coherent project context, ERC, exports, parity, then bounded source facts.
5. One canonical provenance-aware manufacturing model accepts adapters; analyzers declare required capabilities.
6. Gerber/X2+Excellon becomes a real parsed baseline. ODB++ and IPC-2581 face the same legal/corpus/conformance/security/maintenance gate.
7. Advanced analyzers ship only with calibrated goldens, explicit inference labels, fuzz/security budgets, and release adoption evidence.

## Roadmap Implications

| Phase | Outcome | Research flag |
| ------- | --------- | --------------- |
| 1. Decision-First Evidence Contract | Measurable report truth and end-to-end tracer | Research complete enough to execute |
| 2. Report UX and Golden Corpus | One-screen comprehension, deep links, BOM matrix, accessibility | UI contract/checkpoint |
| 3. Supply Snapshot v2 | Demand-aware exact identity and legal provider observations | Legal/provider checkpoint before direct adapters |
| 4. KiCad Schematic Release Evidence | Native ERC/hierarchy/parity and artifact consistency | KiCad version/corpus research |
| 5. Manufacturing Model and Gerber Baseline | Canonical model plus real Gerber/X2+Excellon | Parser legitimacy/corpus checkpoint |
| 6. Intelligent Interchange Gate | Evidence-based ODB++ vs IPC-2581 choice and qualified adapter or no-go | Mandatory one-way/costly decision checkpoint |
| 7. Decision-Grade DFM and Assembly | Capability-gated high-value analyzers | Calibration checkpoint before blocker promotion |
| 8. Hardened Release and Skill Adoption | Fuzz/security/performance, docs/skill, release evidence | Release gate |

## Open Decision Gates

- **Supply permission:** for Nexar, DigiKey, Mouser, and LCSC, establish account/API fields and written intended-use policy for local caching, HTML embedding, sharing, and retention. No permission means no adapter behavior beyond `not checked`.
- **Gerber parser dependency:** verify package legitimacy, official Ucamco corpus behavior, parse completeness, and bounded resource use.
- **ODB++:** partner license, varied legal corpus, semantic conformance, virtual-archive security, and maintenance capacity.
- **IPC-2581:** standard/XSD redistribution rights, crate maturity/completeness, corpus behavior, unknown XML handling, and memory limits.
- **KiCad support matrix:** normalize stable IDs only after fixture runs on supported released majors.
- **Analyzer policy:** deterministic families need adjudicated precision before release-blocking status; heuristics stay non-blocking unless evidence earns promotion.

## What Remains `not checked`

Any missing/ambiguous exact identity; named distributor not successfully queried; unknown authorization/stock/lead/MOQ/order multiple/price applicability; stale or legally non-displayable observations; alternate equivalence without explicit approval; unsupported native ERC/parity; incomplete parser capability; and claims not retained by exported Altium/netlist artifacts.

## Primary Source Index

- KiCad CLI: <https://docs.kicad.org/9.0/en/cli/cli.html>
- KiCad schematic format: <https://dev-docs.kicad.org/en/file-formats/sexpr-schematic/>
- Ucamco Gerber: <https://www.ucamco.com/en/gerber/downloads>
- ODB++ terms/spec resources: <https://odbplusplus.com/design/partner-terms-of-use/>
- IPC-2581 samples: <https://www.ipc2581.com/ipc-2581-revc-test-cases/>
- Nexar terms: <https://nexar.com/api/legal>
- DigiKey agreement: <https://developer.digikey.com/api-user-agreement>
- Mouser terms: <https://www.mouser.com/en/apiterms/>
- LCSC terms: <https://www.lcsc.com/docs/>
- WCAG 2.2: <https://www.w3.org/TR/WCAG22/>
