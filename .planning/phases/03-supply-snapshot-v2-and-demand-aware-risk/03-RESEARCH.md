# Phase 3 Research

Official provider documentation was reviewed without authentication or provider calls.

| Provider | Useful official contract facts | RateMyPCB gate |
| --- | --- | --- |
| Nexar/Octopart | Exact MPN/manufacturer, sellers/offers, authorization flag with a manufacturer-line caveat, inventory/MOQ/package/prices; controlling API terms were inaccessible. | Query, cache, embed, sharing, and retention unknown; disabled. |
| Mouser Search V2 | MPN+manufacturer search, availability, package, MOQ, order multiple, price breaks, lifecycle, lead time. Public terms prohibit storing/caching API content and constrain display. | RateMyPCB application/use approval absent; disabled. |
| DigiKey Product Information v4 | Manufacturer part number plus manufacturer ID, product variations, package quantities, MOQ/standard package/prices/status. Agreement limits approved purposes and prohibits local database/unapproved downstream disclosure. | Public multi-provider display and retention approval absent; disabled. |
| LCSC OpenAPI | Search can use MPN/manufacturer concepts; public response schema/data-use rights are incomplete. | API-specific cache/embed/share/retention rights unknown; disabled. |

## Engineering implications

The safe deliverable is a provider-neutral offline contract with synthetic fixtures and explicit unknown/null states. Each observation stays seller/provider scoped; the evaluator never aggregates sellers. Retrieval freshness, upstream observation age, and legal expiry are separate. Authorization is three-state and never inferred from seller identity. Provider-specific adapters are not required for Phase 3 completion while legal/account gates are closed.

## Sources

- Nexar support: Supply GraphQL examples, authorization, limits, distributor classification, inventory freshness.
- Mouser official Search API and API Terms.
- DigiKey official ProductDetails v4, Product Information plan/FAQ, shared concepts, and API User Agreement.
- LCSC official OpenAPI, API overview, and access-frequency FAQ.

No provider response, credential, unofficial payload example, scraped page, or test cassette was used.
