# Phase 7: Decision-Grade DFM and Assembly Analysis - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-08-30
**Phase:** 07-decision-grade-dfm-and-assembly-analysis
**Areas discussed:** dependency boundary, canonical model reuse, qualification/promotion, implementation slicing, production authority input, schematic reconciliation reuse, release-action contract

---

## Dependency boundary

| Option | Description | Selected |
| -------- | ------------- | ---------- |
| Integrate immediately | Enable analyzers before Phase 6 records a format decision | |
| Inert design only | Limit work to Phase 7 planning and non-production contract/fixture design | ✓ |
| Require adapter adoption | Block Phase 7 unless Phase 6 adopts an intelligent format | |

**User's choice:** Inert design only until Phase 6 records adopt/no-go; no-go remains valid.
**Notes:** No production analyzer, required coverage, blocking finding, or inference promotion may appear early.

---

## Canonical model reuse

| Option | Description | Selected |
| -------- | ------------- | ---------- |
| New DFM board model | Normalize Phase 7 into a second model | |
| Extend existing contracts minimally | Consume `FabricationReview`, dispatcher, evidence, and validator | ✓ |
| Viewer-side analysis | Compute policy and actions in JavaScript | |

**User's choice:** Extend existing contracts minimally.
**Notes:** Viewer remains a renderer; no second approval engine.

---

## Qualification and promotion

| Option | Description | Selected |
| -------- | ------------- | ---------- |
| Analyzer decides blocking | Each analyzer emits its own final gate impact | |
| Reviewed family policy | Central fail-closed qualification controls `GateImpact` | ✓ |
| Global inference promotion | One checkpoint promotes all inference checks | |

**User's choice:** Reviewed family policy with family-specific human inference checkpoints.
**Notes:** Deterministic blockers require positive/hard-negative/mutation metrics and at least 95% precision; recall is reported without inventing a threshold.

---

## Implementation slicing

| Option | Description | Selected |
| -------- | ------------- | ---------- |
| Analyzer framework first | Build plugins/registries before any family | |
| One deterministic tracer | One small module and focused fixtures, then expand | ✓ |
| Implement all families together | Land DFM-01 through DFM-04 in one broad change | |

**User's choice:** One deterministic tracer, then bounded expansion.
**Notes:** Contract/fixture planning is safe now; production execution waits for Phase 6.

---

## Production authority input

| Option | Description | Selected |
| -------- | ------------- | ---------- |
| Fixture-only thresholds/order facts | Exercise families without a production authority path | |
| One bounded local seam | Normalize source/version-bound profile/project rules and representable order acknowledgements into existing fixed-point constraints/construction | ✓ |
| New profile/order model | Add a second public policy model | |

**User's choice:** One bounded local seam before geometry and construction families.
**Notes:** Missing threshold authority remains `not_checked`; unrepresented order/profile facts produce confirmation gaps only. Anonymous presets and common-practice defaults are not authority.

---

## Schematic reconciliation reuse

| Option | Description | Selected |
| -------- | ------------- | ---------- |
| Reimplement population comparisons | Duplicate BOM/placement/occurrence joins in `dfm.rs` | |
| Map existing typed output | Consume `SchematicReview.mismatches` and preserve current identity/fallback semantics | ✓ |
| Replace schematic reconciliation | Move all existing behavior into a new subsystem | |

**User's choice:** Map existing typed reconciliation output.
**Notes:** `dfm.rs` owns only family qualification and evidence mapping for the tracer; it does not own population/fitted/DNP/quantity/placement/footprint/revision comparisons.

---

## Release-action contract

| Option | Description | Selected |
| -------- | ------------- | ---------- |
| New report recommendation engine | Add a second public action model | |
| Validate assessment P1 | Core ranks top unblock evidence refs; P1 must intersect them | ✓ |
| Score-first ordering | Let score improvements outrank missing evidence | |

**User's choice:** Validate assessment P1 against the core-ranked unblock set.
**Notes:** Required evidence outranks qualified blockers, which outrank evidence-only attention; score never controls approval or action priority.

## the agent's Discretion

- Deterministic tie ordering, fixture names, and the final small module name.

## Deferred Ideas

- Physical package compatibility without authoritative package/land-pattern evidence.
- Intelligent-format parity before Phase 6 adoption.
- Any inference family becoming blocking before explicit family-specific human approval.
