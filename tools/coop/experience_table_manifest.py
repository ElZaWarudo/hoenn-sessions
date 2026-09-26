#!/usr/bin/env python3
"""Bind a ROM's pointer-free experience progression table to its linked ELF."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

try:
    from .player_transfer_manifest import ManifestError, read_rom_symbol
except ImportError:
    from player_transfer_manifest import ManifestError, read_rom_symbol


SYMBOL = "gExperienceTables"
GROWTH_RATES = 6
MAX_LEVEL = 100
EXPECTED_SIZE = GROWTH_RATES * (MAX_LEVEL + 1) * 4


def symbol_from_nm(output: str) -> tuple[int, int]:
    matches = []
    for line in output.splitlines():
        parts = line.split()
        if parts and parts[0] == SYMBOL:
            if len(parts) != 4 or parts[1] not in ("r", "R"):
                raise ManifestError("experience table has an invalid ELF symbol")
            try:
                matches.append((int(parts[2], 16), int(parts[3], 16)))
            except ValueError as error:
                raise ManifestError("experience table has invalid symbol numbers") from error
    if len(matches) != 1 or matches[0][1] != EXPECTED_SIZE or matches[0][0] % 4:
        raise ManifestError("experience table has an unexpected linked layout")
    return matches[0]


def build_manifest(elf: Path, rom: Path, nm: str) -> dict:
    try:
        result = subprocess.run(
            [nm, "--defined-only", "--print-size", "--format=posix", str(elf)],
            capture_output=True, text=True, check=False,
        )
    except OSError as error:
        raise ManifestError(f"cannot run nm: {error}") from error
    if result.returncode:
        raise ManifestError(result.stderr.strip() or "nm failed")
    address, size = symbol_from_nm(result.stdout)
    payload = read_rom_symbol(rom, address, size)
    return {
        "schema_version": 1,
        "rom_sha256": hashlib.sha256(rom.read_bytes()).hexdigest(),
        "symbol": SYMBOL,
        "address": address,
        "size": size,
        "sha256": hashlib.sha256(payload).hexdigest(),
    }


def require_same_experience_tables(worlds: dict[str, dict]) -> None:
    if len(worlds) < 2 or len({entry["sha256"] for entry in worlds.values()}) != 1:
        raise ManifestError("world ROMs disagree on experience progression")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--elf", type=Path, required=True)
    parser.add_argument("--rom", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--nm", default=os.environ.get("ARM_NM", "arm-none-eabi-nm"))
    args = parser.parse_args()
    try:
        manifest = build_manifest(args.elf, args.rom, args.nm)
    except (ManifestError, OSError) as error:
        parser.exit(1, f"experience table manifest: {error}\n")
    args.manifest.parent.mkdir(parents=True, exist_ok=True)
    args.manifest.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
