# Phase 3 Validation

| Requirement | Automated evidence | Status |
| --- | --- | --- |
| SUP-01 | Supply validator rejects wrong/missing versions, unknown fields/enums, bad times/bounds, duplicates, float/invalid decimal money, unsorted breaks, and incomplete provenance. | automated |
| SUP-02 | Exact canonical manufacturer+MPN join; explicit match enum; v1 duplicate mutation proves ambiguity rather than first selection. | automated |
| SUP-03 | Synthetic fixture and report DTO preserve provider/seller/SKU/auth/package/region/nullable quantities/lead time/prices/times/provenance. | automated |
| SUP-04 | Fixture asserts independent checked, quota-error, and not-checked states for Mouser/DigiKey/LCSC. | automated |
| SUP-05 | Fixture proves quantity 2 × build 10 + 10% attrition + 1 spare = 23, then MOQ/order-multiple purchase quantity 30. | automated |
| SUP-06 | Raw/normalized/source/time lifecycle assertions and conflict state are typed and report-visible. | automated |
| SUP-07 | Fixture proves decimal `0.7500` applies at purchasable quantity; currency/package/region and commercial fields gate use. | automated |
| SUP-08 | Missing named terms profile and non-synthetic forbidden-retention mutations are rejected before report creation. Live adapters remain absent. | automated + human-needed provider approval |
| SUP-09 | Synthetic suggestion stays `not-checked`; approved alternates require authority/time/resolvable observation refs. | automated |
| SUP-10 | Redistribution-safe v2 fixture plus v1 conservative-import and mutation tests. | automated |

## Commands

- Focused: `cargo test -p ratemypcb-core supply_ --locked`
- Placement regression: `cargo test -p ratemypcb-core placement_ --locked`
- Adapter self-check: `python3 skills/review-pcb-dfm/scripts/enrich_bom.py --self-test`
- Full: `cargo fmt --all -- --check`; `cargo test --all --locked`; relevant Node tests; `git diff --check`.

Provider account/legal authorization is not an automated pass condition. It is recorded as human-needed and provider-scoped `not-checked`.
