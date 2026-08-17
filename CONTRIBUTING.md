# Contributing

Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and
`cargo test --all` before submitting a change. New checks need a stable finding
ID, evidence-based wording, an explicit coverage statement, and fixtures for
both positive and negative cases. Do not claim a check passed when its required
evidence was unavailable.

