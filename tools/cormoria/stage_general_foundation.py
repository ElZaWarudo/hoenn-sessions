"""Stage the pinned donor General tileset foundation outside the live ROM.

Read Git objects from the exact donor commit, verify the committed inventory,
and optionally cross-check an extracted archive. No host tileset is substituted.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

from tools.cormoria import import_world

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "tools/cormoria/general_foundation_manifest.json"
REVISION = "f7997186345885bfa23a170e5f573851fc034b9b"
TREE = "f75c219630afdf552875547c196705fa5d787586"
REPOSITORY = "https://github.com/dsmyst/dreamstone-mysteries"
PATHS = ("src/graphics.c", "data/tilesets/primary/general/tiles.png",
         *(f"data/tilesets/primary/general/palettes/{i:02}.pal" for i in range(16)))


class FoundationError(ValueError):
    """The donor source cannot be proven to match the pinned foundation."""


def git(repo: Path, *args: str) -> bytes:
    result = subprocess.run(["git", "-C", str(repo), *args], capture_output=True, check=False)
    if result.returncode:
        raise FoundationError(f"git {' '.join(args)}: {result.stderr.decode(errors='replace').strip()}")
    return result.stdout


def object_inventory(repo: Path) -> dict[str, dict[str, object]]:
    if git(repo, "rev-parse", "HEAD").decode().strip() != REVISION:
        raise FoundationError("donor HEAD differs from pinned revision")
    if git(repo, "rev-parse", "HEAD^{tree}").decode().strip() != TREE:
        raise FoundationError("donor tree differs from pinned tree")
    remote = git(repo, "remote", "get-url", "origin").decode().strip().removesuffix(".git")
    if remote != REPOSITORY:
        raise FoundationError("donor origin differs from pinned repository")
    result: dict[str, dict[str, object]] = {}
    for relative in PATHS:
        line = git(repo, "ls-tree", "HEAD", "--", relative).decode().strip()
        fields = line.split()
        if len(fields) != 4 or fields[0] != "100644" or fields[1] != "blob" or fields[3] != relative:
            raise FoundationError(f"missing regular Git blob: {relative}")
        data = git(repo, "cat-file", "blob", fields[2])
        result[relative] = {"path": relative, "git_blob": fields[2],
                            "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
    return result


def expected_manifest(repo: Path) -> dict[str, object]:
    return {"schema_version": 1, "provenance": {"repository": REPOSITORY,
            "revision": REVISION, "tree": TREE},
            "files": list(object_inventory(repo).values())}


def stage(repo: Path, output: Path, archive: Path | None = None,
          manifest: Path = MANIFEST) -> int:
    repo = repo.resolve(strict=True)
    output = output.resolve()
    archive = archive.resolve(strict=True) if archive is not None else None
    if output.exists() or output == ROOT or output.is_relative_to(ROOT) or (
            output == repo or output.is_relative_to(repo)) or (
            archive is not None and (output == archive or output.is_relative_to(archive))):
        raise FoundationError("output must be fresh and outside the repository and donor inputs")
    expected = expected_manifest(repo)
    committed = json.loads(manifest.read_text(encoding="utf-8"))
    if committed != expected:
        raise FoundationError("committed General inventory differs from pinned Git objects")
    pending: list[tuple[Path, bytes]] = []
    for record in expected["files"]:
        relative = record["path"]
        data = git(repo, "cat-file", "blob", record["git_blob"])
        if archive is not None:
            source = (archive / import_world.safe_relative(relative)).resolve()
            if not source.is_relative_to(archive) or not source.is_file():
                raise FoundationError(f"extracted archive differs from pinned Git blob: {relative}")
            archive_data = source.read_bytes()
            # The donor's extracted Windows checkout has CRLF .pal text.
            # Stage the Git object bytes, never the normalized checkout bytes.
            if relative.endswith(".pal") and b"\r" not in archive_data.replace(b"\r\n", b""):
                archive_data = archive_data.replace(b"\r\n", b"\n")
            if archive_data != data:
                raise FoundationError(f"extracted archive differs from pinned Git blob: {relative}")
        pending.append((Path("source") / import_world.safe_relative(relative), data))
    output.mkdir(parents=True)
    for relative, data in pending:
        target = output / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    (output / "foundation_manifest.json").write_text(
        json.dumps(expected, indent=2) + "\n", encoding="utf-8", newline="\n")
    return len(pending)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--donor-git", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--archive", type=Path)
    args = parser.parse_args(argv)
    try:
        count = stage(args.donor_git, args.output, args.archive)
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        print(f"Cormoria General foundation: {exc}", file=sys.stderr)
        return 1
    print(f"Staged {count} pinned General source files at {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
