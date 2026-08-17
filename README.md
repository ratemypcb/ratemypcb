# RateMyPCB

RateMyPCB is a local, deterministic PCB manufacturing preflight and DFM linter.
It reviews KiCad source projects and fabrication ZIPs without uploading design
data, then emits a human verdict or a stable JSON report for agents and CI.

```sh
cargo run -p ratemypcb-cli -- review .
cargo run -p ratemypcb-cli -- review board.kicad_pcb --format json
cargo run -p ratemypcb-cli -- doctor
cargo run -p ratemypcb-cli -- schema
```

End users do not need Rust. Download the signed binary for your platform from
GitHub Releases, or let the Agent Skill run its checksum-verifying user-local
installer after you approve the download. Rust is needed only to build from
source.

The standalone engine checks evidence it can prove locally. When a compatible
`kicad-cli` is installed, `--native auto` adds KiCad's native DRC report. Without
it, the review still completes and marks exact clearance, connectivity, custom
rules, exclusions, and zone-refill checks as `not_run`.

RateMyPCB is a release aid, not a compliance certificate. Safety-critical
designs require qualified engineering review and fabricator validation.

## Supported inputs

- `.kicad_pcb` with coherent `.kicad_pro` and `.kicad_dru` sidecars
- project directories containing one or more KiCad boards
- fabrication ZIPs containing Gerber, drill, BOM, placement, and/or source files
- Altium artifacts are inventoried; `.PcbDoc` source-aware DRC is not supported

Use `--board relative/path.kicad_pcb` when a repository contains multiple boards.
Use `--fail-on critical|high|medium|low|info|never` to make findings gate CI.

Licensed under Apache-2.0. RateMyPCB trademarks remain reserved.
