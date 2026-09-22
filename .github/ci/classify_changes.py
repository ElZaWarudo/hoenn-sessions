#!/usr/bin/env python3
"""Select CI checks conservatively from a pull request's complete Git diff."""

import argparse
import re
import subprocess
import sys
from collections.abc import Iterable
from pathlib import Path


def classify_paths(paths: Iterable[str]) -> dict[str, bool]:
    checks = {"rom": False, "rust": False, "installer": False}
    for path in paths:
        if path.startswith("docs/") or path == "README.md":
            continue
        if "/" not in path and path.startswith("LICENSE"):
            continue
        if path.startswith("installer/"):
            checks["installer"] = True
            checks["rust"] = True
        elif path.startswith(("coop/", "android/", "deploy/")) or path in (
            "Cargo.toml",
            "Cargo.lock",
        ):
            checks["rust"] = True
        else:
            # Unknown paths, CI definitions, and build tools require every check.
            return dict.fromkeys(checks, True)
    return checks


def changed_paths(base: str, head: str = "HEAD", *, cwd: Path | None = None) -> list[str]:
    result = subprocess.run(
        ["git", "diff", "--name-only", "--no-renames", "-z", base, head, "--"],
        cwd=cwd,
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    # Disabling rename detection includes both the removed and added paths.
    return [path.decode("utf-8", errors="surrogateescape") for path in result.stdout.split(b"\0") if path]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", help="Full base commit SHA")
    parser.add_argument("--head", default="HEAD", help="Full head commit SHA (default: HEAD)")
    parser.add_argument("--all", action="store_true", help="Require every check without inspecting Git")
    args = parser.parse_args()

    if args.all:
        checks = dict.fromkeys(("rom", "rust", "installer"), True)
    else:
        if not args.base or not re.fullmatch(r"(?:[0-9a-fA-F]{40}|[0-9a-fA-F]{64})", args.base):
            parser.error("--base must be a full commit SHA unless --all is used")
        if args.head != "HEAD" and not re.fullmatch(r"(?:[0-9a-fA-F]{40}|[0-9a-fA-F]{64})", args.head):
            parser.error("--head must be HEAD or a full commit SHA")
        try:
            checks = classify_paths(changed_paths(args.base, args.head))
        except (OSError, subprocess.CalledProcessError) as error:
            # Emit no selections on failure; the workflow must fail this job.
            print(f"Cannot classify changes: {error}", file=sys.stderr)
            return 1

    for name, required in checks.items():
        print(f"{name}={str(required).lower()}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
