# RateMyPCB

## Install the skill

```sh
npx skills add ratemypcb/ratemypcb --skill review-pcb-dfm
```

Then open your PCB repository in Claude Code, Codex, Cursor, or another
Agent Skills-compatible tool and ask:

> Review this PCB for DFM before I send it to manufacturing.

The skill runs the deterministic CLI, reasons over its evidence, and produces a
self-contained report titled with a concise engineering judgment such as
`5/10 — Revise BOM: obsolete MCU`. The CLI score remains visible separately.

```sh
ratemypcb review path/to/complete-project --board board.kicad_pcb \
  --schematic root.kicad_sch --scope full --profile aisler \
  --bom bom.csv --placement positions.csv --format json --output report.json
ratemypcb digest report.json
ratemypcb render --report report.json --assessment assessment.json --output report.html
```

Use `--schematic` only when automatic KiCad root selection is ambiguous; it must
name one bounded project-relative automatic root and cannot select external paths
or enable native checks for ZIP input. Run `ratemypcb doctor --json` to inspect the
detected KiCad version/major and the explicit PCB DRC, schematic ERC, coherent-
project parity, and format limitations. Native KiCad checks support majors 8, 9,
and 10; exits 0 and 5 are completed analyses.

KiCad 10 is locally exercised by this project. KiCad 8/9 compatibility is
documentation/schema-attested, not claimed as locally executed. Native Altium
`.SchDoc` and generic-netlist analysis is not supported: `.SchDoc` is inventory
only and recognized netlists preserve explicit export fields only. Missing or
unknown exclusion state remains visible and cannot become an active finding or
pass. Schematic hierarchy, ERC, parity, and reconciliation families remain
`evidence_only` and are neither required nor blocking pending a separately
approved promotion checkpoint. Live provider adapters are disabled pending provider-specific written
approval for query, retention, embedding, and sharing.

The HTML report has a separate bill-of-materials tab. Every parsed BOM line keeps
its exact manufacturer+MPN and demand inputs, then reports lifecycle, independent
Mouser/DigiKey/LCSC states, seller-scoped stock/MOQ/order-multiple/package evidence,
decimal-string applicable pricing, and alternate authority. Missing data is
`not checked`, never zero stock or an invented substitute. Reviewing a complete project preserves KiCad rules, variables,
library tables and local footprints for both native and fabricator-profile DRC.
