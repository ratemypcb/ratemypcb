# Phase 4 Research: KiCad Schematic Release Evidence

**Researched:** 2026-08-27  
**Confidence:** High for released CLI commands, exit codes, official JSON schemas, hierarchy identity, and current code seams; medium for exclusion completeness; intentionally bounded for Altium and generic netlists.

## Research Summary

Phase 4 should extend the existing `review()` pipeline rather than add a schematic command or parallel report. The smallest honest implementation is:

1. one internal, version-dispatched `kicad-cli` runner shared by PCB DRC, schematic ERC, parity, and native exports;
2. one focused `schematic.rs` module for bounded hierarchy/source facts, native-output normalization, and reconciliation;
3. additive report/schema/CLI/viewer integration through the Phase 1 evidence contract.

Native KiCad output is authoritative. Source parsing exists only for inventory, occurrence identity, and facts absent from ERC/export output. New schematic families remain evidence-only and non-blocking in this phase; neither missing schematic evidence nor a new schematic finding is silently promoted into the existing approval gate.

## Current-Code Findings

- `classify()` and `Loaded` do not recognize or retain `.kicad_sch`, `.SchDoc`, netlists, child-sheet sources, library tables, or schematic project context.
- `select_board()`/`load_path()` are the upstream seam. Root selection must be added beside them so ERC, parity, exports, reconciliation, digests, and provenance all use the same selected project.
- `BoardFacts`, BOM review, and placement review correlate normalized reference sets only. They cannot distinguish reused-sheet occurrences or compare UUID/path, value, footprint, fitted/DNP, pin-pad, net, quantity, or revision.
- Native execution already has `kicad_version`, `wait_with_timeout`, `NativeMode`, project staging, and required-mode exit handling. Reuse and narrow these seams instead of creating another process framework.
- `supported_kicad_version()` accepts every future major. Phase 4 must explicitly dispatch only 8, 9, and 10.
- Current PCB DRC passes `--refill-zones`; that option is documented only for KiCad 10. It is unnecessary for the Phase 4 non-mutating common command and must not be sent to 8/9.
- Current DRC parsing removes excluded markers and flattens `schematic_parity` into ordinary DRC. Both behaviors violate the Phase 4 evidence contract.
- Current auto-mode execution failures become `failed`; Phase 4 requires `not_run` with the diagnostic cause retained. Required mode continues to return `Error::Native`, which the CLI maps to exit 3.
- `finalize_evidence()` has the known native version available indirectly but serializes every `kicad-cli` producer as `unknown`. It also has no composite board/root digest for parity.
- Existing SHA-256 evidence identity over artifact digest, stable check ID, and structured location remains authoritative. Occurrence UUID path, source path, marker channel, and item UUIDs belong in `location`; prose and reference text alone do not.
- ZIP handling inventories sources without staging them. Native checks must remain `not_run` for ZIP input in this phase.

## Released KiCad CLI Contract

### Common commands

The documented common command surface for released majors 8, 9, and 10 is:

```sh
kicad-cli sch erc --format json --severity-all \
  --exit-code-violations --output erc.json design.kicad_sch

kicad-cli pcb drc --format json --severity-all \
  --schematic-parity --exit-code-violations \
  --output drc.json design.kicad_pcb

kicad-cli sch export bom --output bom.csv design.kicad_sch
kicad-cli sch export netlist --format kicadsexpr \
  --output design.net design.kicad_sch
kicad-cli pcb export pos --format csv --units mm --side both \
  --output positions.csv design.kicad_pcb
```

Do not use a shell. Pass fixed argument vectors. Do not enable KiCad 10 `--save-board`; omit `--refill-zones` from the shared 8/9/10 DRC path. Run from the intact project tree so same-name `.kicad_pro`, relative child sheets, libraries, and text variables remain discoverable.

### Exit and output semantics

- Exit `0`: analysis completed with no violations.
- Exit `5`: analysis completed with violations; parse the expected report.
- Any other exit: tool/input/output failure.
- A missing, oversized, malformed, truncated, wrong-kind, or version-inconsistent report is a tool failure even after exit 0/5.
- In `NativeMode::Auto`, every tool failure is serialized as `not_run` with bounded diagnostics.
- In `NativeMode::Required`, every tool failure returns `Error::Native` and CLI exit 3.
- A completed report with markers is attention evidence, not a scanner crash.

### Version policy

Parse `kicad-cli version`, dispatch explicitly with `matches!(major, 8 | 9 | 10)`, and validate the JSON report's `kicad_version` against the dispatched major. Major 7, major 11, an unparseable version, and future majors are unsupported/not-run. Board/schematic format dates may produce a limitation but never prove CLI compatibility.

The repository context records KiCad 10.0.5 as locally executable. KiCad 8/9 command compatibility is documentation-attested and represented by sanitized, official-schema-shaped fixtures; those fixtures must say they were not locally executed. Do not inflate that into a local execution claim.

## Official JSON Shapes

### ERC

Official `erc.v1.json` roots contain:

- `source`, `date`, and `kicad_version`;
- `sheets[]` with human `path`, occurrence `uuid_path`, and `violations[]`.

### DRC

Official `drc.v1.json` roots contain:

- `source`, `date`, `kicad_version`, and `coordinate_units`;
- distinct `violations`, `unconnected_items`, and `schematic_parity` arrays.

A marker contains `type`, `description`, `severity`, `items`, and optional `excluded`; DRC exclusions may have `comment`. Affected items carry UUID, description, and position. KiCad 9 adds optional `included_severities`; current/KiCad 10 schemas additionally allow optional `ignored_checks`.

Normalize only documented fields, tolerate additional fields within the bounded report, and retain raw producer strings. ERC sheet paths and UUID paths remain attached to every normalized marker. Never flatten parity into DRC or ERC.

## Exclusion Semantics

Official CLI documentation promises exclusion selectors, but official issues show ERC filtering/serialization defects in KiCad 8 prerelease and KiCad 9.0.6. Therefore:

- request all severities;
- preserve every marker, including excluded markers;
- model exclusion as `true`, `false`, or unknown (`Option<bool>` or an equivalent explicit enum);
- preserve comments and included-severity metadata when emitted;
- absence of `excluded` is unknown, never inferred false;
- derive active finding counts only from markers explicitly known not excluded or conservatively report an unknown active/exclusion count when the producer omitted state;
- do not claim exclusion completeness for 8/9, or for 10 without pinned execution evidence.

## Hierarchy and Root Selection

KiCad native instance identity is project/root context plus UUID path. Reused child sheets have distinct occurrence paths even when they reuse one child file and symbol UUID. The bounded occurrence key is:

```text
(project identity, root digest, sheet UUID path, item UUID)
```

Reference, unit, symbol UUID, source path, and human sheet path are attributes, not primary keys.

Automatic root selection must:

1. prefer a `.kicad_sch` coherently linked to the selected board and same-name `.kicad_pro`;
2. build the bounded child-sheet graph and exclude child-only candidates from root candidacy;
3. preserve per-project instance records and variables;
4. return typed ambiguity when multiple coherent roots remain;
5. accept `--schematic PATH` only as an explicit bounded override.

Before reconciliation, report missing child files, unresolved `${VAR}` paths, duplicate/broken instance paths, recursion/cycles, and ambiguous roots. Child paths must be normalized, relative, and confined to the selected project root. Symlinks and ZIP entries retain the existing safety boundaries; ZIP source is inventoried but not natively executed.

## Source and Export Fact Authority

Use native outputs first:

- ERC for electrical-rule markers and sheet occurrence paths;
- native schematic BOM for fitted/BOM population and symbol fields represented by that export;
- native `kicadsexpr` netlist for components, pins, nets, and connectivity represented by that export;
- native PCB position CSV for populated placements;
- PCB DRC `schematic_parity` for native board↔schematic parity.

Parse source only for facts missing from those outputs: item/symbol UUID, occurrence UUID path, unit, `in_bom`, `on_board`, DNP state, footprint assignment, explicit fields, pin electrical types, power symbols/flags, and source connectivity needed to identify an exported fact. Record producer/evidence class per fact; do not emit one blanket `source-structure` claim for all schematic semantics.

A bounded stdlib tokenizer/parser is preferable to a new parsing dependency here: the repository has no S-expression/XML parser, and only a constrained subset is required. It must enforce source bytes, token count/length, nesting depth, string length, occurrence count, child count, and cycle bounds and fail closed on truncation or unsupported syntax.

## Reconciliation Contract

Perform staged joins in this order:

1. schematic occurrence UUID path + item UUID;
2. board footprint path/UUID;
3. normalized reference only as an explicit weak fallback when unique on both sides.

Then compare, without filling unknown fields:

- reference/UUID identity;
- value and footprint;
- fitted, DNP, `in_bom`, and `on_board` state;
- symbol pin ↔ board pad mapping;
- schematic/export net ↔ board pad net;
- BOM grouped quantity and population;
- placement population;
- board/root/BOM/placement source digests and declared revision/project identity.

Every mismatch gets one stable check-family ID and an occurrence location containing the strongest available UUID path/item UUID plus source paths. A reference fallback is confidence-labeled weaker evidence and must never silently masquerade as an exact join.

## Gate Policy

Phase 4 does not promote schematic checks into required evidence or blocking approval. Native ERC/parity and exact reconciliation can carry their real severity and visible attention state, but need an explicit report-authoritative `evidence_only`/non-blocking gate disposition so existing medium/high finding logic does not accidentally close approval.

Promotion requires a later reviewed checkpoint with adjudicated positive, hard-negative, and mutation metrics per check family. Decoupling, interface, and power heuristics are not added in this phase. Power intent is preserved as source fact only.

## Bounded Non-KiCad Claims

- `.SchDoc`: inventory/capability evidence only. No native ERC, hierarchy, parity, DNP, or source-aware reconciliation claim.
- Recognized generic `.net`/XML exports: preserve only explicit components, nets, pins, and sheet-path metadata actually present.
- Unrecognized netlist syntax: inventory as unsupported/not-checked; do not guess.
- Altium converted to KiCad: any later KiCad result is a post-import KiCad check, not Altium-native parity.
- Source-aware Altium automation remains deferred to EDA2-01.

## Validation Architecture

1. **Native adapter unit tests**: exact 8/9/10 command vectors, 0/5 completion, other exits/not-run, report bounds, version mismatch, distinct channels, and tri-state exclusions.
2. **Core fixture integration**: hierarchical project inventory, reused occurrences, export/source facts, staged joins, mismatch matrix, bounded Altium/netlist states, ZIP not-run, and stable provenance.
3. **CLI tracer**: auto not-run report survives exact digest, assessment validation, and self-contained rendering; required-mode failure exits 3; `--schematic` resolves only explicit ambiguity.
4. **Schema/viewer/skill regressions**: generated schema equality, generic evidence deep links, marker visibility, no browser-side analysis, and capability wording alignment.

No package installation, live service, customer fixture, credential, or network access is needed.

## Threat Model Inputs

- **Process execution:** argument injection, shell expansion, unbounded stderr/output, timeout orphaning, source mutation, temporary-file collision, and stale output reuse.
- **Path/archive:** traversal, absolute/external child paths, symlink escape, case/Unicode duplicate paths, cycles, deep trees, ZIP staging, and project-context confusion.
- **Parser/resource:** oversized source/JSON/export, deep nesting, excessive tokens/occurrences/markers, malformed UTF-8, malformed/truncated JSON, and silent record truncation.
- **Semantic authority:** exit 5 misclassified as failure, unsupported major guessed compatible, parity run against an unrelated schematic, absent exclusion inferred false, reference-only reused-sheet conflation, and lossy export presented as native source truth.
- **Provenance:** unknown native version, wrong artifact digest, prose-based occurrence IDs, or board-only parity identity.
- **Presentation:** viewer policy recomputation, broken evidence refs, unsafe text insertion, and unsupported capability wording.
- **Fixture provenance:** only project-authored/sanitized data; each native fixture manifest records source URL/schema branch, command, producer version, sanitization, and whether it was locally executed.

## Recommended File Ownership

- `crates/ratemypcb-core/src/schematic.rs`: bounded runner adapters, normalized native records, hierarchy/source fact model, exports, and reconciliation.
- `crates/ratemypcb-core/src/lib.rs`: loading/review/report/evidence/gate wiring and existing PCB DRC migration.
- `crates/ratemypcb-core/tests/schematic_release.rs`: released-major, hierarchy, mismatch, capability, and failure integration corpus.
- `crates/ratemypcb-cli/src/main.rs`: `--schematic`, honest doctor/help output, and existing exit semantics.
- `schemas/report-2.0.json`: generated additive equality update.
- Existing viewer/skill/docs files: consume core decisions and describe bounded capabilities; no second analysis engine.

## Primary Sources

- KiCad 8 CLI: <https://docs.kicad.org/8.0/en/cli/cli.html>
- KiCad 9 CLI: <https://docs.kicad.org/9.0/en/cli/cli.html>
- KiCad 10 CLI: <https://docs.kicad.org/10.0/en/cli/cli.html>
- Official exit codes: <https://docs.kicad.org/doxygen/namespaceCLI_1_1EXIT__CODES.html>
- ERC 8 schema: <https://gitlab.com/kicad/code/kicad/-/raw/8.0/resources/schemas/erc.v1.json>
- DRC 8 schema: <https://gitlab.com/kicad/code/kicad/-/raw/8.0/resources/schemas/drc.v1.json>
- ERC 9 schema: <https://gitlab.com/kicad/code/kicad/-/raw/9.0/resources/schemas/erc.v1.json>
- DRC 9 schema: <https://gitlab.com/kicad/code/kicad/-/raw/9.0/resources/schemas/drc.v1.json>
- Current ERC schema: <https://gitlab.com/kicad/code/kicad/-/raw/master/resources/schemas/erc.v1.json>
- Current DRC schema: <https://gitlab.com/kicad/code/kicad/-/raw/master/resources/schemas/drc.v1.json>
- KiCad schematic format: <https://dev-docs.kicad.org/en/file-formats/sexpr-schematic/index.html>
- Stable release policy: <https://dev-docs.kicad.org/en/rules-guidelines/release-policy/index.html>
- ERC exclusion defect #16924: <https://gitlab.com/kicad/code/kicad/-/work_items/16924>
- ERC exclusion defect #22377: <https://gitlab.com/kicad/code/kicad/-/work_items/22377>
- KiCad Altium importer limits: <https://dev-docs.kicad.org/en/import-formats/altium/index.html>
- KiCad 9 netlist documentation: <https://docs.kicad.org/9.0/en/eeschema/eeschema.html>

## Residual Risks

- KiCad 8/9 fixtures are documentation/schema-attested, not locally executed in this checkout; their manifests must preserve that distinction.
- No official evidence proves exclusion completeness in every KiCad 10 build. Keep unknown exclusion state and the limitation until a pinned fixture proves otherwise.
- Violation `type` and descriptions are producer data, not stable API identifiers. Stable evidence identity must include structural marker/item location rather than prose.
- KiCad project discovery for parity is conventional rather than formally specified. Require coherent same-project context and fail closed on ambiguity.
- Phase 2 human accessibility/usability checks remain independent and deferred; Phase 4 automated viewer regressions do not close them.
