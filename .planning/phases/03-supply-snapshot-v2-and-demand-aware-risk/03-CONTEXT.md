# Phase 3 Context

## Goal

Deliver lawful offline supply-v2 evidence that cannot turn ambiguity, missing provider checks, stale data, unknown quantities, or unapproved alternates into a pass.

## Locked decisions

- Identity is conservative canonical manufacturer+MPN; raw values remain visible. No fuzzy, MPN-only, or first-result joins.
- Named Mouser, DigiKey, and LCSC checks are independent. Omission is `not-checked`; provider errors and not-found remain distinct; missing stock is null.
- Demand is BOM fitted quantity × declared build quantity plus ceiling attrition and explicit spares. Offer applicability requires known authorization, region, packaging, stock, MOQ, and order multiple.
- Money is a decimal string. Currency is never converted or compared across currencies.
- Suggestions never mitigate. Approval requires exact alternate identity, named authority, approval time, and resolvable evidence.
- Provider observations are gated by explicit query/memory/disk/embed/share decisions and legal expiry. Project-authored synthetic data is separately labeled.
- Snapshot v1 is imported conservatively: aggregates and float prices are not promoted, named providers remain `not-checked`, and duplicates are ambiguous.

## Provider checkpoint

No repository evidence grants RateMyPCB its intended provider use. Live Nexar, Mouser, DigiKey, and LCSC adapters remain disabled. No credentials, calls, payload recordings, or durable provider data are part of this phase. Human provider/account approval remains needed for each exact query, cache, logging, fixture, embed, share, export, backup, and retention behavior.

## Phase 2 continuity

Human usability/accessibility verification remains explicitly deferred. Before Phase 3, report rendering was changed so an untrusted saved report cannot reopen report-declared local paths; required-evidence validation now compares the authoritative summary exactly; KiCad limitation evidence families are source-coupled rather than index-fallback coupled.
