# Pitfalls Research

**Confidence:** High unless marked as an open gate.

| Pitfall | Failure mode | Prevention / measurable signal |
| --------- | -------------- | -------------------------------- |
| Score-first report | User mistakes a high number for approval | Disposition/actions/coverage first; scores secondary; comprehension test |
| Coverage collapsed into confidence/risk | Unknown evidence lowers apparent risk or passes | Separate enums/policy; mutation goldens prove every missing required check closes approval |
| Positional evidence IDs | Assessment links drift when ordering changes | Canonical occurrence key + deterministic ID; ordering/prose stability fixtures |
| Viewer computes conclusions | JSON/terminal and HTML disagree | Core emits decisions; viewer renders only; cross-consumer goldens |
| Schema drift | skill/docs/fixtures accept 1.1 while core emits 1.2 | One authority and generated-file equality test |
| MPN-only/first-result joins | Wrong manufacturer part accepted | Identity tuple, candidate count, explicit ambiguous/not-found/error states |
| Unknown numeric as zero | Missing stock appears unavailable, or missing lead time appears immediate | Nullable values plus explicit status; adversarial fixture |
| Seller aggregation | Global stock hides named-provider gaps | Preserve provider/seller/SKU/packaging offers and per-provider checks |
| Suggested alternate promotion | Cheap/provider candidate reduces release risk | Provenance + qualification state; only explicit `approved` mitigates |
| Provider terms treated as docs | Restricted live data is persisted/shared | Versioned terms profile gates query/retention/embed/share; no scraping fallback |
| “Fresh retrieval” equals live | Cached upstream feed shown as real time | Show retrieval and upstream age separately; provider-specific freshness |
| Filename/token fabrication checks overstated | Invalid geometry passes “parsed” | Rename shallow evidence; real parser completeness/capability gates approval |
| Format badge equals completeness | Partial ODB++/IPC-2581 receives best-evidence status | Capability matrix and omissions, independent of extension |
| Premature ODB++ commitment | License/corpus/security/maintenance trap | Blocking checkpoint with explicit no-go outcome |
| IPC-2581 assumed open/easy | XSD rights, XML DoS, crate gaps ignored | Equal legal/corpus/conformance/security/maintenance gate |
| Archive extraction | traversal, links, bombs, allocation exhaustion | Read-only virtual archive, byte/count/depth/ratio/reference budgets, fuzzing |
| Hand-rolled KiCad semantics | hierarchy/power/ERC claims are wrong | Native ERC/parity/exports first; source parse only for missing facts |
| Reference-only schematic matching | reused sheets and units collide | Occurrence/UUID hierarchical identity and mutation fixtures |
| Netlist overclaim | connectivity import becomes native ERC/parity claim | Capability declaration; list fields lost by export |
| Altium overclaim | opaque source treated as validated | Inventory/native-not-run unless licensed native report/automation |
| Heuristics become blockers too early | decoupling/interface false positives stop release | Inference label, precision corpus, non-blocking until threshold reached |
| Self-contained HTML assumed safe | sensitive board/provider data persists indefinitely | Explicit sensitivity notice, redaction policy, no restricted data without permission |
| Monolith “cleanup” milestone | refactor delays user value and risks dirty baseline | Extract plain modules only inside vertical evidence slices |
| Golden corpus licensing | redistributed real designs violate terms | Purpose-built/permissive fixtures with license metadata and digests |
| Test count as quality | many shallow tests miss ambiguity/traceability | Mutation fixtures and user-visible acceptance metrics |

## Open Gates

- Exact provider account agreements and response fields.
- ODB++ development/license approval and corpus access.
- IPC-2581 XSD/test redistribution rights and parser maintenance.
- Supported KiCad major-version marker stability.
- Legitimacy and security review of any new parser dependency.

## Primary Sources

- Nexar terms: <https://nexar.com/api/legal>
- Mouser terms: <https://www.mouser.com/en/apiterms/>
- DigiKey agreement: <https://developer.digikey.com/api-user-agreement>
- LCSC terms: <https://www.lcsc.com/docs/>
- ODB++ partner terms: <https://odbplusplus.com/design/partner-terms-of-use/>
- IPC FAQ: <https://www.ipc.org/frequently-asked-questions>
- Rust tar entry security boundary: <https://docs.rs/tar/latest/tar/struct.Entry.html>
- KiCad schematic format: <https://dev-docs.kicad.org/en/file-formats/sexpr-schematic/>
