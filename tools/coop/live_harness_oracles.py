"""Read-only, descriptor-driven oracles for test-only multi-ROM travel.

These checks inspect ROM-written Flash1M images. They never manufacture a save
or make an authority decision; the signed server and client own those steps.
"""

from __future__ import annotations

import binascii
import hashlib
import struct
from dataclasses import dataclass
from pathlib import Path

from player_transfer_manifest import (OWNER_SHARED_PLAYER, OWNER_WORLD_LOCAL,
                                      REKEY_FIELD_IDS, parse_schema_payload)

SECTOR = 4096
SECTOR_SIZES = (3892, 3968, 3968, 3968, 3968, 2976,
                3968, 3968, 3968, 3968, 3968, 3968, 3968, 3968, 2400)
FLASH_SIZE = 128 * 1024
COOP_SIZE = 672


class OracleFailure(AssertionError):
    """The first mismatching live-save boundary."""


def _u16(data: bytes, offset: int) -> int:
    return struct.unpack_from("<H", data, offset)[0]


def _u32(data: bytes, offset: int) -> int:
    return struct.unpack_from("<I", data, offset)[0]


def _checksum(payload: bytes) -> int:
    total = 0
    for (word,) in struct.iter_unpack("<I", payload[:len(payload) // 4 * 4]):
        total = (total + word) & 0xFFFFFFFF
    return (total + (total >> 16)) & 0xFFFF


@dataclass(frozen=True)
class FlashSave:
    path: Path
    sha256: str
    counter: int
    sectors: tuple[bytes, ...]
    lineage: bytes
    coop: bytes

    @property
    def generation(self) -> int:
        return _u32(self.coop, 28)

    def field(self, storage: int, offset: int, size: int) -> bytes:
        if storage == 3:
            block = b"".join(s[3968:4084] for s in self.sectors)
        elif storage == 1:
            block = self.sectors[0][:SECTOR_SIZES[0]]
        elif storage == 0:
            block = b"".join(self.sectors[i][:SECTOR_SIZES[i]] for i in range(1, 6))
        elif storage == 2:
            block = b"".join(self.sectors[i][:SECTOR_SIZES[i]] for i in range(6, 15))
        else:
            raise OracleFailure(f"unknown descriptor storage {storage}")
        if offset < 0 or size < 0 or offset + size > len(block):
            raise OracleFailure(f"field outside storage {storage}: {offset}+{size}")
        return block[offset:offset + size]


def _slot(data: bytes, index: int) -> tuple[int, tuple[bytes, ...]]:
    sectors: list[bytes | None] = [None] * 15
    counter: int | None = None
    for physical in range(15):
        start = (index * 15 + physical) * SECTOR
        sector = data[start:start + SECTOR]
        logical = _u16(sector, 4084)
        if logical >= 15 or sectors[logical] is not None:
            raise OracleFailure(f"slot {index}: duplicate/invalid logical sector {logical}")
        if _u32(sector, 4088) != 0x08012025:
            raise OracleFailure(f"slot {index}: sector {physical} signature")
        if _u16(sector, 4086) != _checksum(sector[:SECTOR_SIZES[logical]]):
            raise OracleFailure(f"slot {index}: sector {physical} checksum")
        value = _u32(sector, 4092)
        if counter is not None and value != counter:
            raise OracleFailure(f"slot {index}: mixed counters")
        counter = value
        sectors[logical] = sector
    if any(s is None for s in sectors):
        raise OracleFailure(f"slot {index}: missing logical sector")
    return counter, tuple(sectors)  # type: ignore[return-value]


def read_flash(path: Path) -> FlashSave:
    return read_flash_bytes(Path(path).read_bytes(), path)


def read_flash_bytes(data: bytes, path: Path) -> FlashSave:
    """Inspect one immutable read, including while its source file is replaced."""
    if len(data) not in (FLASH_SIZE, FLASH_SIZE + 16):
        raise OracleFailure(f"{path}: invalid Flash1M length {len(data)}")
    candidates = []
    for index in range(2):
        try:
            candidates.append((index, *_slot(data, index)))
        except OracleFailure:
            pass  # The inactive rotating slot may be incomplete.
    if not candidates:
        raise OracleFailure(f"{path}: neither Flash1M slot is valid")
    if len(candidates) == 2:
        first, second = candidates
        a, b = first[1], second[1]
        counter = b if a == 0xFFFFFFFF and b == 0 else a if a == 0 and b == 0xFFFFFFFF else max(a, b)
    else:
        counter = candidates[0][1]
    selected_index = counter & 1
    selected = next((item for item in candidates if item[0] == selected_index and item[1] == counter), None)
    if selected is None:
        raise OracleFailure(f"{path}: ROM-selected slot {selected_index} is invalid")
    _, _, sectors = selected
    sb2 = sectors[0]
    lineage = sb2[0:8] + sb2[16:18] + sb2[19:23]
    block3 = b"".join(s[3968:4084] for s in sectors)
    coop = block3[4:4 + COOP_SIZE]
    if _u32(coop, 0) != 0x31505343 or _u16(coop, 4) != 2 or _u16(coop, 6) != COOP_SIZE:
        raise OracleFailure(f"{path}: invalid co-op schema-two record")
    if _u32(coop, 668) != binascii.crc32(coop[:668]):
        raise OracleFailure(f"{path}: co-op CRC mismatch")
    return FlashSave(Path(path), hashlib.sha256(data).hexdigest(), counter, sectors, lineage, coop)


def _logical_value(field_id: int, data: bytes, key: int) -> bytes:
    if field_id not in REKEY_FIELD_IDS:
        return data
    out = bytearray(data)
    if field_id in (0x0102, 0x020D):
        struct.pack_into("<I", out, 0, _u32(out, 0) ^ key)
    elif field_id == 0x0103:
        struct.pack_into("<H", out, 0, _u16(out, 0) ^ (key & 0xFFFF))
    elif field_id == 0x0106:
        for pos in range(2, len(out), 4):
            struct.pack_into("<H", out, pos, _u16(out, pos) ^ (key & 0xFFFF))
    elif field_id == 0x0113:
        for pos in range(0, len(out), 4):
            struct.pack_into("<I", out, pos, _u32(out, pos) ^ key)
    return bytes(out)


def _key(save: FlashSave, fields: list[dict]) -> int:
    field = next(f for f in fields if f["id"] == 0x020B)
    return _u32(save.field(field["storage"], field["offset"], field["size"]), 0)


def logical_field(save: FlashSave, descriptor: bytes, field_id: int) -> bytes:
    fields = parse_schema_payload(descriptor)["fields"]
    field = next(f for f in fields if f["id"] == field_id)
    return _logical_value(field_id, save.field(field["storage"], field["offset"], field["size"]),
                          _key(save, fields))


def check_shared_witnesses(save: FlashSave, descriptor: bytes, witnesses: list[dict]) -> list[dict]:
    """Reject empty or changed fixture witnesses before launching or uploading.

    Hash logical values so destination encryption keys cannot invalidate a
    witness. This proves populated bytes, not Pokémon validity or species.
    """
    fields = {f["id"]: f for f in parse_schema_payload(descriptor)["fields"]}
    if not isinstance(witnesses, list) or len(witnesses) > len(fields):
        raise OracleFailure("shared witnesses must be a bounded list")
    checked, seen = [], set()
    for witness in witnesses:
        if not isinstance(witness, dict) or set(witness) != {"field_id", "sha256", "offset", "size", "min_nonzero_bytes"}:
            raise OracleFailure("shared witness needs field_id, sha256, offset, size and min_nonzero_bytes")
        fid, expected, minimum = (witness[k] for k in ("field_id", "sha256", "min_nonzero_bytes"))
        if type(fid) is not int or fid in seen or fid not in fields:
            raise OracleFailure("shared witness field is invalid or duplicated")
        if fields[fid]["ownership"] != OWNER_SHARED_PLAYER or fid == 0x0403:
            raise OracleFailure("shared witness must select player data")
        if (not isinstance(expected, str) or len(expected) != 64
                or any(c not in "0123456789abcdef" for c in expected)):
            raise OracleFailure("shared witness digest must be lowercase SHA-256")
        value = logical_field(save, descriptor, fid)
        offset, size = witness["offset"], witness["size"]
        if (type(offset) is not int or type(size) is not int or offset < 0
                or size < 1 or offset + size > len(value)):
            raise OracleFailure("shared witness population span is invalid")
        if type(minimum) is not int or not 1 <= minimum <= size:
            raise OracleFailure("shared witness population threshold is invalid")
        nonzero = sum(byte != 0 for byte in value[offset:offset + size])
        if nonzero < minimum:
            raise OracleFailure(f"shared witness 0x{fid:04X} is insufficiently populated")
        actual = hashlib.sha256(value).hexdigest()
        if actual != expected:
            raise OracleFailure(f"shared witness 0x{fid:04X} changed")
        seen.add(fid)
        checked.append({"field_id": fid, "sha256": actual, "offset": offset,
                        "size": size, "nonzero_bytes": nonzero})
    return checked


def check_travel_witnesses(source: FlashSave, destination: FlashSave, descriptor: bytes,
                          witnesses: list[dict], *, exact_source: bool) -> list[dict]:
    if witnesses and not exact_source:
        raise OracleFailure("populated travel witnesses require the exact ferry source")
    checked = check_shared_witnesses(source, descriptor, witnesses)
    check_shared_witnesses(destination, descriptor, witnesses)
    return checked


def check_projection(source: FlashSave, destination: FlashSave, template: FlashSave,
                     descriptor: bytes, *, allow_runtime_changes: frozenset[int] = frozenset(),
                     expected_generation_delta: int = 1) -> dict:
    """Check one player's staged/arrived save against source and destination template.

    ``allow_runtime_changes`` is only for explicitly named post-boot fields;
    staged saves should pass with the empty default.
    """
    schema = parse_schema_payload(descriptor)
    fields = schema["fields"]
    if source.lineage[:9] != destination.lineage[:9] or source.lineage[10:] != destination.lineage[10:]:
        raise OracleFailure("trainer lineage changed across ROMs")
    source_key, destination_key = _key(source, fields), _key(destination, fields)
    checked = {"shared": [], "world_local": [], "runtime_exceptions": sorted(allow_runtime_changes)}
    for field in fields:
        fid, owner, storage, offset, size = (field[k] for k in ("id", "ownership", "storage", "offset", "size"))
        actual = destination.field(storage, offset, size)
        if owner == OWNER_SHARED_PLAYER:
            if fid == 0x0403:
                continue
            expected = source.field(storage, offset, size)
            if fid not in allow_runtime_changes and _logical_value(fid, actual, destination_key) != _logical_value(fid, expected, source_key):
                raise OracleFailure(f"shared player field 0x{fid:04X} changed")
            checked["shared"].append(f"0x{fid:04X}")
        elif owner == OWNER_WORLD_LOCAL:
            if fid == 0x020B:  # Destination encryption key is inherently local.
                continue
            expected = template.field(storage, offset, size)
            if fid not in allow_runtime_changes and actual != expected:
                raise OracleFailure(f"destination world-local field 0x{fid:04X} changed")
            checked["world_local"].append(f"0x{fid:04X}")
        else:
            raise OracleFailure(f"unresolved descriptor owner for 0x{fid:04X}")
    if destination.generation != source.generation + expected_generation_delta:
        raise OracleFailure(f"co-op generation {source.generation} -> {destination.generation}, expected +{expected_generation_delta}")
    if source.coop[:28] != destination.coop[:28] or source.coop[32:668] != destination.coop[32:668]:
        raise OracleFailure("co-op membership/progress changed outside generation and CRC")
    checked["source_sha256"] = source.sha256
    checked["destination_sha256"] = destination.sha256
    checked["generation"] = [source.generation, destination.generation]
    return checked
