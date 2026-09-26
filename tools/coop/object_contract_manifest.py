#!/usr/bin/env python3
"""Compare linked shared-ID table scalar bytes across ROMs.

Pointer locations come from a compiler-emitted descriptor in each ROM. Pointer
presence is compared separately from scalar bytes. This does not attest to
pointed-to data, callbacks, menu behavior, or full object semantics and must
not be used as an object_catalog_sha256 release claim.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess

try:
    from .player_transfer_manifest import ManifestError
except ImportError:
    from player_transfer_manifest import ManifestError


TABLES = (
    "gItemsInfo", "gSpeciesInfo", "gMovesInfo", "gAbilitiesInfo",
    "gTMHMItemMoveIds",
)
DESCRIPTOR = "gCoopObjectScalarDescriptor"
SYMBOLS = (DESCRIPTOR, *TABLES)
HEADER = struct.Struct("<IHH")
ENTRY = struct.Struct("<HH48H")
DESCRIPTOR_SIZE = HEADER.size + len(TABLES) * ENTRY.size
MAGIC = 0x3143534F


def symbols_from_nm(output: str) -> dict[str, tuple[int, int]]:
    found: dict[str, list[tuple[int, int]]] = {name: [] for name in SYMBOLS}
    for line in output.splitlines():
        parts = line.split()
        if parts and parts[0] in found:
            if len(parts) != 4 or parts[1] not in ("r", "R", "d", "D"):
                raise ManifestError(f"invalid ELF symbol {parts[0]}")
            try:
                address, size = int(parts[2], 16), int(parts[3], 16)
            except ValueError as exc:
                raise ManifestError(f"invalid ELF symbol numbers {parts[0]}") from exc
            found[parts[0]].append((address, size))
    if any(len(values) != 1 for values in found.values()):
        raise ManifestError("missing or duplicate shared-object ELF symbol")
    result = {name: values[0] for name, values in found.items()}
    for name, (address, size) in result.items():
        if address % 4 or size <= 0:
            raise ManifestError(f"unaligned or empty shared-object ELF symbol {name}")
    if result[DESCRIPTOR][1] != DESCRIPTOR_SIZE:
        raise ManifestError("unexpected compiler descriptor size")
    return result


def decode_descriptor(payload: bytes) -> list[tuple[int, tuple[int, ...]]]:
    if len(payload) != DESCRIPTOR_SIZE:
        raise ManifestError("unexpected object scalar descriptor length")
    magic, version, count = HEADER.unpack_from(payload)
    if (magic, version, count) != (MAGIC, 1, len(TABLES)):
        raise ManifestError("unsupported object scalar descriptor")
    layouts = []
    for index in range(count):
        stride, pointer_count, *offsets = ENTRY.unpack_from(payload, HEADER.size + index * ENTRY.size)
        if (stride < 1 or stride > 4096 or pointer_count > 48
                or (index == 4 and pointer_count != 0)
                or (index != 4 and pointer_count == 0)
                or any(offsets[pointer_count:])):
            raise ManifestError(f"invalid object scalar layout {TABLES[index]}")
        pointers = tuple(offsets[:pointer_count])
        if (len(set(pointers)) != len(pointers)
                or any(offset % 4 or offset + 4 > stride for offset in pointers)):
            raise ManifestError(f"invalid pointer offsets for {TABLES[index]}")
        layouts.append((stride, pointers))
    return layouts


def scalar_bytes(payload: bytes, stride: int, pointers: tuple[int, ...]) -> tuple[bytes, bytes]:
    if not payload or len(payload) % stride:
        raise ManifestError("shared-object table length is not a record multiple")
    canonical = bytearray(payload)
    presence = bytearray()
    for base in range(0, len(canonical), stride):
        for offset in pointers:
            presence.append(int(any(canonical[base + offset:base + offset + 4])))
            canonical[base + offset:base + offset + 4] = b"\0" * 4
    return bytes(canonical), bytes(presence)


def manifest_from_rom(rom: bytes, symbols: dict[str, tuple[int, int]]) -> dict:
    desc_address, desc_size = symbols[DESCRIPTOR]
    # read_rom_symbol's path-based interface is intentionally avoided here so
    # synthetic ROM tests exercise the exact same bounded address math.
    def read(name: str) -> bytes:
        address, size = symbols[name]
        offset = address - 0x08000000
        if offset < 0 or size <= 0 or offset + size > len(rom) or address + size > 0x0A000000:
            raise ManifestError(f"{name} is outside the shipped ROM")
        return rom[offset:offset + size]

    layouts = decode_descriptor(read(DESCRIPTOR))
    entries = {}
    for name, (stride, pointers) in zip(TABLES, layouts):
        address, size = symbols[name]
        raw = read(name)
        canonical, presence = scalar_bytes(raw, stride, pointers)
        entries[name] = {
            "address": address,
            "size": size,
            "record_count": size // stride,
            "record_stride": stride,
            "pointer_offsets": list(pointers),
            "raw_sha256": hashlib.sha256(raw).hexdigest(),
            "scalar_sha256": hashlib.sha256(canonical).hexdigest(),
            "pointer_presence_sha256": hashlib.sha256(presence).hexdigest(),
        }
    return {
        "schema_version": 1,
        "scope": "linked-table-scalar-bytes-only",
        "rom_sha256": hashlib.sha256(rom).hexdigest(),
        "descriptor": {"address": desc_address, "size": desc_size,
                       "sha256": hashlib.sha256(read(DESCRIPTOR)).hexdigest()},
        "tables": entries,
    }


def require_same_scalar_tables(worlds: dict[str, dict]) -> None:
    if len(worlds) < 2:
        raise ManifestError("at least two world ROMs are required")
    reference = None
    for world, manifest in worlds.items():
        tables = manifest.get("tables")
        if not isinstance(tables, dict) or set(tables) != set(TABLES):
            raise ManifestError(f"invalid scalar table manifest for {world}")
        fingerprint = tuple((tables[name]["record_count"], tables[name]["record_stride"],
                             tuple(tables[name]["pointer_offsets"]),
                             tables[name]["scalar_sha256"],
                             tables[name]["pointer_presence_sha256"]) for name in TABLES)
        if reference is None:
            reference = fingerprint
        elif fingerprint != reference:
            raise ManifestError(f"world ROMs disagree on shared-object scalar data: {world}")


def build_manifest(elf: Path, rom: Path, nm: str) -> dict:
    try:
        result = subprocess.run(
            [nm, "--defined-only", "--print-size", "--format=posix", str(elf)],
            capture_output=True, text=True, check=False,
        )
    except OSError as exc:
        raise ManifestError(f"cannot run nm: {exc}") from exc
    if result.returncode:
        raise ManifestError(result.stderr.strip() or "nm failed")
    return manifest_from_rom(rom.read_bytes(), symbols_from_nm(result.stdout))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--elf", type=Path, required=True)
    parser.add_argument("--rom", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--nm", default=os.environ.get("ARM_NM", "arm-none-eabi-nm"))
    args = parser.parse_args()
    try:
        manifest = build_manifest(args.elf, args.rom, args.nm)
    except (ManifestError, OSError) as exc:
        parser.exit(1, f"object scalar manifest: {exc}\n")
    args.manifest.parent.mkdir(parents=True, exist_ok=True)
    args.manifest.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
