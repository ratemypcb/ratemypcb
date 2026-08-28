#!/usr/bin/env python3
"""Verify Phase 5 summary frontmatter and independent-review JSON."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
REQUIREMENTS = ROOT / ".planning/REQUIREMENTS.md"
REQUIREMENT_ID = re.compile(r"^[A-Z][A-Z0-9]*-[0-9]{2}$")


class ValidationError(Exception):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValidationError(message)


def unique_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        require(key not in result, f"duplicate JSON key: {key}")
        result[key] = value
    return result


def parse_frontmatter(path: Path) -> dict[str, str]:
    lines = path.read_text().splitlines()
    require(bool(lines) and lines[0] == "---", f"{path} has no leading frontmatter")
    try:
        end = lines.index("---", 1)
    except ValueError as error:
        raise ValidationError(f"{path} has unclosed frontmatter") from error
    result: dict[str, str] = {}
    for line in lines[1:end]:
        require(
            bool(line) and not line[0].isspace(),
            f"unsupported nested frontmatter: {line}",
        )
        require(":" in line, f"invalid frontmatter line: {line}")
        key, value = line.split(":", 1)
        key, value = key.strip(), value.strip()
        require(bool(key) and key not in result, f"duplicate frontmatter key: {key}")
        result[key] = value
    return result


def parse_requirement_list(value: str) -> list[str]:
    require(
        value.startswith("[") and value.endswith("]"),
        "requirements_completed must be a bracket array",
    )
    body = value[1:-1].strip()
    if not body:
        return []
    result = [item.strip() for item in body.split(",")]
    require(
        all(REQUIREMENT_ID.fullmatch(item) for item in result), "invalid requirement ID"
    )
    require(len(result) == len(set(result)), "duplicate completed requirement")
    return result


def expected_requirements(value: str) -> list[str]:
    if not value:
        return []
    result = value.split(",")
    require(
        all(REQUIREMENT_ID.fullmatch(item) for item in result),
        "invalid expected requirement ID",
    )
    require(len(result) == len(set(result)), "duplicate expected requirement")
    return result


def verify_requirement_rows(requirements: list[str]) -> None:
    text = REQUIREMENTS.read_text()
    for requirement in requirements:
        definition = re.findall(
            rf"^- \[x\] \*\*{re.escape(requirement)}\*\*:", text, re.MULTILINE
        )
        trace = re.findall(
            rf"^\| {re.escape(requirement)} \| Phase 5 \| Complete \|$",
            text,
            re.MULTILINE,
        )
        require(
            len(definition) == 1,
            f"{requirement} definition is not exactly checked once",
        )
        require(
            len(trace) == 1,
            f"{requirement} traceability row is not exactly Complete once",
        )


def verify_summary(
    path: Path, status: str, decision: str | None, requirements: str
) -> None:
    frontmatter = parse_frontmatter(path)
    require(frontmatter.get("status") == status, "summary status mismatch")
    if decision is not None:
        require(frontmatter.get("decision") == decision, "summary decision mismatch")
    expected = expected_requirements(requirements)
    actual = parse_requirement_list(frontmatter.get("requirements_completed", ""))
    require(actual == expected, "summary requirements_completed mismatch")
    verify_requirement_rows(actual)


def verify_review(path: Path, recommendation: str) -> None:
    value = json.loads(path.read_text(), object_pairs_hook=unique_object)
    require(isinstance(value, dict), "review must be a JSON object")
    require(
        value.get("recommendation") == recommendation, "review recommendation mismatch"
    )
    require(value.get("independent") is True, "review is not independent")
    require(bool(value.get("reviewer_identity")), "reviewer identity missing")
    findings = value.get("findings")
    require(isinstance(findings, dict), "review findings must be an object")
    for severity in ("P0", "P1", "P2"):
        require(findings.get(severity) == [], f"review has {severity} findings")


def main() -> int:
    parser = argparse.ArgumentParser()
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--summary", type=Path)
    source.add_argument("--review", type=Path)
    parser.add_argument("--status")
    parser.add_argument("--decision")
    parser.add_argument("--requirements", default="")
    parser.add_argument("--recommendation")
    args = parser.parse_args()
    try:
        if args.summary:
            require(bool(args.status), "--status is required with --summary")
            verify_summary(args.summary, args.status, args.decision, args.requirements)
        else:
            require(
                bool(args.recommendation), "--recommendation is required with --review"
            )
            verify_review(args.review, args.recommendation)
    except (ValidationError, OSError, UnicodeError, json.JSONDecodeError) as error:
        print(f"Phase 5 artifact verification failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
