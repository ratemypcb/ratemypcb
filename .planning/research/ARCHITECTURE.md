# Architecture Research

**Confidence:** High for the target responsibility split; medium for intelligent-format and provider adapter details pending gates.

## Existing Flow to Preserve

`review-pcb-dfm` → CLI `review` → core `review()` → bounded loaders/checkers → deterministic versioned `Report` → exact-byte digest → independent `Assessment` validation → HTML snapshot.

The viewer consumes evidence. It must not compute release truth, BOM risk, or fabrication conclusions.

## Target Flow

```text
bounded artifact inputs / native tools / provider observations
  → format adapters with explicit capabilities, omissions, provenance
  → one canonical evidence model
  → capability-gated analyzers + cross-artifact reconciler
  → versioned deterministic report + approval policy
  → digest-bound assessment with evidence references
  → terminal and decision-first offline HTML consumers
```

## Responsibility Map

| Tier | Owns | Must not own |
| ------ | ------ | -------------- |
| Input boundary | path/archive/provider limits, selection, digests, source metadata | engineering conclusions |
| Native-tool adapter | versioned `kicad-cli` ERC/DRC/export/parity execution and marker preservation | circuit correctness claims beyond output |
| Format adapters | syntax/structure → canonical facts + capabilities/omissions | approval or risk policy |
| Canonical evidence | layers, geometry, tools, nets, components, construction, provenance, conflicts, unknowns | format-specific UI objects |
| Analyzers/reconciler | checks with declared capability requirements and stable finding identity | silently merging conflicting sources |
| Report policy | required coverage, risk/confidence/freshness separation, approval gate, schema | prose engineering judgment |
| Assessment | evidence-linked disposition, rationale, actions, questions | mutation of deterministic evidence |
| CLI/viewer | orchestration and accessible presentation | hidden analysis or alternate approval |

## Canonical Manufacturing Evidence

Minimum domains:

- product/document identity and digests;
- layer roles/order/context/polarity;
- fixed-point geometry, transforms, regions, repetitions, profile/cutouts;
- apertures/symbols and plated/non-plated drill/rout tools/spans;
- nets/pins/features/connectivity where explicitly supplied;
- components/packages/placement/fitted/DNP/variants;
- stackup/material/thickness/finish/impedance evidence where supplied;
- source constraints separate from observed release geometry;
- per-fact provenance class (`explicit`, `derived`, `inferred`), source location, confidence, parser completeness, warning/conflict.

Unknown, absent, unsupported, failed, and false are separate states. Lower-fidelity input never overwrites stronger evidence; reconciliation records agreement or conflict.

## Plain Module Direction

Extract only as new work needs the seam:

- `input`: bounded inventory, selection, archive virtual filesystem.
- `evidence`: findings, coverage, provenance, stable IDs, gate.
- `design`: native KiCad board/schematic evidence.
- `fabrication`: canonical manufacturing types and Gerber/Excellon adapters.
- `assembly`: BOM/placement/correlation.
- `supply`: typed snapshot validation/evaluation.
- `report`: DTO/schema/assessment validation.

This is not a plugin system. Input adapters implement one internal contract; analyzers consume declared capabilities.

## Data Contract Rules

- Rust DTO/schema generation is authoritative; checked-in schemas must equality-test against generated output.
- `checkId` identifies a rule family; deterministic `findingId` identifies one canonical occurrence and remains stable across ordering/prose changes.
- Every evidence item carries artifact digest, source/tool/version, structured location, evidence class, confidence, and freshness when applicable.
- Assessment verdict/actions/category summaries/questions all reference valid evidence IDs.
- Self-contained reports redact or omit provider fields whose terms profile forbids persistence.

## Open Gates

1. Provider-specific terms profile and written intended-use decision before adapter persistence/display.
2. Gerber parser package legitimacy and official-corpus behavior before dependency adoption.
3. ODB++ partner license plus diverse corpus before implementation commitment.
4. IPC-2581 standard/XSD redistribution plus crate security/maturity before implementation commitment.
5. Supported KiCad major/version matrix before freezing normalized ERC/parity identity fields.

## Primary Sources

- KiCad CLI: <https://docs.kicad.org/9.0/en/cli/cli.html>
- Ucamco Gerber specification/downloads: <https://www.ucamco.com/en/gerber/downloads>
- ODB++ specification: <https://odbplusplus.com/wp-content/uploads/sites/2/2024/08/odb_spec_user.pdf>
- IPC-2581 validation: <https://www.ipc2581.com/ipc-2581-file-validation-tool/>
- Nexar query templates: <https://support.nexar.com/support/solutions/articles/101000472564>
