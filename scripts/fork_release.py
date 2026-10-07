#!/usr/bin/env python3
"""Prepare and verify Herdr Houston release inputs without publishing them."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import tomllib
from pathlib import Path

NAMESPACE = "herdr-houston"
TAGLINE = "Houston, we have a pane"
ASSETS = tuple(
    f"{NAMESPACE}-{platform}"
    for platform in (
        "linux-x86_64", "linux-aarch64", "macos-x86_64", "macos-aarch64", "windows-x86_64.zip"
    )
)


def prepare(tag: str, cargo: Path, commit: str) -> dict[str, str]:
    match = re.fullmatch(r"houston-v(\d+\.\d+\.\d+)-([1-9]\d*)", tag)
    if match is None:
        raise ValueError("expected houston-v<package-version>-<positive-build-number>")
    base, build = match.groups()
    package_version = tomllib.loads(cargo.read_text(encoding="utf-8"))["package"]["version"]
    if base != package_version:
        raise ValueError("fork tag base must equal Cargo.toml package version")
    if re.fullmatch(r"[0-9a-f]{40}", commit) is None:
        raise ValueError("expected a full source commit SHA")
    return {
        "tag": tag, "base_version": base, "build_id": build,
        "version": f"{base}-houston.{build}", "namespace": NAMESPACE,
        "commit": commit, "tagline": TAGLINE, "repository": "ravan/herdr",
    }


def bundle(artifacts: Path, output: Path, metadata: dict[str, str]) -> None:
    verified = []
    for name in ASSETS:
        source = artifacts / name / name
        info = json.loads((artifacts / name / f"{name}.build.json").read_text(encoding="utf-8"))
        if info != metadata:
            raise ValueError(f"artifact provenance differs for {name}")
        if not source.is_file() or source.stat().st_size == 0:
            raise ValueError(f"missing or empty artifact: {name}")
        digest = hashlib.sha256(source.read_bytes()).hexdigest()
        expected = (artifacts / name / f"{name}.sha256").read_text(encoding="ascii").strip()
        if expected != f"{digest}  {name}":
            raise ValueError(f"artifact checksum differs for {name}")
        verified.append((source, name, digest))
    output.mkdir(parents=True, exist_ok=False)
    for source, name, _ in verified:
        shutil.copy2(source, output / name)
    (output / "SHA256SUMS").write_text(
        "".join(f"{digest}  {name}\n" for _, name, digest in verified), encoding="ascii"
    )
    (output / "FORK_BUILD.json").write_text(
        json.dumps({**metadata, "sha256": {name: digest for _, name, digest in verified}}, indent=2) + "\n",
        encoding="utf-8",
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("prepare", "bundle", "checksum"))
    parser.add_argument("--tag", required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--cargo", type=Path, default=Path("Cargo.toml"))
    parser.add_argument("--github-output", type=Path)
    parser.add_argument("--metadata", type=Path)
    parser.add_argument("--notes", type=Path)
    parser.add_argument("--asset", type=Path)
    parser.add_argument("--artifacts", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    try:
        metadata = prepare(args.tag, args.cargo, args.commit)
        if args.github_output:
            with args.github_output.open("a", encoding="utf-8") as destination:
                destination.write("".join(f"{key}={value}\n" for key, value in metadata.items()))
        if args.metadata:
            args.metadata.write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
        if args.notes:
            args.notes.write_text(
                f"**{TAGLINE}**\n\n"
                f"Herdr Houston `{metadata['version']}` is Ravan's fork with collections, missions, and Mission control.\n\n"
                f"Source: `{metadata['commit']}`. Install as `herdr-houston` (Windows: `herdr-houston.exe`). "
                "It uses separate config/state directories. Upstream self-update and automatic SSH installation are disabled.\n\n"
                "Download the asset for your platform and verify it against `SHA256SUMS`. "
                "Extract the complete Windows ZIP to keep its ConPTY runtime beside the executable.\n",
                encoding="utf-8",
            )
        if args.command == "checksum":
            if args.asset is None or args.asset.name not in ASSETS:
                raise ValueError("expected one of the five Houston release assets")
            digest = hashlib.sha256(args.asset.read_bytes()).hexdigest()
            args.asset.with_name(args.asset.name + ".sha256").write_text(
                f"{digest}  {args.asset.name}\n", encoding="ascii"
            )
        elif args.command == "bundle":
            if args.artifacts is None or args.output is None:
                raise ValueError("bundle requires --artifacts and --output")
            bundle(args.artifacts, args.output, metadata)
    except (ValueError, OSError, KeyError) as error:
        parser.error(str(error))


if __name__ == "__main__":
    main()
