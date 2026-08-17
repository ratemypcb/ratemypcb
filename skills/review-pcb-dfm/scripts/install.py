#!/usr/bin/env python3
"""Install a verified RateMyPCB release binary into a user-local directory."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import stat
import tarfile
import tempfile
import urllib.error
import urllib.request
import zipfile


REPOSITORY = "ratemypcb/ratemypcb"
API = f"https://api.github.com/repos/{REPOSITORY}"
DOWNLOADS = f"https://github.com/{REPOSITORY}/releases/download"
USER_AGENT = "ratemypcb-skill-installer/1"


def platform_target() -> tuple[str, str]:
    system = platform.system().lower()
    machine = platform.machine().lower()
    architectures = {
        "x86_64": "x86_64",
        "amd64": "x86_64",
        "aarch64": "aarch64",
        "arm64": "aarch64",
    }
    architecture = architectures.get(machine)
    systems = {
        "linux": "unknown-linux-gnu",
        "darwin": "apple-darwin",
        "windows": "pc-windows-msvc",
    }
    suffix = systems.get(system)
    if not architecture or not suffix or (system == "windows" and architecture != "x86_64"):
        raise RuntimeError(f"Unsupported platform: {system or 'unknown'} {machine or 'unknown'}")
    return f"{architecture}-{suffix}", system


def default_destination(system: str) -> Path:
    if system == "windows":
        base = Path(os.environ.get("LOCALAPPDATA", Path.home() / "AppData" / "Local"))
        return base / "RateMyPCB" / "bin" / "ratemypcb.exe"
    return Path.home() / ".local" / "bin" / "ratemypcb"


def request(url: str) -> bytes:
    try:
        with urllib.request.urlopen(
            urllib.request.Request(url, headers={"User-Agent": USER_AGENT}), timeout=30
        ) as response:
            return response.read()
    except (urllib.error.URLError, TimeoutError) as error:
        raise RuntimeError(f"Download failed for {url}: {error}") from error


def resolve_version(version: str) -> str:
    if version != "latest":
        return version if version.startswith("v") else f"v{version}"
    try:
        payload = json.loads(request(f"{API}/releases/latest"))
        tag = str(payload["tag_name"])
    except (ValueError, KeyError, TypeError) as error:
        raise RuntimeError("GitHub returned invalid latest-release metadata.") from error
    if not tag.startswith("v"):
        raise RuntimeError(f"Unexpected release tag: {tag}")
    return tag


def expected_hash(checksum: bytes, asset: str) -> str:
    fields = checksum.decode("utf-8").strip().split()
    if not fields or len(fields[0]) != 64 or any(character not in "0123456789abcdefABCDEF" for character in fields[0]):
        raise RuntimeError(f"Invalid checksum file for {asset}.")
    if len(fields) > 1 and fields[-1].lstrip("*") != asset:
        raise RuntimeError(f"Checksum filename does not match {asset}.")
    return fields[0].lower()


def extract_binary(archive: Path, system: str, output: Path) -> None:
    member = "ratemypcb.exe" if system == "windows" else "ratemypcb"
    if system == "windows":
        with zipfile.ZipFile(archive) as package:
            names = [name for name in package.namelist() if Path(name).name == member]
            if len(names) != 1:
                raise RuntimeError("Release archive does not contain exactly one RateMyPCB binary.")
            with package.open(names[0]) as source, output.open("wb") as destination:
                shutil.copyfileobj(source, destination)
    else:
        with tarfile.open(archive, "r:gz") as package:
            members = [item for item in package.getmembers() if item.isfile() and Path(item.name).name == member]
            if len(members) != 1:
                raise RuntimeError("Release archive does not contain exactly one RateMyPCB binary.")
            source = package.extractfile(members[0])
            if source is None:
                raise RuntimeError("Could not read the RateMyPCB binary from its release archive.")
            with source, output.open("wb") as destination:
                shutil.copyfileobj(source, destination)


def install(version: str, destination: Path, dry_run: bool) -> None:
    target, system = platform_target()
    tag = resolve_version(version) if not dry_run or version == "latest" else (version if version.startswith("v") else f"v{version}")
    extension = "zip" if system == "windows" else "tar.gz"
    asset = f"ratemypcb-{tag}-{target}.{extension}"
    url = f"{DOWNLOADS}/{tag}/{asset}"
    checksum_url = f"{url}.sha256"
    if dry_run:
        print(f"target={target}")
        print(f"asset={url}")
        print(f"checksum={checksum_url}")
        print(f"destination={destination}")
        return

    archive_bytes = request(url)
    wanted = expected_hash(request(checksum_url), asset)
    actual = hashlib.sha256(archive_bytes).hexdigest()
    if actual != wanted:
        raise RuntimeError(f"SHA-256 mismatch for {asset}; refusing to install it.")

    destination = destination.expanduser().resolve()
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="ratemypcb-install-") as temporary:
        temporary_path = Path(temporary)
        archive = temporary_path / asset
        archive.write_bytes(archive_bytes)
        candidate = temporary_path / destination.name
        extract_binary(archive, system, candidate)
        if system != "windows":
            candidate.chmod(candidate.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
        with tempfile.NamedTemporaryFile(
            prefix=f".{destination.name}-", suffix=".tmp", dir=destination.parent, delete=False
        ) as staged_file:
            staged = Path(staged_file.name)
            with candidate.open("rb") as source:
                shutil.copyfileobj(source, staged_file)
        try:
            if system != "windows":
                staged.chmod(candidate.stat().st_mode)
            os.replace(staged, destination)
        finally:
            staged.unlink(missing_ok=True)
    print(f"Installed RateMyPCB {tag} at {destination}")
    if str(destination.parent) not in os.environ.get("PATH", "").split(os.pathsep):
        print(f"Add {destination.parent} to PATH, or invoke the binary by its full path.")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", default="latest", help="release tag, such as v0.1.0 (default: latest)")
    parser.add_argument("--destination", type=Path, help="full destination path for the executable")
    parser.add_argument("--dry-run", action="store_true", help="print resolved paths without downloading")
    arguments = parser.parse_args()
    _, system = platform_target()
    destination = arguments.destination or default_destination(system)
    try:
        install(arguments.version, destination, arguments.dry_run)
    except RuntimeError as error:
        parser.exit(1, f"ratemypcb installer: {error}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
