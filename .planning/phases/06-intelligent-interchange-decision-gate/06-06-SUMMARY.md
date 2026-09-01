---
phase: 06-intelligent-interchange-decision-gate
plan: 06
subsystem: private-odbpp-archive-ingestion
status: complete
tags: [rust, odbpp, gzip, ustar, archive-security, official-evidence]
requires:
  - phase: 06-intelligent-interchange-decision-gate
    plan: 05
    provides: Accepted parser-wide absolute deadline/cancellation and deterministic accounting boundary.
provides:
  - Byte-identified bounded Unix ingestion for one gzip method-8 member containing exact USTAR headers and bounded GNU long-name metadata.
  - No-follow archive opens, owner-only RAII temporary materialization, raw path/type validation, stored-mode suppression, typed limits, and deterministic manifest/accounting.
  - Project-authored hostile archive regressions plus one private official v8.1 evidence run.
  - One fresh correctness/security review and one remediation pass closing three P1 and two P2 findings.
  - PRIVATE repository checkpoint 1d7d859e74d995f86eb6b4110ef86673cd4f1938 on main.
affects: [FMT-03-evidence, future-FMT-04-candidate]
requirements-completed: []
completed: 2026-08-30
---

# Phase 6 Plan 06: Secure Archive Ingestion and Official-Sample Evidence

## Outcome

The private parser now accepts an unpacked directory or a byte-identified Unix archive path through one `ParseControl`. The archive lane accepts exactly one gzip method-8 member containing exact USTAR magic/version headers and the bounded GNU long-name records actually present in the committed official sample. It never shells out. PAX, GNU long-link, sparse, links, devices/special entries, member compression, nested archives, concatenated gzip, and non-Unix archive ingestion fail typed.

The official archive is fully read, decompressed, raw-entry-accounted, safely materialized, matrix-parsed, and cleaned. Package parsing then fails closed at the pre-existing exact fixed-point boundary: `steps/CELLULAR_FLIP-PHONE/profile` line 12 has a coordinate finer than one picometre. No rounding, package, adapter result, omission ledger, or capability record is returned. This is useful archive/parser evidence only; it is not conformance, representation, adoption, or product-support evidence.

## Byte-identified container facts

- Outer bytes: gzip magic `1f8b`, compression method 8, one member.
- Decoded headers: exact `ustar\0` plus version `00`.
- Raw TAR entries: 1,372.
- Logical entries: 1,190 — 839 regular files and 351 directories.
- Explicitly handled metadata: 182 GNU long-name records carrying 19,058 bytes.
- Member-level `.Z` or other nested compression: zero.
- Every other raw extension/type is rejected before materialization.

The first implementation used `tar::Archive::entries()`'s preprocessed view and only a five-byte `ustar` prefix. The independent review correctly blocked that boundary. The remediation switched to raw iteration, exact eight-byte magic/version validation, explicit metadata accounting, and a narrowly bounded GNU-long-name state machine because those 182 records are part of the official bytes.

## Production archive budgets

| Budget | Limit | Official observation |
| --- | ---: | ---: |
| Compressed file | 16 MiB | 11,653,177 bytes |
| Decoded TAR stream | 64 MiB | 58,189,824 bytes |
| Raw entries, including metadata | 2,048 | 1,372 |
| One regular entry | 40 MiB | 33,606,137 bytes |
| Total regular entry bytes | 64 MiB | 57,106,098 bytes |
| Materialized bytes | bounded by total regular bytes | 57,106,098 bytes |
| Path | 256 ASCII bytes | within limit |
| Decoded/compressed ratio | 32:1 | 4.993:1 |
| Read/write chunk | 64 KiB | bounded throughout |

Every increment/multiplication is checked. The decoded-byte limit independently caps high-ratio input. Counts include raw GNU metadata so extension records cannot evade entry limits.

## Security and execution contract

- Unix opens use `O_NOFOLLOW | O_CLOEXEC`; descriptor metadata, not a second pathname lookup, decides regular-file type and compressed size.
- Raw USTAR name/prefix fields are validated before any path API conversion. Paths must be normalized ASCII-relative, one-root, and free of absolute, `..`, `.`, empty, backslash, colon, trailing-dot/space, Windows-reserved, duplicate, full-path case-collision, and component-prefix case-collision forms.
- Only regular files/directories plus a pending bounded GNU long-name record are accepted. PAX/global-PAX, GNU long-link/sparse, hard/symbolic links, continuous files, devices, and FIFOs fail typed.
- Nested archive/compression extensions and gzip/UNIX-compress/ZIP/bzip2/xz/zstd/TAR magic fail typed.
- Parser-owned `TempDir` uses mode `0700`; files use create-new mode `0600`. Archive ownership and all stored modes are ignored; the official sample's 839 executable file modes are counted but never applied.
- RAII cleanup is exercised on success, semantic error, resource failure, deadline, and active cancellation. No extracted tree persists.
- One control carries `ArchiveRead`, `Decompression`, `Materialization`, `Discovery`, `Read`, `Matrix`, `Profile`, `Geometry`, `Topology`, and `Mapping` counters. Interruption after an I/O attempt wins over that I/O result and returns no package/evidence.
- Logical manifest entries are sorted. Equivalent project-authored archives with reordered entries produce equal semantic output, logical manifest, non-source byte/record metrics, and operation counts.

## Dependency and supply-chain receipt

Direct dependencies, all crates.io registry sources and locked:

| Crate | Version | Features/use | License |
| --- | --- | --- | --- |
| [gzip decoder crate](https://crates.io/crates/flate2/1.1.10) | 1.1.10 | defaults off, pure-Rust backend; gzip/DEFLATE | MIT OR Apache-2.0 |
| `tar` | 0.4.46 | `default-features = false`; raw TAR iteration only, never crate unpacking | MIT OR Apache-2.0 |
| `tempfile` | 3.27.0 | cryptographically randomized owned `TempDir` and RAII cleanup | MIT OR Apache-2.0 |
| `libc` | 0.2.189 | Unix `O_NOFOLLOW`/`O_CLOEXEC` constants | MIT OR Apache-2.0 |

Locked transitives: `adler2 2.0.1`, `bitflags 2.13.1`, `cfg-if 1.0.4`, `crc32fast 1.5.1`, `errno 0.3.14`, `fastrand 2.5.0`, `filetime 0.2.29`, `getrandom 0.4.3`, `linux-raw-sys 0.12.1`, `miniz_oxide 0.9.1`, `once_cell 1.21.4`, `r-efi 6.0.0`, `rustix 1.1.4`, `simd-adler32 0.3.10`, `windows-link 0.2.1`, and `windows-sys 0.61.2`.

`cargo metadata --locked` found 20 non-root packages, all registry-sourced with an available permissive MIT/Apache-2.0/0BSD/Zlib choice. `cargo tree --locked --duplicates` found no duplicate versions. `cargo audit --deny warnings` loaded 1,226 RustSec advisories and found zero vulnerabilities/warnings. Offline locked tests/tree passed after fetch. No native compression/archive library or Git/path dependency exists.

## Official private evidence

Release-mode parser observation from the warmed test binary:

- Format: `GzipUstarGnuLongName`.
- Raw/logical/metadata entries: 1,372 / 1,190 / 182.
- Metadata bytes: 19,058.
- Files/directories: 839 / 351.
- Compressed/decoded/regular/materialized bytes: 11,653,177 / 58,189,824 / 57,106,098 / 57,106,098.
- Ignored executable modes: 839.
- Matrix parse: success.
- Package parse: typed `Feature` failure at `steps/CELLULAR_FLIP-PHONE/profile:12`, `coordinate is finer than one picometre`.
- Package/adapter/omissions/capabilities: none / none / 0 / 0.
- Operations `(archive_read, decompression, materialization, discovery, read, matrix, profile, geometry, topology, mapping)`: `(360, 9414, 28439, 65, 36, 1404, 521, 1, 0, 0)`; total 40,240.
- In-test archive-to-error elapsed: 211,129 µs.
- Test process: 0.46 s real, 0.06 s user, 0.14 s sys; 4,210,688-byte maximum RSS and 2,982,200-byte peak memory footprint.

These are one local process/machine observation and set no production threshold. The committed sample remains one producer-unstated, rights-unresolved v8.1 example.

The ignored synthetic release case also passed after the new zero-valued archive stages: small/medium/large elapsed observations were 883/1,385/10,210 µs; the isolated test process reported 0.26 s real, 15,368,192-byte maximum RSS, and 14,172,496-byte peak memory footprint. These remain non-representative.

## Fresh correctness/security review and one remediation pass

Review run: fresh read-only `reviewer` child `9833b6d8-1c44-49d3-8073-3a9e6df927a6`. Initial verdict: **BLOCK**, no P0, three P1, two P2. One remediation pass closed all five; no second broad review or zero-findings loop was run.

| Finding | Closure and focused regression |
| --- | --- |
| P1: five-byte USTAR check plus preprocessed TAR iteration let PAX/GNU metadata bypass raw type/count policy | Exact eight-byte USTAR magic/version, `.raw(true)`, raw entry counting, explicit bounded GNU long-name handling required by the official bytes, and rejection tests for PAX, GNU long-link/sparse, dangling/consecutive/unsafe long names, and malformed magic/version |
| P1: symlink swap between metadata and following `File::open` | Unix no-follow descriptor open; descriptor-only regular/size checks; direct symlink regression |
| P1: `entry.path_bytes()` could platform-normalize raw backslashes | Raw USTAR name/prefix parser before path APIs; direct and GNU-long-name backslash regressions |
| P2: non-Unix permission helpers were no-ops | Archive ingestion now fails typed on non-Unix; Unix directory/file modes are enforced |
| P2: temporary write errors escaped as bare I/O without archive evidence | Writes map non-interruption failures to typed `Materialization` with accumulated evidence and preserve typed cancellation; simulated disk-full and cancellation regressions |

## Gate results

- `cargo fmt --all -- --check`: pass.
- `cargo test --locked --all-targets`: **56 passed**, 0 failed; 1 ignored scaling case (52 library + 1 official integration + 3 regular synthetic integration).
- Strict `cargo clippy --locked --all-targets --all-features -- -D warnings`: pass.
- Locked/offline all-target test and dependency tree: pass.
- Primary Rust LSP on five changed source/test files: zero diagnostics.
- Focused lens: no production finding; reported `rust-unwrap` only inside `#[cfg(test)]` setup/assertion code, where panic intentionally fails a test. The inherited geometry/lib test findings are unchanged.
- Source and complete 57,106,098-byte decompressed archive Gitleaks 8.30.1 scans: zero findings.
- Public locked/offline unsupported-format regression: 1 passed, 0 failed, 6 filtered.
- GSD: Plan 06-06 structure has zero errors/warnings; all six phase plans are valid, with only 06-02's intentional historical empty-dependency wave warning; roadmap/phase state, `git diff --check`, exact private staged files, and zero public staged/product/Phase-7 files pass.

## Private commit receipt

- Repository: <https://github.com/ratemypcb/ratemypcb-odbpp> (`PRIVATE`)
- Branch/default: `main`
- Commit: `1d7d859e74d995f86eb6b4110ef86673cd4f1938`
- Subject: `feat: add bounded ODB++ archive ingestion`
- Push: ordinary fast-forward from receipt SHA `83e15f1e07eedb62c9f2fc017a08c0c5138766b8`.
- Origin: `https://github.com/ratemypcb/ratemypcb-odbpp.git`.
- Local/tracking/remote/API SHA equality, PRIVATE visibility, default `main`, and clean worktree: verified.
- No force-push, tag, release, visibility change, public CI, external contact, or public-worktree commit.

## Changed private files

- `Cargo.toml`
- `Cargo.lock`
- `README.md`
- `src/archive.rs`
- `src/lib.rs`
- `src/geometry.rs`
- `src/main.rs`
- `tests/archive_evidence.rs`

The original archive/provenance files were read but not modified or duplicated.

## Residual boundary

Still unsupported or unproven: the official sub-picometre profile coordinate and therefore all later package/capability semantics for this sample; UNIX `.Z` and every member/nested compression; PAX/GNU link/sparse and non-USTAR/non-gzip containers; non-Unix secure archive materialization; blocking filesystem-call cancellation latency; concurrent hostile mutation of unpacked trees; broader ODB++ entities/references; representative multi-producer/multi-revision corpus behavior; independent semantic conformance; production performance/allocation thresholds; public integration/release audit; maintenance ownership; and every third-party-rights/distribution/publication/support claim.

FMT-03, FMT-04, and FMT-05 remain pending. The next decision-product plan is reserved as 06-08 and is not created.
