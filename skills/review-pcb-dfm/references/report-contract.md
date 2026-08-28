# Report contract

## Versioning

Active schemas: report 2.0 and assessment 2.0.

The authoritative generated paths are `schemas/report-2.0.json` and `schemas/assessment-2.0.json`. Historical 1.x schemas remain compatibility records, not active declarations. The CLI's deterministic `score` and the assessment's engineering `rating` are secondary metadata; assessment `disposition` alone is the release decision.

## Evidence report

- `reviewScope`: `design`, `fabrication`, `assembly`, or `full`.
- `observedRisk`, required-check `coverage`, `evidenceConfidence`, `freshness`, and `approvalEligible` are separate states; none substitutes for another.
- Missing, failed, stale, unsupported, unknown, `not_run`, `not_provided`, and attention-required evidence closes approval without reducing observed risk or becoming a pass-like label.
- `score.value`: deterministic 0–10 evidence score shown only after disposition, actions, and completeness/freshness.
- `requiredEvidence`: explicit check ID, provenance-backed evidence ID, execution, result, freshness, and confidence.
- `coverage` and `findings`: one globally unique public evidence ID per occurrence, separate stable `checkId`, and bounded provenance containing artifact identity/digest, producer/version, structured location, evidence class, confidence, freshness, and observation time.
- `categories`: Design Integrity, Fabrication, BOM, Assembly, Supply Chain, and Evidence & Coverage with status plus referenced IDs.
- `bom`: additive line-by-line BOM evidence. Each row preserves raw manufacturer+MPN, calculated build demand, raw/normalized lifecycle assertions and conflicts, independent named-provider states, seller-scoped authorization/SKU/package/region/quantity/timestamp/provenance, decimal-string applicable pricing, alternate authority, and a core-provided release-impact judgment. Missing or legally unavailable evidence is explicit and never treated as a pass.
- `nativeDrc.violations`: preserved KiCad violation type, group, severity, description, tri-state exclusion (`true`, `false`, or unknown), and affected item payloads. Ordinary DRC, unconnected, schematic-parity, and ERC channels remain distinct.
- `schematic`: additive bounded project evidence: selected project/root/board paths and exact digests, canonical occurrence UUID paths and item IDs, fact provenance, capability states, evidence-only reconciliation mismatches, native ERC, and coherent board/schematic parity. Exact occurrence identity is authoritative; unique-reference fallback is explicitly low confidence. A missing `excluded` field remains unknown, never active or pass.
- `profileDrc`: second native pass using a complete staged KiCad project plus the selected fabricator minimums; report only genuine deltas from the native pass.
- `approvalEligible`: deterministic gate derived from required scope coverage and active findings.
- `profile`: selected versioned fabricator profile and its official source.
- `limitationEvidenceRefs`: optional additive references parallel to visible limitations. New reports provide nonempty references so each rendered limitation resolves to provenance; older unlinked limitations are not promoted as evidence-backed claims.

Generate the authoritative schema with `ratemypcb schema`.

## KiCad schematic capability boundary

Review the complete project directory so `.kicad_pro`, hierarchy children,
variables, library context, board, BOM, placement, and netlist exports remain
coherent. `--schematic PATH` is only a bounded project-relative selector for
multiple automatic roots; invalid, external, child-only, duplicate, or
non-ambiguous selection is invalid input (CLI exit 2). It cannot enable native
execution for ZIP input.

Native PCB DRC, schematic ERC, and coherent-project parity dispatch only to
KiCad CLI majors 8, 9, and 10. Native tool exits 0 and 5 both mean completed;
unavailable, timeout, other exits, missing/oversized/malformed/truncated output,
or executable/report-major mismatch become provenance-bearing `not_run` in
auto mode and CLI execution failure (exit 3) in required mode. KiCad 10 is
locally exercised; KiCad 8/9 fixtures are documentation/schema-attested and are
not described as locally executed.

ZIP schematics remain inventory-only for native checks. Altium `.SchDoc` is
inventory-only. Recognized generic netlists retain explicit exported component,
net, and pin fields only; they do not imply native ERC, hierarchy, parity, DNP,
power intent, or revision semantics. Every Phase 4 schematic family remains
`evidence_only` and absent from `requiredEvidence` until a separate approved
corpus checkpoint promotes a named family.

## Assessment 2.0

Required fields are `assessmentSchemaVersion`, `reportDigest`, `rating`, `disposition`, `verdict`, `verdictEvidenceRefs`, `rationale`, `categorySummaries`, `actions`, and `questions`. Bind `reportDigest` to the exact report bytes and do not rewrite the report afterward. Present one disposition and verdict, no more than three actions, required-evidence completeness/freshness, and only then rating/score. Verdicts, category summaries, actions, and structured questions must each carry nonempty evidence references resolving to the bound report.

```json
{
  "assessmentSchemaVersion": "2.0",
  "reportDigest": "64 lowercase SHA-256 characters",
  "rating": 5,
  "disposition": "revise",
  "verdict": "Revise BOM: obsolete MCU",
  "verdictEvidenceRefs": ["ev-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"],
  "rationale": "The layout evidence is promising, but the selected MCU blocks release.",
  "categorySummaries": [
    {"categoryId": "supply-chain", "summary": "One obsolete fitted part.", "evidenceRefs": ["ev-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]}
  ],
  "actions": [
    {"priority": 1, "title": "Approve an MCU replacement", "rationale": "The current part is obsolete.", "evidenceRefs": ["ev-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]}
  ],
  "questions": [
    {"question": "Which approved replacement is qualified?", "evidenceRefs": ["ev-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]}
  ]
}
```

The CLI rejects unknown or duplicate evidence references, report-digest mismatches, invalid ratings/dispositions, overlong verdicts, more than three actions, and approval while `approvalEligible` is false. The viewer consumes validated data and must not truncate or repair over-budget assessments.

## Exit codes

- `0`: completed and no configured threshold met.
- `1`: completed but configured threshold met.
- `2`: invalid/ambiguous input or assessment.
- `3`: execution failure.
