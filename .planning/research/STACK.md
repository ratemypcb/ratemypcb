# Stack Research

**Scope:** Decision-grade local PCB release review  
**Confidence:** High for current repository and published formats/tools; medium for provider-account fields and intelligent-format implementation feasibility.

## Keep

- **Rust 1.85 workspace, serde/serde_json, bounded local I/O:** existing deterministic core and CLI are the right execution boundary.
- **Separate `Report` + digest-bound `Assessment`:** preserve measured evidence independently from engineering judgment.
- **Browser-native HTML/CSS/JS:** keep the self-contained viewer dependency-free; use semantic HTML (`details`, tables, links, buttons) before custom widgets.
- **`kicad-cli`:** primary authority for supported KiCad PCB DRC, schematic ERC, exports, and schematic parity. Parse source only for facts native outputs omit.
- **Python stdlib adapter script:** adequate for opt-in provider calls and normalized snapshot creation; no SDK/service/database is justified yet.

## Add only after gates

| Capability | Candidate | Gate | Confidence |
| ------------ | ----------- | ------ | ------------ |
| Gerber/X2 grammar | Existing Rust `gerber_parser` crate | Package legitimacy, official Ucamco corpus, bounded parse behavior | High feasibility |
| Excellon | Small bounded parser or existing audited crate | Grammar coverage and fuzz corpus | Medium |
| IPC-2581 | Existing Rust `ipc2581` crate | License/XSD redistribution, maintenance, corpus, hostile XML, completeness | Medium |
| ODB++ | Production importer only after spike | Siemens partner license, varied legal corpus, conformance, archive security, maintenance | Low-medium |
| Supply providers | Official Nexar, DigiKey, Mouser, LCSC APIs | Credentials, exact account schema, written intended-use/retention/display decision | Medium |

## Do Not Add

- A plugin framework, hosted backend, database, browser analysis logic, unofficial scraping, fuzzy auto-matching, or custom KiCad ERC engine.
- A new frontend framework or accessibility library; the viewer is small and native semantics cover the contract.
- An ODB++ parser dependency before the legal/corpus checkpoint.

## Version/Compatibility Policy

- Record tool/parser/provider versions and source digests in evidence.
- Test KiCad behavior against each supported released major; do not bind claims to nightly docs.
- Treat checked-in JSON schema as generated/verified output from one authoritative Rust DTO/schema path.
- Preserve Gerber/X2+Excellon support regardless of intelligent-format outcome.

## Primary Sources

- KiCad CLI: <https://docs.kicad.org/9.0/en/cli/cli.html>
- KiCad schematic format: <https://dev-docs.kicad.org/en/file-formats/sexpr-schematic/>
- KiCad board format: <https://dev-docs.kicad.org/en/file-formats/sexpr-pcb/>
- Ucamco Gerber downloads: <https://www.ucamco.com/en/gerber/downloads>
- ODB++ partner terms: <https://odbplusplus.com/design/partner-terms-of-use/>
- IPC-2581 Rev C samples: <https://www.ipc2581.com/ipc-2581-revc-test-cases/>
- DigiKey ProductDetails: <https://developer.digikey.com/products/product-information-v4/productsearch/productdetails>
- Mouser Search API: <https://www.mouser.com/en/api-search/>
- LCSC Open API: <https://www.lcsc.com/docs/openapi/index.html>
- Nexar API terms: <https://nexar.com/api/legal>
