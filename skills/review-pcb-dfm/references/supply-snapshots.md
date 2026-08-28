# Supply snapshots

RateMyPCB supply snapshot v2 is an offline, provider-neutral evidence contract. `scripts/enrich_bom.py` only creates a deterministic request template from exact BOM manufacturer+MPN pairs; it performs no network request and marks Mouser, DigiKey, and LCSC independently `not-checked`.

A v2 snapshot declares retrieval, evidence, and legal expiry separately; build quantity, attrition basis points, spares, region, currency, and packaging; explicit terms decisions for every named provider; exact identity/match state; raw and normalized lifecycle assertions; independent provider checks; and seller-scoped offers. Offers retain authorization state, seller/SKU, packaging, region, nullable stock/MOQ/order multiple/lead time, decimal-string price breaks, timestamps, and provenance. Missing quantities stay `null`, never zero.

RateMyPCB validates all bounds and terms before evaluation. Demand is `BOM quantity × build quantity + ceil(attrition) + spares`; MOQ and order multiple determine purchasable quantity. Stock and price apply only to an authorized, current offer matching region, packaging, and currency. Sellers are never aggregated and currencies are never converted or compared.

Provider gates remain closed. No RateMyPCB-specific written approval exists for query, cache, fixtures, HTML embedding, sharing, or retention for Nexar, Mouser, DigiKey, or LCSC. Do not add credentials or provider payloads to snapshots. Mouser and DigiKey public terms restrict storage/database uses; Nexar controlling terms and LCSC API data rights remain unresolved. Record each unavailable provider as `not-checked`, not not-found or zero stock.

Only project-authored synthetic fixtures may use `synthetic-test-data`. Suggestions stay candidate evidence and never reduce risk. An approved alternate requires named engineering authority, approval time, exact manufacturer+MPN, and evidence references. Snapshot v1 is accepted only through a conservative importer: aggregate stock/float prices are discarded, named providers remain `not-checked`, duplicates become ambiguous, and alternates remain unapproved.
