#!/usr/bin/env python3
"""Validate and publish the compiler-derived shared-player schema.

The ROM contains the schema as ``gCoopPlayerTransferSchema``.  This tool is
kept separate from the live bridge manifest while travel is inactive, but it
uses the same ELF/ROM symbol pairing discipline: a manifest is accepted only
when the bytes read from the linked ROM pass the complete coverage contract.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import stat
import struct
import subprocess
from pathlib import Path
from typing import Any


SCHEMA_SYMBOL = "gCoopPlayerTransferSchema"
SCHEMA_MAGIC = 0x31545043
SCHEMA_VERSION = 3
# Compiler-emitted layout from both world profiles. Changing a saved byte span
# requires a deliberate schema revision and a new audited digest.
PINNED_LAYOUT_SHA256 = "17a72b6437e27d1ae3cc2ff2d1fee401625ceaaeb31a35b6bb31df2d3676abf4"
ROM_START = 0x08000000
ROM_END = 0x0A000000
HEADER_STRUCT = struct.Struct("<IHH9I")
FIELD_STRUCT = struct.Struct("<HBBIII")
HEADER_SIZE = HEADER_STRUCT.size
FIELD_SIZE = FIELD_STRUCT.size

STORAGE_SAVE_BLOCK1 = 0
STORAGE_SAVE_BLOCK2 = 1
STORAGE_POKEMON_STORAGE = 2
STORAGE_SAVE_BLOCK3 = 3
STORAGE_COUNT = 4

OWNER_SHARED_PLAYER = 1
OWNER_WORLD_LOCAL = 2
OWNER_LOCAL_PENDING = 3

# This table is the stable ID ledger.  Adding a field requires a new schema
# version and an explicit owner/storage decision; IDs are never recycled.
EXPECTED_FIELDS: dict[int, tuple[int, int]] = {
    0x0100: (STORAGE_SAVE_BLOCK1, OWNER_WORLD_LOCAL),
    0x0101: (STORAGE_SAVE_BLOCK1, OWNER_SHARED_PLAYER),
    0x0102: (STORAGE_SAVE_BLOCK1, OWNER_SHARED_PLAYER),
    0x0103: (STORAGE_SAVE_BLOCK1, OWNER_SHARED_PLAYER),
    0x0104: (STORAGE_SAVE_BLOCK1, OWNER_SHARED_PLAYER),
    0x0105: (STORAGE_SAVE_BLOCK1, OWNER_SHARED_PLAYER),
    0x0106: (STORAGE_SAVE_BLOCK1, OWNER_SHARED_PLAYER),
    0x0107: (STORAGE_SAVE_BLOCK1, OWNER_SHARED_PLAYER),
    0x0108: (STORAGE_SAVE_BLOCK1, OWNER_WORLD_LOCAL),
    0x0113: (STORAGE_SAVE_BLOCK1, OWNER_SHARED_PLAYER),  # encrypted game statistics
    0x0114: (STORAGE_SAVE_BLOCK1, OWNER_WORLD_LOCAL),
    0x0109: (STORAGE_SAVE_BLOCK1, OWNER_WORLD_LOCAL),  # room placement
    0x0115: (STORAGE_SAVE_BLOCK1, OWNER_WORLD_LOCAL),  # room furnishings
    0x010A: (STORAGE_SAVE_BLOCK1, OWNER_WORLD_LOCAL),
    0x010B: (STORAGE_SAVE_BLOCK1, OWNER_SHARED_PLAYER),
    0x010C: (STORAGE_SAVE_BLOCK1, OWNER_WORLD_LOCAL),
    0x010D: (STORAGE_SAVE_BLOCK1, OWNER_SHARED_PLAYER),  # Day Care custody
    0x010E: (STORAGE_SAVE_BLOCK1, OWNER_WORLD_LOCAL),
    0x010F: (STORAGE_SAVE_BLOCK1, OWNER_SHARED_PLAYER),
    0x0110: (STORAGE_SAVE_BLOCK1, OWNER_SHARED_PLAYER),
    0x0111: (STORAGE_SAVE_BLOCK1, OWNER_WORLD_LOCAL),
    0x0112: (STORAGE_SAVE_BLOCK1, OWNER_SHARED_PLAYER),  # Route 5 custody
    0x0200: (STORAGE_SAVE_BLOCK2, OWNER_SHARED_PLAYER),
    0x0201: (STORAGE_SAVE_BLOCK2, OWNER_WORLD_LOCAL),
    0x0202: (STORAGE_SAVE_BLOCK2, OWNER_SHARED_PLAYER),
    0x0203: (STORAGE_SAVE_BLOCK2, OWNER_SHARED_PLAYER),
    0x0204: (STORAGE_SAVE_BLOCK2, OWNER_WORLD_LOCAL),
    0x0205: (STORAGE_SAVE_BLOCK2, OWNER_SHARED_PLAYER),
    0x0206: (STORAGE_SAVE_BLOCK2, OWNER_SHARED_PLAYER),
    0x0207: (STORAGE_SAVE_BLOCK2, OWNER_SHARED_PLAYER),
    0x0208: (STORAGE_SAVE_BLOCK2, OWNER_SHARED_PLAYER),
    0x0209: (STORAGE_SAVE_BLOCK2, OWNER_SHARED_PLAYER),
    0x020A: (STORAGE_SAVE_BLOCK2, OWNER_WORLD_LOCAL),
    0x020B: (STORAGE_SAVE_BLOCK2, OWNER_WORLD_LOCAL),
    0x020C: (STORAGE_SAVE_BLOCK2, OWNER_WORLD_LOCAL),
    0x020D: (STORAGE_SAVE_BLOCK2, OWNER_SHARED_PLAYER),  # encrypted berry powder
    0x020E: (STORAGE_SAVE_BLOCK2, OWNER_WORLD_LOCAL),
    0x0300: (STORAGE_POKEMON_STORAGE, OWNER_SHARED_PLAYER),
    0x0305: (STORAGE_POKEMON_STORAGE, OWNER_WORLD_LOCAL),
    0x0301: (STORAGE_POKEMON_STORAGE, OWNER_SHARED_PLAYER),
    0x0302: (STORAGE_POKEMON_STORAGE, OWNER_SHARED_PLAYER),
    0x0303: (STORAGE_POKEMON_STORAGE, OWNER_SHARED_PLAYER),
    0x0304: (STORAGE_POKEMON_STORAGE, OWNER_SHARED_PLAYER),
    0x0400: (STORAGE_SAVE_BLOCK3, OWNER_WORLD_LOCAL),
    0x0403: (STORAGE_SAVE_BLOCK3, OWNER_SHARED_PLAYER),
}

# These shared fields cannot be copied byte-for-byte because their contents
# are encrypted with the source world's SaveBlock2.encryptionKey.  The
# destination keeps its own key, so the projector must decrypt and re-encrypt
# before activation. Deposited Pokémon travel under the one-active-world
# journal; the source world stays parked until the destination commits.
REKEY_FIELD_IDS = frozenset({0x0102, 0x0103, 0x0106, 0x0113, 0x020D})
DAYCARE_CUSTODY_FIELD_IDS = frozenset({0x010D, 0x0112})


class ManifestError(ValueError):
    """A linked transfer schema is not safe to publish."""


def _checked_add(left: int, right: int, context: str) -> int:
    result = left + right
    if result < left or result > 0xFFFFFFFF:
        raise ManifestError(f"{context} overflows u32")
    return result


def parse_schema_payload(payload: bytes) -> dict[str, Any]:
    """Validate one compiler-emitted schema and return its decoded fields."""
    if len(payload) < HEADER_SIZE:
        raise ManifestError("player-transfer schema is shorter than its header")
    (
        magic,
        version,
        field_count,
        descriptor_size,
        header_size,
        save_block1_size,
        save_block2_size,
        pokemon_storage_size,
        save_block3_size,
        fields_offset,
        field_record_size,
        reserved,
    ) = HEADER_STRUCT.unpack_from(payload)
    spans = (
        save_block1_size,
        save_block2_size,
        pokemon_storage_size,
        save_block3_size,
    )
    if magic != SCHEMA_MAGIC:
        raise ManifestError("player-transfer schema magic is not CPT1")
    if version != SCHEMA_VERSION:
        raise ManifestError(f"unsupported player-transfer schema version {version}")
    if field_count != len(EXPECTED_FIELDS):
        raise ManifestError(
            f"player-transfer field count {field_count} does not match the ID ledger"
        )
    if descriptor_size != len(payload):
        raise ManifestError("player-transfer descriptor size does not match symbol bytes")
    if header_size != HEADER_SIZE or fields_offset != HEADER_SIZE:
        raise ManifestError("player-transfer header/field offset drifted")
    if field_record_size != FIELD_SIZE:
        raise ManifestError("player-transfer field record size drifted")
    if reserved != 0 or any(span == 0 for span in spans):
        raise ManifestError("player-transfer schema has invalid reserved or span bytes")
    expected_size = _checked_add(
        fields_offset,
        field_count * field_record_size,
        "player-transfer descriptor",
    )
    if expected_size != descriptor_size:
        raise ManifestError("player-transfer descriptor has trailing or missing bytes")

    fields: list[dict[str, int]] = []
    seen: set[int] = set()
    cursor = [0] * STORAGE_COUNT
    for index in range(field_count):
        start = fields_offset + index * field_record_size
        field_id, storage, owner, offset, size, field_reserved = FIELD_STRUCT.unpack_from(
            payload, start
        )
        expected = EXPECTED_FIELDS.get(field_id)
        if expected is None:
            raise ManifestError(f"unknown player-transfer field ID 0x{field_id:04X}")
        if field_id in seen:
            raise ManifestError(f"duplicate player-transfer field ID 0x{field_id:04X}")
        seen.add(field_id)
        if storage >= STORAGE_COUNT or expected[0] != storage or expected[1] != owner:
            raise ManifestError(f"ownership/storage drift for field 0x{field_id:04X}")
        if field_reserved != 0 or size == 0:
            raise ManifestError(f"invalid reserved or zero length for field 0x{field_id:04X}")
        span = spans[storage]
        if offset != cursor[storage] or offset > span or size > span - offset:
            raise ManifestError(f"gap or overlap at player-transfer field 0x{field_id:04X}")
        cursor[storage] += size
        fields.append(
            {
                "id": field_id,
                "storage": storage,
                "ownership": owner,
                "offset": offset,
                "size": size,
            }
        )
    if seen != set(EXPECTED_FIELDS):
        missing = sorted(set(EXPECTED_FIELDS).difference(seen))
        raise ManifestError("missing player-transfer field IDs: " + ", ".join(hex(i) for i in missing))
    if tuple(cursor) != spans:
        raise ManifestError("player-transfer fields do not cover every saved byte")
    return {
        "schema_version": version,
        "field_count": field_count,
        "descriptor_size": descriptor_size,
        "rekey_field_ids": sorted(REKEY_FIELD_IDS),
        "daycare_custody_field_ids": sorted(DAYCARE_CUSTODY_FIELD_IDS),
        "spans": {
            "save_block1": save_block1_size,
            "save_block2": save_block2_size,
            "pokemon_storage": pokemon_storage_size,
            "save_block3": save_block3_size,
        },
        "fields": fields,
    }


def sha256_bytes(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def require_travel_ready(schema: dict[str, Any]) -> None:
    """Check descriptor ownership; runtime still needs a fenced authority."""
    pending = [field["id"] for field in schema["fields"]
               if field["ownership"] == OWNER_LOCAL_PENDING]
    if pending:
        raise ManifestError(
            "player transfer has unresolved fields: "
            + ", ".join(f"0x{field_id:04X}" for field_id in pending)
            + " (every deposited Pokémon needs shared authoritative custody)"
        )
    owners = {field["id"]: field["ownership"] for field in schema["fields"]}
    if (any(owners.get(field_id) != OWNER_SHARED_PLAYER for field_id in DAYCARE_CUSTODY_FIELD_IDS)
            or owners.get(0x0403) != OWNER_SHARED_PLAYER
            or set(schema.get("rekey_field_ids", [])) != REKEY_FIELD_IDS):
        raise ManifestError("player transfer lacks shared custody, co-op authority, or rekey fields")


def require_same_transfer_schema(worlds: dict[str, dict[str, Any]]) -> None:
    """Every ROM in a travel family must describe identical saved byte spans."""
    if not worlds:
        raise ManifestError("player transfer needs at least one world schema")
    first = next(iter(worlds.values()))
    for world, candidate in worlds.items():
        if (candidate.get("schema_version") != first.get("schema_version")
                or candidate.get("spans") != first.get("spans")
                or candidate.get("fields") != first.get("fields")):
            raise ManifestError(f"{world} has an incompatible player-transfer schema")


def parse_nm_symbols(output: str) -> dict[str, tuple[int, int, str]]:
    symbols: dict[str, tuple[int, int, str]] = {}
    for line_number, raw_line in enumerate(output.splitlines(), 1):
        parts = raw_line.split()
        if not parts:
            continue
        if len(parts) not in (3, 4):
            raise ManifestError(f"invalid nm record at line {line_number}")
        name, kind, address_hex = parts[:3]
        size_hex = parts[3] if len(parts) == 4 else "0"
        if name == SCHEMA_SYMBOL:
            if name in symbols:
                raise ManifestError("duplicate player-transfer schema symbol")
            try:
                symbols[name] = (int(address_hex, 16), int(size_hex, 16), kind)
            except ValueError as error:
                raise ManifestError("invalid player-transfer nm record") from error
    return symbols


def read_rom_symbol(rom: Path, address: int, size: int) -> bytes:
    if address < ROM_START or address + size > ROM_END:
        raise ManifestError("player-transfer schema is outside the GBA ROM window")
    offset = address - ROM_START
    try:
        metadata = rom.stat()
        if not stat.S_ISREG(metadata.st_mode):
            raise ManifestError("ROM input is not a regular file")
        with rom.open("rb") as stream:
            stream.seek(offset)
            payload = stream.read(size)
    except OSError as error:
        raise ManifestError(f"cannot read ROM: {error}") from error
    if len(payload) != size:
        raise ManifestError("short read for player-transfer schema")
    return payload


def build_manifest(elf: Path, rom: Path, nm: str) -> dict[str, Any]:
    try:
        completed = subprocess.run(
            [nm, "--defined-only", "--print-size", "--format=posix", str(elf)],
            check=False,
            capture_output=True,
            text=True,
        )
    except OSError as error:
        raise ManifestError(f"cannot run nm: {error}") from error
    if completed.returncode:
        raise ManifestError(completed.stderr.strip() or "nm failed")
    symbol = parse_nm_symbols(completed.stdout).get(SCHEMA_SYMBOL)
    if symbol is None:
        raise ManifestError(f"linked ELF does not define {SCHEMA_SYMBOL}")
    address, size, kind = symbol
    if kind not in ("r", "R") or address % 4 or size != HEADER_SIZE + len(EXPECTED_FIELDS) * FIELD_SIZE:
        raise ManifestError("player-transfer schema symbol has an unexpected layout")
    payload = read_rom_symbol(rom, address, size)
    layout_sha256 = sha256_bytes(payload)
    if layout_sha256 != PINNED_LAYOUT_SHA256:
        raise ManifestError(
            "player-transfer layout differs from the audited schema digest: "
            + layout_sha256
        )
    decoded = parse_schema_payload(payload)
    return {
        "schema_version": SCHEMA_VERSION,
        "rom_sha256": sha256_bytes(rom.read_bytes()),
        "symbol": SCHEMA_SYMBOL,
        "address": address,
        "size": size,
        "sha256": layout_sha256,
        **decoded,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--elf", type=Path, required=True)
    parser.add_argument("--rom", type=Path, required=True)
    parser.add_argument("--nm", default=os.environ.get("ARM_NM", "arm-none-eabi-nm"))
    parser.add_argument("--manifest", type=Path, default=Path("dist/player_transfer_manifest.json"))
    args = parser.parse_args()
    manifest = build_manifest(args.elf, args.rom, args.nm)
    args.manifest.parent.mkdir(parents=True, exist_ok=True)
    args.manifest.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
