# Report contract

## Contents

- [Versioning](#versioning)
- [Core fields](#core-fields)
- [Coverage](#coverage)
- [Exit codes](#exit-codes)

## Versioning

The `schemaVersion` major number changes for breaking wire-format changes.
Consumers must tolerate additive fields within a major version.

## Core fields

- `score.value`: display score from 0 through 10.
- `score.raw`: deterministic integer score used for calibration.
- `score.verdict`: playful, professional summary; not a certification.
- `confidence`: `low`, `medium`, or `high`, based on evidence and native DRC.
- `findings`: stable IDs with severity, evidence, remediation, location, source.
- `nativeDrc`: execution status, tool version, finding count, and note.
- `limitations` and `disclaimer`: mandatory report qualifications.

Generate the authoritative JSON Schema with `ratemypcb schema`.

## Coverage

- `passed`: the described check ran and found no attention condition.
- `attention`: the check ran and produced evidence requiring review.
- `not_run`: required evidence or an optional engine was unavailable; no pass is
  claimed.
- `not_provided`: the input artifact was absent.

## Exit codes

- `0`: review completed and no configured severity threshold was met.
- `1`: review completed but `--fail-on` threshold was met.
- `2`: invalid or ambiguous user input.
- `3`: internal or optional-native execution failure.
