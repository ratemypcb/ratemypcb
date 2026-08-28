# Phase 4 Context

## Goal

Add native-first, hierarchy-aware KiCad schematic/ERC/parity evidence and bounded cross-artifact consistency without claiming semantics that did not run.

## Locked decisions

- `kicad-cli` is authoritative for ERC and PCB↔schematic parity; source parsing is limited to inventory, hierarchy identity, and explicit source facts absent from native output.
- Supported native majors are exactly 8, 9, and 10. Unknown/future majors are `not_run`; source format dates never establish tool compatibility.
- Native exit 0 and 5 both mean analysis completed. Any other exit, timeout, missing/oversized/malformed JSON, or unavailable tool is `not_run` in auto mode and an error in required mode.
- ERC sheets and DRC ordinary, unconnected, and schematic-parity channels remain distinct. Excluded markers are retained; absence of an `excluded` field remains unknown.
- Schematic occurrences are identified by project/root context plus UUID occurrence path and item UUID, never by reference alone. Reused child sheets therefore remain distinct.
- Automatic root selection uses selected board/project basename and hierarchy references. Ambiguity is explicit; `--schematic` is the bounded override.
- Native parity requires a coherent native board, schematic root, and project context. ZIPs are inventoried but native checks do not run because source trees are not staged.
- Deterministic schematic families remain evidence-only/non-blocking until the released-major fixture corpus is adjudicated. Heuristics are not added.
- `.SchDoc` is inventory-only. Generic netlists retain only explicit export fields and never imply native ERC, hierarchy, power intent, DNP state, or parity.

## Evidence boundary

KiCad 10.0.5 is locally executable. KiCad 8 and 9 command/schema compatibility is documented and represented by sanitized schema-shaped fixtures, but is not described as locally executed. No Altium automation, licensed corpus, credential, download, or package installation is authorized or needed.

## Phase continuity

The Phase 1 approval/evidence contract and Phase 3 supply semantics remain authoritative. Missing schematic evidence may close approval only if a later checkpoint promotes it into required coverage; this phase does not silently promote unadjudicated families.
