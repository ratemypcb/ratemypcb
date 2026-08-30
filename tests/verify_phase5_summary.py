#!/usr/bin/env python3
"""Verify Phase 5 summary frontmatter and completed requirement rows."""

from __future__ import annotations

import argparse
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REQUIREMENTS = ROOT / ".planning/REQUIREMENTS.md"
REQUIREMENT_ID = re.compile(r"^[A-Z][A-Z0-9]*-[0-9]{2}$")


class ValidationError(ValueError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValidationError(message)


def parse_frontmatter(path: Path) -> dict[str, str]:
    lines = path.read_text().splitlines()
    require(bool(lines) and lines[0] == "---", "summary frontmatter missing")
    result: dict[str, str] = {}
    for line in lines[1:]:
        if line == "---":
            return result
        require(":" in line, f"invalid frontmatter line: {line}")
        key, value = line.split(":", 1)
        key = key.strip()
        require(bool(key) and key not in result, f"duplicate frontmatter key: {key}")
        result[key] = value.strip()
    raise ValidationError("unterminated summary frontmatter")


def parse_requirements(value: str) -> list[str]:
    require(value.startswith("[") and value.endswith("]"), "invalid requirements list")
    body = value[1:-1].strip()
    if not body:
        return []
    values = [item.strip() for item in body.split(",")]
    require(
        all(REQUIREMENT_ID.fullmatch(item) for item in values), "invalid requirement id"
    )
    require(len(values) == len(set(values)), "duplicate requirement id")
    return values


def verify_requirement_rows(requirements: list[str]) -> None:
    text = REQUIREMENTS.read_text()
    for requirement in requirements:
        require(
            re.search(
                rf"^- \[x\] \*\*{re.escape(requirement)}\*\*:", text, re.MULTILINE
            )
            is not None,
            f"requirement is not checked: {requirement}",
        )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--summary", type=Path, required=True)
    parser.add_argument("--status", required=True)
    parser.add_argument("--decision")
    parser.add_argument("--requirements", default="")
    args = parser.parse_args()

    frontmatter = parse_frontmatter(args.summary)
    require(frontmatter.get("status") == args.status, "summary status mismatch")
    if args.decision is not None:
        require(
            frontmatter.get("decision") == args.decision, "summary decision mismatch"
        )
    expected = [item for item in args.requirements.split(",") if item]
    actual = parse_requirements(frontmatter.get("requirements_completed", ""))
    require(actual == expected, "summary requirements_completed mismatch")
    verify_requirement_rows(actual)


if __name__ == "__main__":
    main()
