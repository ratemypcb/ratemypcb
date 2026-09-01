# Phase 6: Intelligent Interchange Decision Gate - Context

**Gathered:** 2026-08-30
**Status:** Complete — the human selected FMT-03 no-go for both ODB++ and IPC-2581 for this release. FMT-04 is Not Applicable; existing unsupported/not-checked behavior completes FMT-05. PRIVATE SHA `a4216f6909754155555e9290c2ec84e0eb16d267` remains quarantined research only.

<domain>
## Phase Boundary

Evaluate ODB++ and IPC-2581 against the same evidence gates and record the human decision. The selected outcome is no-go for both formats for this release. Private parser work is not adoption, integration, distribution, or release authority. Native KiCad and Gerber/X2+Excellon remain the strongest path, and format presence never improves approval.

</domain>

<decisions>
## Implementation Decisions

### Evidence gate

- **D-01:** Compare both formats on the same rows: implementation and distribution authority; specification/schema/sample/validator asset rights; representative lawful corpus; canonical semantic coverage; hostile-input bounds; measured performance and determinism; exact dependency audit; maintenance ownership.
- **D-02:** An unresolved row fails closed. Public access, marketing, schema validity, an open-source parser license, or a format badge is not substitute evidence.
- **D-03:** Use the exact `CapabilityId`, `CapabilityState`, `Authority`, provenance, omission, conflict, and `dispatch_analyzer` contracts in `crates/ratemypcb-core/src/fabrication.rs`; do not create an intelligent-format approval path.
- **D-04:** Security findings remain `unproven` until executed hostile-input evidence exists; do not claim a parser is safe or unsafe from architecture alone.
- **D-05:** Legal findings are engineering release gates, not legal advice. ODB++ implementation/distribution authority is unresolved. IPC-2581's official “open, license-free” description is favorable implementation evidence, while standard/XSD/sample/validator use and redistribution rights remain separately unresolved.

### Pre-decision boundary

- **D-06:** Before the FMT-03 adoption decision, local original ODB++ parser work is allowed only in the separate private crate. Add no RateMyPCB product dependency, public adapter, classifier, corpus bytes, vendored schema/specification, or support claim.
- **D-07:** Keep official or ambiguous corpus material outside the public repository and outside the private repository except for the exact human-directed receipt/use in D-23/D-24. Parser tests otherwise remain project-authored synthetic files unless a separate rights manifest authorizes corpus use; download availability, private storage, and private execution never imply public CI or redistribution rights.
- **D-08:** Preserve source locations and deterministic parser evidence. Add no freeze, GPG, detached-authority, canonical-review JSON, or repeated zero-findings ceremony.
- **D-09:** Native KiCad and Gerber/X2+Excellon remain supported in every outcome. Generic XML must not imply IPC-2581 and `.tgz` presence must not imply ODB++.

### Decision and follow-up

- **D-10:** The FMT-03 checkpoint is complete: the human selected no-go for both ODB++ and IPC-2581 for this release.
- **D-11:** The ODB++ reply, rights-cleared representative corpus, conformance oracle, and equivalent technical evidence are future reopening conditions, not Phase 6 blockers.
- **D-12:** FMT-03 is Complete. FMT-04 is Not Applicable because no adapter was adopted. FMT-05 is Complete because existing product behavior already reports both formats unsupported/not checked and cannot improve approval.
- **D-13 (superseded by D-17):** Through accepted Plan 06-04, `/Users/mattiafiumara/repos/ratemypcb-odbpp-private` remained outside Git. The later explicit private-repository authorization below now controls Git operations for that directory only.
- **D-14:** Finish the smallest evidence-bearing core first: bounded unpacked-tree matrix/profile/basic-feature parsing with exact fixed-point units, deterministic typed provenance, explicit unsupported constructs, and a private policy-free `AdapterResult`-shaped mapping. Archives, public adapters, and product behavior remain out of scope.
- **D-15:** After accepting Plan 06-02, continue immediately with one bounded private Plan 06-03 for exact arc-aware primitive extents and provable straight-profile topology. Arc-heavy, workload-limited, or ambiguous topology must stay partial/unsupported; do not broaden record coverage.
- **D-16:** After accepting Plan 06-03, extend that same exact predicate engine to line-only general layer surfaces. Multiple islands must be disjoint, each hole must have exactly one containing island, one carried workload bounds all surface proof, and every unproved surface keeps `GeometryRegions` partial.
- **D-17:** Preserve accepted Plan 06-04 in the private GitHub repository `https://github.com/ratemypcb/ratemypcb-odbpp` at initial private `main` commit `9a9ffcbca70b279796d84d2f7f8fdfc51b8091d9`. The repository is PRIVATE; `origin` is `https://github.com/ratemypcb/ratemypcb-odbpp.git`. Ordinary scoped commits and normal pushes are authorized only for completed/reviewed private parser slices. Force-push, public visibility, releases, tags, deletion/rename/transfer, external assets, and public-worktree commits remain prohibited.
- **D-18:** Execute one bounded technical Plan 06-05 for a stdlib-only parser-wide control carrying one absolute deadline, explicit cooperative cancellation, and deterministic per-stage operation counts through discovery, path checks, chunked reads, matrix, profile, geometry, topology, and private mapping. Timeout/cancel returns a typed error and no partial package or mapping result.
- **D-19:** Add only project-authored synthetic hostile/scaling evidence. Account bytes, physical lines, records, features, contour vertices, topology work, and operation checkpoints deterministically. Record reproducible local wall-time/RSS/operation-count observations as non-representative measurements without inventing an adoption or release threshold.
- **D-20:** Plan 06-05 must not broaden ODB++ semantics. Fixed-point/provenance and Plan 06-04 Complete/Partial boundaries remain invariant; representative corpus, conformance oracle, production performance, archive/concurrent-mutation, maintenance, and legal/integration/publication gaps stay explicit.
- **D-21:** Accept Plan 06-05 after one fresh review and one bounded remediation pass. The parser now checks every relevant filesystem result before mapping it, charges profile surface/extent work to `Profile`, checks after final omission collection, and uses barrier-synchronized active cancellation regressions. Private `main`, tracking, remote ref, and GitHub API all equal reviewed SHA `07a42c937cf550eeb7c9d5d5c233b474cb386a0d`.
- **D-22:** Treat the local release-mode scaling observations as non-representative only: small/medium/large elapsed values were 541/1,380/8,167 µs; the warmed Cargo/test process reported 0.26 s real and 21,889,024-byte maximum RSS. No adoption, release, production-performance, or cancellation-latency threshold is created from these values.
- **D-23:** Explicit human direction supersedes D-07/D-17 only for retaining the original official `designodb_rigidflex.tgz` byte-for-byte in PRIVATE `ratemypcb/ratemypcb-odbpp`. Receipt SHA `83e15f1e07eedb62c9f2fc017a08c0c5138766b8` adds only the 11,653,177-byte archive and provenance note; SHA-256 is `e67cbbdf95044b0a961fea956ef0e292121755b5de413e95a3265269eb24ee78`. Rights remain unresolved; storage alone grants no parser use, public CI, redistribution, adoption, integration, or third-party-clearance claim. The archive has no unsafe paths/links/devices or expansion blocker, but all 839 regular entries carry mode `0777`, so it remains quarantined.
- **D-24:** The accepted private evidence culminates at SHA `a4216f6909754155555e9290c2ec84e0eb16d267`, which preserves exact precision omissions without rounding. It remains quarantined research and closes no rights, representation, conformance, adoption, public-CI, integration, distribution, publication, or release row.
- **D-25:** No product change is needed for no-go. Existing CLI and doctor output already mark ODB++ and IPC-2581 unsupported, while unavailable capability-gated analysis remains not checked.

### the agent's Discretion

- Keep the comparison and checkpoint concise. Prefer existing planning, capability, report, CLI, and test seams; no new framework or decision artifact format.

</decisions>

<canonical_refs>

## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Product and phase truth

- `.planning/ROADMAP.md` — Phase 5 acceptance and Phase 6 goal/checkpoint.
- `.planning/REQUIREMENTS.md` — FMT-01..FMT-05 and immutable approval-honesty requirements.
- `.planning/STATE.md` — current feature position and blockers.
- `.planning/phases/05-manufacturing-evidence-model-and-gerber-baseline/05-CONTEXT.md` — canonical model, resource, digest, adapter/analyzer, and no-ceremony contracts.
- `.planning/phases/05-manufacturing-evidence-model-and-gerber-baseline/05-VERIFICATION.md` — accepted Phase 5 baseline at local commit `5e0fa62a5865cdea1a7755c6bedcedab3a64ba07`.
- `/tmp/ratemypcb-parallel-prep.phase6-formats.md` — prior research input only; its Phase 5 status is stale and must not override the current branch.

### Existing implementation seams

- `crates/ratemypcb-core/src/fabrication.rs` — exact canonical capability ledger and fail-closed analyzer dispatcher.
- `crates/ratemypcb-core/src/lib.rs` — report evidence, required coverage, approval, and archive boundaries.
- `crates/ratemypcb-core/Cargo.toml` and `Cargo.lock` — current production dependency baseline.
- `crates/ratemypcb-cli/src/main.rs` — current ODB++/IPC-2581 unsupported CLI and doctor output.
- `crates/ratemypcb-cli/tests/decision_report.rs` — existing unsupported-format regression.

### Primary external evidence

- `https://odbplusplus.com/design/why-odb-d` — public ODB++ access, ownership, and maintenance statements.
- `https://odbplusplus.com/design/partner-terms-of-use/` — one published partner-license route and its conditions; not an executed RateMyPCB grant.
- `https://odbplusplus.com/design/our-resources/` and `https://odbplusplus.com/wp-content/resource-docs/designodb_rigidflex.tgz` — official v8.1 sample listing and exact private receipt source; storage is not third-party rights clearance.
- `https://www.ipc2581.com/solicitation-for-input-for-ipc-2581-revc/` — official “open, license-free” statement.
- `https://www.ipc2581.com/ipc-2581-file-validation-tool/` — validator scope, schema-only limitation, and separate validator license.
- `https://www.ipc2581.com/ipc-2581-revc-test-cases/` — official Rev C test-case inventory.
- `https://docs.rs/crate/ipc2581/0.3.3` — concrete Rust candidate metadata and source; not adoption authority.

</canonical_refs>

<code_context>

## Existing Code Insights

### Reusable Assets

- `CapabilityId`/`CapabilityRecord`/`AdapterResult`: canonical facts, explicit state, authority, provenance, omissions, and conflicts.
- `dispatch_analyzer`: requires exactly one `complete` record per prerequisite; every other state returns `not_checked`.
- CLI `doctor` and help output: already identify ODB++ and IPC-2581 as unsupported.

### Established Patterns

- Original-byte SHA-256 and deterministic canonical model digests remain legitimate integrity evidence.
- One carried manufacturing deadline and existing resource limits are the baseline; format-specific virtual-tree/XML limits need proof, not assumption.
- Browser parsing and filename inference remain non-authoritative.

### Integration Points

- Any later adopted adapter enters `FabricationReview`; analyzers, evidence finalization, approval policy, schemas, CLI, viewer, and skill remain consumers.
- A later no-go path should reuse existing unsupported/not-checked evidence and CLI/report seams rather than create placeholder adapters.

</code_context>

<specifics>
## Specific Ideas

One comparison table, one completed no-go checkpoint, and one quarantined private technical track. Plans 06-02 through 06-07 culminate at PRIVATE SHA `a4216f6909754155555e9290c2ec84e0eb16d267`; that evidence remains non-representative research only. Plan 06-08 verifies the existing unsupported/not-checked product path without product changes.

</specifics>

<deferred>
## Deferred Ideas

- Any ODB++ or IPC-2581 adapter is deferred to a separately authorized future reopening with complete rights, corpus, conformance, security, performance, dependency, and maintenance evidence.
- No FMT-05 implementation is deferred: existing unsupported/not-checked behavior already satisfies it.
- Additional private parser or corpus work does not reopen the release decision by itself.

</deferred>

---

*Phase: 06-intelligent-interchange-decision-gate*
*Context gathered: 2026-08-30*
