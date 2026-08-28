#!/usr/bin/env python3
"""Fail-closed authority checks for the pinned RateMyPCB Gerber parser fork."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any
from urllib.parse import quote

ROOT = Path(__file__).resolve().parents[1]
PHASE = ROOT / ".planning/phases/05-manufacturing-evidence-model-and-gerber-baseline"
CANDIDATE = PHASE / "05-02-FORK-CANDIDATE.json"
REVIEW = PHASE / "05-02-FORK-REVIEW.json"
DECISION = PHASE / "05-02-FORK-DECISION.json"
ATTESTATION = PHASE / "05-03-PRECLEAN-ATTESTATION.json"
FORK_URL = "https://github.com/ratemypcb/gerber-parser.git"
FORK_REF = "refs/heads/ratemypcb/gerber-parser-accounting-fix"
RELEASE_SHA = "8a07cc6064894cbf63978012969af5c1f656a30b"
TOKENIZER_SHA = "f4160c7c6ca1b4cdd9c5273a3916b4fd087b5e34"
PRODUCTION_SHA = "54004bc52c11699b49cd287a49135380feee86b3"
PATCH_SHA256 = "f9c64bbabd9731ccb68ce8708c64048fe8ae4fe7aff20931062504448d3c1787"
SOURCE_SHA256 = "170003700d3fe343667e00b4c7ad225ccb0b71f6c5a35fb170cc5a128080f366"
TEST_SHA256 = "29bb344de8ff7e6c741b861479d11a3891802aa5f320a6cd4d4d801941fa6980"
CHANGED_FILES = ["src/parser.rs", "tests/component_tests.rs"]
IDENTITY_KEYS = (
    "fork_url",
    "fork_ref",
    "release_sha",
    "tokenizer_sha",
    "head_sha",
    "tree_sha",
    "archive_sha256",
)


class ValidationError(Exception):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValidationError(message)


def unique_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ValidationError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(), object_pairs_hook=unique_object)
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise ValidationError(f"cannot read {path}: {error}") from error
    require(isinstance(value, dict), f"{path} must contain a JSON object")
    return value


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def parse_time(value: Any, field: str) -> datetime:
    require(
        isinstance(value, str) and value.endswith("Z"), f"{field} must be UTC RFC3339"
    )
    try:
        parsed = datetime.fromisoformat(value[:-1] + "+00:00")
    except ValueError as error:
        raise ValidationError(f"invalid {field}: {value}") from error
    now = datetime.now(timezone.utc)
    require(parsed <= now + timedelta(minutes=5), f"{field} is in the future")
    require(parsed >= now - timedelta(days=7), f"{field} is stale")
    return parsed


def require_hex(value: Any, length: int, field: str) -> str:
    require(
        isinstance(value, str)
        and len(value) == length
        and all(character in "0123456789abcdef" for character in value),
        f"{field} must be {length} lowercase hex characters",
    )
    return value


def run(*args: str, cwd: Path | None = None, binary: bool = False) -> str | bytes:
    completed = subprocess.run(
        args,
        cwd=cwd,
        check=True,
        capture_output=True,
        text=not binary,
    )
    return completed.stdout


def cargo_probe(root: Path, head_sha: str) -> dict[str, str]:
    core_text = (root / "crates/ratemypcb-core/Cargo.toml").read_text()
    declarations: dict[str, str] = {}
    section = ""
    for raw_line in core_text.splitlines():
        line = raw_line.strip()
        if line.startswith("[") and line.endswith("]"):
            section = line
        elif line.startswith("gerber_parser"):
            require(
                section in ("[dependencies]", "[dev-dependencies]"),
                "parser declaration is in an invalid section",
            )
            require(
                section not in declarations,
                f"duplicate parser declaration in {section}",
            )
            declarations[section] = line
    require(
        "gerber_parser" not in (root / "Cargo.toml").read_text(),
        "root workspace declares gerber_parser",
    )

    lock_text = (root / "Cargo.lock").read_text()
    locked_packages = [
        package
        for package in lock_text.split("[[package]]")
        if 'name = "gerber_parser"' in package
    ]
    if not declarations:
        require(not locked_packages, "Cargo.lock retains gerber_parser after cleanup")
        return {"cargo_placement": "absent", "cargo_head": ""}

    require(
        len(declarations) == 1, "gerber_parser cannot be both normal and dev dependency"
    )
    expected = f'gerber_parser = {{ git = "{FORK_URL}", rev = "{head_sha}" }}'
    declaration = next(iter(declarations.values()))
    require(
        declaration == expected,
        "Cargo dependency must contain only exact fork git URL and full rev",
    )
    require(
        len(locked_packages) == 1,
        "Cargo.lock must contain exactly one gerber_parser package",
    )
    source_lines = [
        line.strip()
        for line in locked_packages[0].splitlines()
        if line.strip().startswith("source =")
    ]
    require(len(source_lines) == 1, "locked parser must have one source")
    expected_source = f'source = "git+{FORK_URL}?rev={head_sha}#{head_sha}"'
    require(
        source_lines[0] == expected_source, "Cargo.lock source URL/rev/SHA mismatch"
    )
    return {
        "cargo_placement": "normal" if "[dependencies]" in declarations else "dev",
        "cargo_head": head_sha,
    }


def read_only_probe(candidate: dict[str, Any], root: Path = ROOT) -> dict[str, Any]:
    sibling = root.parent / "gerber-parser"
    head_sha = candidate["head_sha"]
    remote = str(run("git", "ls-remote", FORK_URL, FORK_REF)).strip().splitlines()
    require(len(remote) == 1, "exact fork ref must resolve exactly once")
    remote_head, remote_ref = remote[0].split()
    require(remote_ref == FORK_REF, "remote ref mismatch")
    run("git", "cat-file", "-e", f"{head_sha}^{{commit}}", cwd=sibling)
    verified_head = str(
        run("git", "rev-parse", f"{head_sha}^{{commit}}", cwd=sibling)
    ).strip()
    tree_sha = str(
        run("git", "rev-parse", f"{verified_head}^{{tree}}", cwd=sibling)
    ).strip()
    parent_sha = str(run("git", "rev-parse", f"{verified_head}^", cwd=sibling)).strip()
    changed_files_text = run(
        "git", "diff", "--name-only", RELEASE_SHA, verified_head, cwd=sibling
    )
    assert isinstance(changed_files_text, str)
    changed_files = sorted(changed_files_text.splitlines())
    patch = run(
        "git",
        "diff",
        "--binary",
        TOKENIZER_SHA,
        verified_head,
        "--",
        *CHANGED_FILES,
        cwd=sibling,
        binary=True,
    )
    source = run(
        "git", "show", f"{verified_head}:src/parser.rs", cwd=sibling, binary=True
    )
    tests = run(
        "git",
        "show",
        f"{verified_head}:tests/component_tests.rs",
        cwd=sibling,
        binary=True,
    )
    assert isinstance(patch, bytes)
    assert isinstance(source, bytes)
    assert isinstance(tests, bytes)
    archive = run(
        "git",
        "archive",
        "--format=tar",
        f"--prefix=gerber-parser-{head_sha}/",
        head_sha,
        cwd=sibling,
        binary=True,
    )
    assert isinstance(archive, bytes)
    run("git", "verify-commit", verified_head, cwd=sibling)
    branch = remote_ref.removeprefix("refs/heads/")
    endpoint = (
        f"repos/ratemypcb/gerber-parser/branches/{quote(branch, safe='')}/protection"
    )
    protection_text = run("gh", "api", endpoint)
    assert isinstance(protection_text, str)
    protection = json.loads(protection_text, object_pairs_hook=unique_object)
    require(isinstance(protection, dict), "invalid branch protection response")
    return {
        "remote_ref": remote_ref,
        "remote_head": remote_head,
        "verified_head": verified_head,
        "tree_sha": tree_sha,
        "parent_sha": parent_sha,
        "changed_files": changed_files,
        "patch_sha256": hashlib.sha256(patch).hexdigest(),
        "source_sha256": hashlib.sha256(source).hexdigest(),
        "test_sha256": hashlib.sha256(tests).hexdigest(),
        "archive_sha256": hashlib.sha256(archive).hexdigest(),
        "branch_protected": True,
        "branch_locked": protection.get("lock_branch", {}).get("enabled") is True,
        "required_signatures": protection.get("required_signatures", {}).get("enabled")
        is True,
        "enforce_admins": protection.get("enforce_admins", {}).get("enabled") is True,
        "required_linear_history": protection.get("required_linear_history", {}).get(
            "enabled"
        )
        is True,
        "allow_force_pushes": protection.get("allow_force_pushes", {}).get("enabled"),
        "allow_deletions": protection.get("allow_deletions", {}).get("enabled"),
        "commit_signature_valid": True,
        **cargo_probe(root, head_sha),
    }


def validate_candidate(
    root: Path = ROOT,
    probe: dict[str, Any] | None = None,
    expected_placement: str = "dev",
) -> tuple[dict[str, Any], dict[str, Any]]:
    candidate = load_json(root / CANDIDATE.relative_to(ROOT))
    review = load_json(root / REVIEW.relative_to(ROOT))

    require(candidate.get("schema_version") == 1, "candidate schema_version must be 1")
    require(candidate.get("fork_url") == FORK_URL, "candidate fork URL mismatch")
    require(candidate.get("fork_ref") == FORK_REF, "candidate fork ref mismatch")
    require(candidate.get("release_sha") == RELEASE_SHA, "release SHA mismatch")
    require(candidate.get("tokenizer_sha") == TOKENIZER_SHA, "tokenizer SHA mismatch")
    require(
        candidate.get("parent_sha") == TOKENIZER_SHA, "candidate parent SHA mismatch"
    )
    require_hex(candidate.get("head_sha"), 40, "candidate head_sha")
    require(candidate.get("head_sha") == PRODUCTION_SHA, "production SHA mismatch")
    require(
        candidate["head_sha"] != TOKENIZER_SHA,
        "candidate must include the error-funnel commit",
    )
    require_hex(candidate.get("tree_sha"), 40, "candidate tree_sha")
    require_hex(candidate.get("archive_sha256"), 64, "candidate archive_sha256")
    require(
        candidate.get("recommendation") == "PASS",
        "candidate recommendation is not PASS",
    )
    require(
        candidate.get("branch_protected") is True, "candidate branch is not protected"
    )
    require(candidate.get("branch_locked") is True, "candidate branch is not locked")
    require(
        candidate.get("required_signatures") is True,
        "candidate branch does not require signatures",
    )
    require(
        candidate.get("commit_signature_valid") is True,
        "candidate commit signature is invalid",
    )
    governance = candidate.get("governance")
    if not isinstance(governance, dict):
        raise ValidationError("candidate governance evidence missing")
    require(governance.get("enforce_admins") is True, "candidate admins are exempt")
    require(
        governance.get("required_linear_history") is True,
        "candidate does not require linear history",
    )
    require(
        governance.get("allow_force_pushes") is False, "candidate allows force push"
    )
    require(governance.get("allow_deletions") is False, "candidate allows deletion")
    require(
        candidate.get("changed_files") == CHANGED_FILES,
        "candidate changed-file allowlist mismatch",
    )
    patch = candidate.get("patch")
    if not isinstance(patch, dict):
        raise ValidationError("candidate patch evidence missing")
    require(patch.get("diff_sha256") == PATCH_SHA256, "candidate patch hash mismatch")
    require(
        patch.get("source_sha256") == SOURCE_SHA256, "candidate source hash mismatch"
    )
    require(patch.get("test_sha256") == TEST_SHA256, "candidate test hash mismatch")
    require(
        patch.get("public_api_changed") is False, "candidate claims public API drift"
    )
    candidate_time = parse_time(candidate.get("generated_at_utc"), "generated_at_utc")

    require(review.get("schema_version") == 1, "review schema_version must be 1")
    for key in IDENTITY_KEYS:
        require(review.get(key) == candidate.get(key), f"review {key} mismatch")
    require(
        review.get("candidate_sha256") == digest(root / CANDIDATE.relative_to(ROOT)),
        "candidate hash drift",
    )
    require(bool(review.get("reviewer_identity")), "reviewer identity missing")
    require(review.get("independent") is True, "review is not independent")
    require(
        review.get("recommendation") == "ACCEPT", "review recommendation is not ACCEPT"
    )
    findings = review.get("findings")
    if not isinstance(findings, dict):
        raise ValidationError("review findings must be an object")
    for severity in ("P0", "P1", "P2"):
        require(findings.get(severity) == [], f"review has {severity} findings")
    review_time = parse_time(review.get("reviewed_at_utc"), "reviewed_at_utc")
    require(candidate_time <= review_time, "review predates candidate")

    observed = probe or read_only_probe(candidate, root)
    require(observed.get("remote_ref") == candidate["fork_ref"], "wrong remote ref")
    require(
        observed.get("remote_head") == candidate["head_sha"], "wrong remote ref head"
    )
    require(
        observed.get("verified_head") == candidate["head_sha"],
        "wrong local object head",
    )
    require(observed.get("tree_sha") == candidate["tree_sha"], "wrong local tree")
    require(observed.get("parent_sha") == TOKENIZER_SHA, "wrong local parent")
    require(
        observed.get("changed_files") == CHANGED_FILES, "wrong local changed files"
    )
    require(observed.get("patch_sha256") == PATCH_SHA256, "wrong local patch hash")
    require(observed.get("source_sha256") == SOURCE_SHA256, "wrong local source hash")
    require(observed.get("test_sha256") == TEST_SHA256, "wrong local test hash")
    require(
        observed.get("archive_sha256") == candidate["archive_sha256"],
        "archive hash mismatch",
    )
    require(observed.get("branch_protected") is True, "remote branch is not protected")
    require(observed.get("branch_locked") is True, "remote branch is not locked")
    require(
        observed.get("required_signatures") is True,
        "remote branch does not require signatures",
    )
    require(
        observed.get("commit_signature_valid") is True,
        "remote commit signature is invalid",
    )
    require(observed.get("enforce_admins") is True, "remote admins are exempt")
    require(
        observed.get("required_linear_history") is True,
        "remote branch does not require linear history",
    )
    require(
        observed.get("allow_force_pushes") is False, "remote branch allows force push"
    )
    require(observed.get("allow_deletions") is False, "remote branch allows deletion")
    require(
        observed.get("cargo_placement") == expected_placement,
        f"Cargo placement must be {expected_placement}",
    )
    expected_cargo_head = (
        "" if expected_placement == "absent" else candidate["head_sha"]
    )
    require(observed.get("cargo_head") == expected_cargo_head, "wrong Cargo head")
    return candidate, review


def validate_decision(
    root: Path = ROOT,
    probe: dict[str, Any] | None = None,
    expected_placement: str = "dev",
) -> tuple[dict[str, Any], dict[str, Any], dict[str, Any]]:
    candidate, review = validate_candidate(root, probe, expected_placement)
    decision = load_json(root / DECISION.relative_to(ROOT))
    require(decision.get("schema_version") == 1, "decision schema_version must be 1")
    require(
        decision.get("decision") in ("PASS_F416", "STOP_F416"), "unknown decision token"
    )
    require(bool(decision.get("actor")), "decision actor missing")
    for key in IDENTITY_KEYS:
        require(decision.get(key) == candidate.get(key), f"decision {key} mismatch")
    require(
        decision.get("candidate_sha256") == digest(root / CANDIDATE.relative_to(ROOT)),
        "decision candidate hash drift",
    )
    require(
        decision.get("review_sha256") == digest(root / REVIEW.relative_to(ROOT)),
        "decision review hash drift",
    )
    decided = parse_time(decision.get("decided_at_utc"), "decided_at_utc")
    reviewed = parse_time(review.get("reviewed_at_utc"), "reviewed_at_utc")
    require(reviewed <= decided, "decision predates review")
    return candidate, review, decision


def source_hashes(root: Path) -> dict[str, str]:
    source_root = root / "crates/ratemypcb-core/src"
    result: dict[str, str] = {}
    for path in sorted(source_root.rglob("*.rs")):
        require(path.is_file() and not path.is_symlink(), f"unsafe source path: {path}")
        result[path.relative_to(root).as_posix()] = digest(path)
    return result


def validate_cleaned_stop(
    root: Path = ROOT, probe: dict[str, Any] | None = None
) -> None:
    _, _, decision = validate_decision(root, probe, "absent")
    require(decision["decision"] == "STOP_F416", "cleaned-stop requires STOP_F416")
    attestation = load_json(root / ATTESTATION.relative_to(ROOT))
    require(
        attestation.get("source_hashes") == source_hashes(root),
        "production source changed after STOP",
    )
    core_text = (root / "crates/ratemypcb-core/Cargo.toml").read_text()
    lock_text = (root / "Cargo.lock").read_text()
    require(
        "gerber_parser" not in core_text, "parser remains in core manifest after STOP"
    )
    require(
        'name = "gerber_parser"' not in lock_text, "parser remains in lock after STOP"
    )
    for path in (root / "crates/ratemypcb-core/src").rglob("*.rs"):
        require(
            "gerber_parser" not in path.read_text(), f"parser import remains in {path}"
        )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--mode",
        required=True,
        choices=(
            "candidate",
            "decision",
            "production",
            "production-read-only",
            "cleaned-stop",
        ),
    )
    args = parser.parse_args()
    try:
        if args.mode == "candidate":
            validate_candidate()
        elif args.mode == "decision":
            validate_decision()
        elif args.mode in ("production", "production-read-only"):
            _, _, decision = validate_decision(expected_placement="normal")
            require(
                decision["decision"] == "PASS_F416", "production requires PASS_F416"
            )
        else:
            validate_cleaned_stop()
    except (
        ValidationError,
        OSError,
        subprocess.CalledProcessError,
        KeyError,
        ValueError,
    ) as error:
        print(f"gerber fork verification failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
