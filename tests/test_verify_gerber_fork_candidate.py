#!/usr/bin/env python3
"""Mutation checks for the Gerber fork authority verifier."""

from __future__ import annotations

import hashlib
import json
import tempfile
from collections.abc import Callable
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any
from unittest.mock import patch

import verify_gerber_fork_candidate as verifier
from verify_gerber_fork_candidate import (
    FORK_REF,
    FORK_URL,
    PATCH_SHA256,
    PRODUCTION_SHA,
    RELEASE_SHA,
    SOURCE_SHA256,
    TEST_SHA256,
    TOKENIZER_SHA,
    ValidationError,
    validate_candidate,
)

HEAD = PRODUCTION_SHA
TREE = "2" * 40
ARCHIVE = "3" * 64


def timestamp(offset: timedelta = timedelta()) -> str:
    value = datetime.now(timezone.utc) + offset
    return value.replace(microsecond=0).isoformat().replace("+00:00", "Z")


def baseline() -> tuple[dict[str, Any], dict[str, Any], dict[str, Any]]:
    candidate: dict[str, Any] = {
        "schema_version": 1,
        "generated_at_utc": timestamp(timedelta(minutes=-2)),
        "fork_url": FORK_URL,
        "fork_ref": FORK_REF,
        "release_sha": RELEASE_SHA,
        "tokenizer_sha": TOKENIZER_SHA,
        "parent_sha": TOKENIZER_SHA,
        "head_sha": HEAD,
        "tree_sha": TREE,
        "archive_sha256": ARCHIVE,
        "recommendation": "PASS",
        "branch_protected": True,
        "branch_locked": True,
        "required_signatures": True,
        "commit_signature_valid": True,
        "governance": {
            "enforce_admins": True,
            "required_linear_history": True,
            "allow_force_pushes": False,
            "allow_deletions": False,
        },
        "changed_files": ["src/parser.rs", "tests/component_tests.rs"],
        "patch": {
            "diff_sha256": PATCH_SHA256,
            "source_sha256": SOURCE_SHA256,
            "test_sha256": TEST_SHA256,
            "public_api_changed": False,
        },
    }
    review: dict[str, Any] = {
        "schema_version": 1,
        "reviewed_at_utc": timestamp(timedelta(minutes=-1)),
        "reviewer_identity": "independent-test-reviewer",
        "independent": True,
        "recommendation": "ACCEPT",
        "findings": {"P0": [], "P1": [], "P2": [], "P3": []},
        **{
            key: candidate[key]
            for key in (
                "fork_url",
                "fork_ref",
                "release_sha",
                "tokenizer_sha",
                "head_sha",
                "tree_sha",
                "archive_sha256",
            )
        },
    }
    probe = {
        "remote_ref": FORK_REF,
        "remote_head": HEAD,
        "verified_head": HEAD,
        "tree_sha": TREE,
        "parent_sha": TOKENIZER_SHA,
        "changed_files": ["src/parser.rs", "tests/component_tests.rs"],
        "patch_sha256": PATCH_SHA256,
        "source_sha256": SOURCE_SHA256,
        "test_sha256": TEST_SHA256,
        "archive_sha256": ARCHIVE,
        "branch_protected": True,
        "branch_locked": True,
        "required_signatures": True,
        "commit_signature_valid": True,
        "enforce_admins": True,
        "required_linear_history": True,
        "allow_force_pushes": False,
        "allow_deletions": False,
        "cargo_placement": "dev",
        "cargo_head": HEAD,
    }
    return candidate, review, probe


def write_case(
    root: Path, candidate: dict[str, Any], review: dict[str, Any]
) -> tuple[Path, Path]:
    phase = (
        root / ".planning/phases/05-manufacturing-evidence-model-and-gerber-baseline"
    )
    phase.mkdir(parents=True, exist_ok=True)
    candidate_path = phase / "05-02-FORK-CANDIDATE.json"
    review_path = phase / "05-02-FORK-REVIEW.json"
    candidate_path.write_text(
        json.dumps(candidate, sort_keys=True, separators=(",", ":")) + "\n"
    )
    review["candidate_sha256"] = hashlib.sha256(candidate_path.read_bytes()).hexdigest()
    review_path.write_text(
        json.dumps(review, sort_keys=True, separators=(",", ":")) + "\n"
    )
    return candidate_path, review_path


def expect_failure(
    mutate: Callable[
        [dict[str, Any], dict[str, Any], dict[str, Any], Path, Path], None
    ],
) -> None:
    candidate, review, probe = baseline()
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        candidate_path, review_path = write_case(root, candidate, review)
        mutate(candidate, review, probe, candidate_path, review_path)
        try:
            validate_candidate(root, probe)
        except ValidationError:
            return
        raise AssertionError("authority mutation unexpectedly passed")


def assert_read_only_probe_has_no_mutating_git_command() -> None:
    calls: list[tuple[str, ...]] = []

    def fake_run(
        *args: str, cwd: Path | None = None, binary: bool = False
    ) -> str | bytes:
        del cwd
        calls.append(args)
        if args[:2] == ("git", "ls-remote"):
            value: str | bytes = f"{HEAD}\t{FORK_REF}\n"
        elif args[:3] == ("git", "rev-parse", f"{HEAD}^{{commit}}"):
            value = f"{HEAD}\n"
        elif args[:3] == ("git", "rev-parse", f"{HEAD}^{{tree}}"):
            value = f"{TREE}\n"
        elif args[:3] == ("git", "rev-parse", f"{HEAD}^"):
            value = f"{TOKENIZER_SHA}\n"
        elif args[:3] == ("git", "diff", "--name-only"):
            value = "src/parser.rs\ntests/component_tests.rs\n"
        elif args[:2] == ("git", "diff"):
            value = b"patch"
        elif args[:2] == ("git", "show"):
            value = b"source"
        elif args[:2] == ("git", "archive"):
            value = b"archive"
        elif args[:2] == ("gh", "api"):
            value = json.dumps(
                {
                    "lock_branch": {"enabled": True},
                    "required_signatures": {"enabled": True},
                    "enforce_admins": {"enabled": True},
                    "required_linear_history": {"enabled": True},
                    "allow_force_pushes": {"enabled": False},
                    "allow_deletions": {"enabled": False},
                }
            )
        else:
            value = b"" if binary else ""
        return value

    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory) / "ratemypcb"
        (root / "crates/ratemypcb-core").mkdir(parents=True)
        root.parent.joinpath("gerber-parser").mkdir()
        root.joinpath("Cargo.toml").write_text("[workspace]\n")
        root.joinpath("crates/ratemypcb-core/Cargo.toml").write_text(
            f'[dependencies]\ngerber_parser = {{ git = "{FORK_URL}", rev = "{HEAD}" }}\n'
        )
        root.joinpath("Cargo.lock").write_text(
            "[[package]]\n"
            'name = "gerber_parser"\n'
            f'source = "git+{FORK_URL}?rev={HEAD}#{HEAD}"\n'
        )
        with patch.object(verifier, "run", fake_run):
            observed = verifier.read_only_probe({"head_sha": HEAD}, root)
        assert observed["verified_head"] == HEAD

    mutating = {
        "fetch",
        "update-ref",
        "checkout",
        "switch",
        "reset",
        "worktree",
        "branch",
        "tag",
        "merge",
        "rebase",
        "commit",
    }
    assert not [args for args in calls if args[0] == "git" and args[1] in mutating]


def main() -> None:
    assert_read_only_probe_has_no_mutating_git_command()
    candidate, review, probe = baseline()
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        write_case(root, candidate, review)
        validate_candidate(root, probe)

    def duplicate_key(
        candidate: dict[str, Any],
        review: dict[str, Any],
        probe: dict[str, str],
        candidate_path: Path,
        review_path: Path,
    ) -> None:
        del candidate, review, probe, review_path
        candidate_path.write_text('{"schema_version":1,"schema_version":1}\n')

    def stale_timestamp(
        candidate: dict[str, Any],
        review: dict[str, Any],
        probe: dict[str, str],
        candidate_path: Path,
        review_path: Path,
    ) -> None:
        del probe
        candidate["generated_at_utc"] = timestamp(timedelta(days=-8))
        write_case(candidate_path.parents[3], candidate, review)
        del review_path

    def packet_hash_drift(
        candidate: dict[str, Any],
        review: dict[str, Any],
        probe: dict[str, str],
        candidate_path: Path,
        review_path: Path,
    ) -> None:
        del candidate, probe, candidate_path
        review["candidate_sha256"] = "0" * 64
        review_path.write_text(json.dumps(review, sort_keys=True) + "\n")

    def wrong_production_head(
        candidate: dict[str, Any],
        review: dict[str, Any],
        probe: dict[str, str],
        candidate_path: Path,
        review_path: Path,
    ) -> None:
        del probe, review_path
        candidate["head_sha"] = "1" * 40
        write_case(candidate_path.parents[3], candidate, review)

    def severity_finding(
        severity: str,
    ) -> Callable[[dict[str, Any], dict[str, Any], dict[str, str], Path, Path], None]:
        def mutate(
            candidate: dict[str, Any],
            review: dict[str, Any],
            probe: dict[str, str],
            candidate_path: Path,
            review_path: Path,
        ) -> None:
            del candidate, probe, candidate_path
            review["findings"][severity] = ["blocker"]
            review_path.write_text(json.dumps(review, sort_keys=True) + "\n")

        return mutate

    def wrong_remote_ref(
        candidate: dict[str, Any],
        review: dict[str, Any],
        probe: dict[str, str],
        candidate_path: Path,
        review_path: Path,
    ) -> None:
        del candidate, review, candidate_path, review_path
        probe["remote_ref"] = "refs/heads/wrong"

    def wrong_tree(
        candidate: dict[str, Any],
        review: dict[str, Any],
        probe: dict[str, str],
        candidate_path: Path,
        review_path: Path,
    ) -> None:
        del candidate, review, candidate_path, review_path
        probe["tree_sha"] = "4" * 40

    def wrong_cargo_placement(
        candidate: dict[str, Any],
        review: dict[str, Any],
        probe: dict[str, str],
        candidate_path: Path,
        review_path: Path,
    ) -> None:
        del candidate, review, candidate_path, review_path
        probe["cargo_placement"] = "normal"

    def missing_branch_protection(
        candidate: dict[str, Any],
        review: dict[str, Any],
        probe: dict[str, Any],
        candidate_path: Path,
        review_path: Path,
    ) -> None:
        del candidate, review, candidate_path, review_path
        probe["branch_protected"] = False

    def force_push_enabled(
        candidate: dict[str, Any],
        review: dict[str, Any],
        probe: dict[str, Any],
        candidate_path: Path,
        review_path: Path,
    ) -> None:
        del candidate, review, candidate_path, review_path
        probe["allow_force_pushes"] = True

    for mutation in (
        duplicate_key,
        stale_timestamp,
        packet_hash_drift,
        wrong_production_head,
        severity_finding("P0"),
        severity_finding("P1"),
        severity_finding("P2"),
        wrong_remote_ref,
        wrong_tree,
        wrong_cargo_placement,
        missing_branch_protection,
        force_push_enabled,
    ):
        expect_failure(mutation)


if __name__ == "__main__":
    main()
