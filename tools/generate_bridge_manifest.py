#!/usr/bin/env python3
"""Generate the mGBA bridge manifest and Lua address module from a linked ELF."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import stat
import struct
import subprocess
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from coop.generate_regional_identities import (
    DIGEST_SIZE as REGISTRY_DIGEST_SIZE,
    RegistryError,
    canonical_registry_bytes,
)


BRIDGE_SYMBOL = "gCoopNetBridge"
SAVE_BLOCK3_SYMBOL = "gSaveblock3"
SAVE_DESCRIPTOR_SYMBOL = "gCoopSaveSchemaDescriptor"
ABI_SYMBOLS = frozenset((BRIDGE_SYMBOL, SAVE_BLOCK3_SYMBOL, SAVE_DESCRIPTOR_SYMBOL))
MANIFEST_SCHEMA_VERSION = 3
EMULATOR_NAME = "mGBA"
EMULATOR_VERSION = "0.10.5"
EMULATOR_PLATFORM = "windows-x64"
EMULATOR_VARIANT = "Qt"
EMULATOR_ARCHIVE_SHA256 = (
    "b497a57c7d9093834dadc64f33a90f7c411439c21fdb8a0143255a45ea37563a"
)
EMULATOR_EXECUTABLE_SHA256 = (
    "5a3c98c2984dd04bd0d7c9378cdfae937ae0d73a196c880bb2eecf3b254af247"
)
BRIDGE_MAGIC = 0x504B434F
BRIDGE_ABI_VERSION = 1
GAME_PROTOCOL_VERSION = 1
GAME_BUILD_ID = 0x00010000
GAME_BUILD_TEXT_ID = "pokecrossroads-beta-1.4-e05c8286-coop-v1"
BRIDGE_SIZE = 9244
MESSAGE_SIZE = 144
QUEUE_SIZE = 4612
EWRAM_START = 0x02000000
EWRAM_END = 0x02040000
ROM_START = 0x08000000
ROM_END = 0x0A000000
UINT32_LIMIT = 1 << 32

SAVE_DESCRIPTOR_MAGIC = 0x31445343
SAVE_DESCRIPTOR_VERSION = 1
SAVE_DESCRIPTOR_SIZE = 64
SAVE_MAGIC = 0x31505343
SAVE_SCHEMA_VERSION = 1
SAVE_STRUCT_SIZE = 672
SAVE_BLOCK3_OFFSET = 4
SAVE_GENERATION_OFFSET = 28
SAVE_CRC_OFFSET = 668
SAVE_SECTOR_DATA_SIZE = 3968
SAVE_BLOCK3_CHUNK_SIZE = 116
SAVE_SECTOR_SIZE = 4096
SAVE_SECTORS_PER_SLOT = 15
SAVE_SLOT_COUNT = 2
SAVE_BLOCK3_PERSISTED_SECTORS = 6
SAVE_BLOCK3_MIN_SIZE = SAVE_BLOCK3_OFFSET + SAVE_STRUCT_SIZE
SAVE_BLOCK3_MAX_SIZE = SAVE_BLOCK3_CHUNK_SIZE * SAVE_BLOCK3_PERSISTED_SECTORS
REGISTRY_VERSION = 1
SAVE_TRAINER_BITS_OFFSET = 68
SAVE_EVENT_BITS_OFFSET = 324
SAVE_FLY_BITS_OFFSET = 580
SAVE_GYM_BITS_OFFSET = 596
SAVE_STATUS_FLAGS_OFFSET = 32
SAVE_REGIONAL_PROGRESS_OFFSET = 36
SAVE_DESCRIPTOR_STRUCT = struct.Struct("<IHHI10HI16s6H")


class ManifestError(RuntimeError):
    """Raised when linked artifacts do not satisfy the bridge ABI."""


@dataclass(frozen=True)
class Symbol:
    name: str
    address: int
    size: int
    kind: str


@dataclass(frozen=True)
class RegistryContract:
    version: int
    digest: bytes


@dataclass(frozen=True)
class SaveSchemaDescriptor:
    descriptor_magic: int
    descriptor_version: int
    descriptor_size: int
    save_magic: int
    save_schema_version: int
    save_struct_size: int
    save_block3_offset: int
    generation_offset: int
    crc32_offset: int
    sector_data_size: int
    save_block3_chunk_size: int
    sector_size: int
    sectors_per_slot: int
    save_slot_count: int
    registry_version: int
    registry_digest: bytes
    trainer_bits_offset: int
    event_bits_offset: int
    fly_bits_offset: int
    gym_bits_offset: int
    status_flags_offset: int
    regional_progress_offset: int


def validate_emulator_contract(value: object) -> dict[str, str]:
    """Return the exact official Windows Qt mGBA artifact contract."""
    expected = {
        "name": EMULATOR_NAME,
        "version": EMULATOR_VERSION,
        "platform": EMULATOR_PLATFORM,
        "variant": EMULATOR_VARIANT,
        "archive_sha256": EMULATOR_ARCHIVE_SHA256,
        "executable_sha256": EMULATOR_EXECUTABLE_SHA256,
    }
    if not isinstance(value, dict) or set(value) != set(expected):
        raise ManifestError("manifest emulator contract has unexpected fields")
    for field, wanted in expected.items():
        actual = value[field]
        if not isinstance(actual, str) or actual != wanted:
            raise ManifestError(f"manifest emulator {field} is not pinned")
    return expected.copy()


def parse_nm_symbols(output: str) -> dict[str, Symbol]:
    """Parse GNU nm's POSIX `name type value size` output."""
    symbols: dict[str, Symbol] = {}
    for line_number, raw_line in enumerate(output.splitlines(), start=1):
        line = raw_line.strip()
        if not line:
            continue
        parts = line.split()
        if len(parts) not in (3, 4):
            raise ManifestError(f"invalid nm record at line {line_number}: {line!r}")
        name, kind, address_hex = parts[:3]
        # GNU nm omits the size column for zero-sized symbols even when
        # --print-size is active. They are irrelevant to bridge validation,
        # but accepting them lets us inspect a complete linked ELF rather
        # than filtering the tool output in a platform-specific shell.
        size_hex = parts[3] if len(parts) == 4 else "0"
        if name in symbols:
            # Linked ROMs can legitimately contain repeated local or weak
            # names. Ambiguity in any externally consumed ABI anchor is unsafe.
            if name in ABI_SYMBOLS:
                raise ManifestError(f"duplicate ABI symbol in nm output: {name}")
            continue
        try:
            address = int(address_hex, 16)
            size = int(size_hex, 16)
        except ValueError as error:
            raise ManifestError(f"invalid hexadecimal nm value at line {line_number}") from error
        symbols[name] = Symbol(name=name, address=address, size=size, kind=kind)
    return symbols


def inspect_elf(elf_path: Path, nm_command: str) -> dict[str, Symbol]:
    completed = subprocess.run(
        [nm_command, "--defined-only", "--print-size", "--format=posix", str(elf_path)],
        check=False,
        capture_output=True,
        text=True,
    )
    if completed.returncode != 0:
        detail = completed.stderr.strip() or "nm returned no diagnostic"
        raise ManifestError(f"failed to inspect {elf_path}: {detail}")
    return parse_nm_symbols(completed.stdout)


def checked_u32_add(*values: int, context: str) -> int:
    total = 0
    for value in values:
        if not isinstance(value, int) or isinstance(value, bool) or value < 0:
            raise ManifestError(f"{context} contains an invalid unsigned value")
        total += value
        if total >= UINT32_LIMIT:
            raise ManifestError(f"{context} overflows a 32-bit address")
    return total


def validate_symbol_range(
    symbol: Symbol,
    *,
    start: int,
    end: int,
    minimum_size: int,
    maximum_size: int,
    region: str,
) -> None:
    if symbol.size < minimum_size or symbol.size > maximum_size:
        expected = (
            str(minimum_size)
            if minimum_size == maximum_size
            else f"{minimum_size}..{maximum_size}"
        )
        raise ManifestError(
            f"{symbol.name} has size {symbol.size}, expected {expected} bytes"
        )
    symbol_end = checked_u32_add(symbol.address, symbol.size, context=f"{symbol.name} range")
    if symbol.address < start or symbol_end > end:
        raise ManifestError(
            f"{symbol.name} range 0x{symbol.address:08X}..0x{symbol_end:08X} "
            f"is outside {region}"
        )


def validate_bridge_symbol(symbols: dict[str, Symbol]) -> Symbol:
    try:
        bridge = symbols[BRIDGE_SYMBOL]
    except KeyError as error:
        raise ManifestError(f"linked ELF does not define {BRIDGE_SYMBOL}") from error
    if bridge.name != BRIDGE_SYMBOL:
        raise ManifestError(f"linked ELF has an inconsistent {BRIDGE_SYMBOL} record")
    if bridge.address % 4 != 0:
        raise ManifestError(f"{BRIDGE_SYMBOL} is not 4-byte aligned")

    validate_symbol_range(
        bridge,
        start=EWRAM_START,
        end=EWRAM_END,
        minimum_size=BRIDGE_SIZE,
        maximum_size=BRIDGE_SIZE,
        region="EWRAM",
    )
    return bridge


def validate_save_block3_layout(block3: Symbol) -> None:
    if block3.address % 4 != 0:
        raise ManifestError(f"{SAVE_BLOCK3_SYMBOL} is not 4-byte aligned")
    validate_symbol_range(
        block3,
        start=EWRAM_START,
        end=EWRAM_END,
        minimum_size=SAVE_BLOCK3_MIN_SIZE,
        maximum_size=SAVE_BLOCK3_MAX_SIZE,
        region="EWRAM",
    )


def validate_save_descriptor_symbol_layout(descriptor: Symbol) -> None:
    if descriptor.address % 4 != 0:
        raise ManifestError(f"{SAVE_DESCRIPTOR_SYMBOL} is not 4-byte aligned")
    validate_symbol_range(
        descriptor,
        start=ROM_START,
        end=ROM_END,
        minimum_size=SAVE_DESCRIPTOR_SIZE,
        maximum_size=SAVE_DESCRIPTOR_SIZE,
        region="the GBA ROM window",
    )


def validate_save_symbols(symbols: dict[str, Symbol]) -> tuple[Symbol, Symbol]:
    try:
        block3 = symbols[SAVE_BLOCK3_SYMBOL]
    except KeyError as error:
        raise ManifestError(f"linked ELF does not define {SAVE_BLOCK3_SYMBOL}") from error
    try:
        descriptor = symbols[SAVE_DESCRIPTOR_SYMBOL]
    except KeyError as error:
        raise ManifestError(f"linked ELF does not define {SAVE_DESCRIPTOR_SYMBOL}") from error
    if block3.name != SAVE_BLOCK3_SYMBOL:
        raise ManifestError(f"linked ELF has an inconsistent {SAVE_BLOCK3_SYMBOL} record")
    if descriptor.name != SAVE_DESCRIPTOR_SYMBOL:
        raise ManifestError(
            f"linked ELF has an inconsistent {SAVE_DESCRIPTOR_SYMBOL} record"
        )
    validate_save_block3_layout(block3)
    validate_save_descriptor_symbol_layout(descriptor)
    return block3, descriptor


def read_save_descriptor_bytes(rom_path: Path, symbol: Symbol) -> bytes:
    if symbol.name != SAVE_DESCRIPTOR_SYMBOL:
        raise ManifestError(f"ROM symbol must be {SAVE_DESCRIPTOR_SYMBOL}")
    validate_save_descriptor_symbol_layout(symbol)
    offset = symbol.address - ROM_START
    end = offset + symbol.size
    try:
        with rom_path.open("rb") as stream:
            metadata = os.fstat(stream.fileno())
            if not stat.S_ISREG(metadata.st_mode):
                raise ManifestError(f"ROM input is not a regular file: {rom_path}")
            if end > metadata.st_size:
                raise ManifestError(
                    f"{symbol.name} ROM range 0x{offset:X}..0x{end:X} exceeds "
                    f"{rom_path} ({metadata.st_size} bytes)"
                )
            stream.seek(offset)
            payload = stream.read(symbol.size)
    except OSError as error:
        raise ManifestError(f"failed to read ROM {rom_path}: {error}") from error
    if len(payload) != symbol.size:
        raise ManifestError(f"short read for {symbol.name} from {rom_path}")
    return payload


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def registry_contract(value: Any) -> RegistryContract:
    if not isinstance(value, dict):
        raise ManifestError("identity registry root must be a JSON object")
    version = value.get("registry_version")
    if not isinstance(version, int) or isinstance(version, bool):
        raise ManifestError("identity registry version must be an integer")
    if version != REGISTRY_VERSION:
        raise ManifestError(
            f"identity registry version {version} does not match supported version "
            f"{REGISTRY_VERSION}"
        )
    try:
        canonical = canonical_registry_bytes(value)
    except RegistryError as error:
        raise ManifestError("identity registry is not canonical JSON data") from error
    return RegistryContract(
        version=version,
        digest=hashlib.sha256(canonical).digest()[:REGISTRY_DIGEST_SIZE],
    )


def load_registry_contract(path: Path) -> RegistryContract:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise ManifestError(f"failed to read identity registry {path}: {error}") from error
    return registry_contract(value)


def parse_save_descriptor(payload: bytes) -> SaveSchemaDescriptor:
    if len(payload) != SAVE_DESCRIPTOR_STRUCT.size:
        raise ManifestError(
            f"save schema descriptor has size {len(payload)}, expected "
            f"{SAVE_DESCRIPTOR_STRUCT.size}"
        )
    return SaveSchemaDescriptor(*SAVE_DESCRIPTOR_STRUCT.unpack(payload))


def validate_save_descriptor(
    descriptor: SaveSchemaDescriptor,
    registry: RegistryContract,
) -> None:
    expected = {
        "descriptor_magic": SAVE_DESCRIPTOR_MAGIC,
        "descriptor_version": SAVE_DESCRIPTOR_VERSION,
        "descriptor_size": SAVE_DESCRIPTOR_SIZE,
        "save_magic": SAVE_MAGIC,
        "save_schema_version": SAVE_SCHEMA_VERSION,
        "save_struct_size": SAVE_STRUCT_SIZE,
        "save_block3_offset": SAVE_BLOCK3_OFFSET,
        "generation_offset": SAVE_GENERATION_OFFSET,
        "crc32_offset": SAVE_CRC_OFFSET,
        "sector_data_size": SAVE_SECTOR_DATA_SIZE,
        "save_block3_chunk_size": SAVE_BLOCK3_CHUNK_SIZE,
        "sector_size": SAVE_SECTOR_SIZE,
        "sectors_per_slot": SAVE_SECTORS_PER_SLOT,
        "save_slot_count": SAVE_SLOT_COUNT,
        "registry_version": registry.version,
        "registry_digest": registry.digest,
        "trainer_bits_offset": SAVE_TRAINER_BITS_OFFSET,
        "event_bits_offset": SAVE_EVENT_BITS_OFFSET,
        "fly_bits_offset": SAVE_FLY_BITS_OFFSET,
        "gym_bits_offset": SAVE_GYM_BITS_OFFSET,
        "status_flags_offset": SAVE_STATUS_FLAGS_OFFSET,
        "regional_progress_offset": SAVE_REGIONAL_PROGRESS_OFFSET,
    }
    for field, wanted in expected.items():
        actual = getattr(descriptor, field)
        if actual != wanted:
            if isinstance(wanted, bytes):
                actual_text = actual.hex()
                wanted_text = wanted.hex()
            else:
                actual_text = str(actual)
                wanted_text = str(wanted)
            raise ManifestError(
                f"{SAVE_DESCRIPTOR_SYMBOL}.{field} is {actual_text}, expected {wanted_text}"
            )


def validate_game_build_id(build_id: str) -> str:
    """Validate the canonical textual build ID using coop-cloud's wire contract."""
    if not isinstance(build_id, str):
        raise ManifestError("game build ID must be a string")
    try:
        encoded = build_id.encode("ascii")
    except UnicodeEncodeError as error:
        raise ManifestError("game build ID must contain only ASCII characters") from error

    if not encoded or len(encoded) > 128:
        raise ManifestError("game build ID must contain between 1 and 128 ASCII bytes")
    allowed = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789._+:-"
    if any(byte not in allowed for byte in encoded):
        raise ManifestError(
            "game build ID may contain only ASCII alphanumerics or ._+:-"
        )
    return build_id


def build_manifest(
    bridge: Symbol,
    block3: Symbol,
    descriptor: SaveSchemaDescriptor,
    registry: RegistryContract,
    rom_sha256: str,
) -> dict[str, object]:
    if not isinstance(rom_sha256, str) or len(rom_sha256) != 64 or any(
        character not in "0123456789abcdef" for character in rom_sha256
    ):
        raise ManifestError("ROM SHA-256 must be 64 lowercase hexadecimal characters")
    validate_bridge_symbol({BRIDGE_SYMBOL: bridge})
    if block3.name != SAVE_BLOCK3_SYMBOL:
        raise ManifestError(f"save block symbol must be {SAVE_BLOCK3_SYMBOL}")
    validate_save_block3_layout(block3)
    validate_save_descriptor(descriptor, registry)

    coop_address = checked_u32_add(
        block3.address,
        descriptor.save_block3_offset,
        context="co-op save address",
    )
    coop_end = checked_u32_add(
        coop_address,
        descriptor.save_struct_size,
        context="co-op save range",
    )
    block3_end = checked_u32_add(
        block3.address,
        block3.size,
        context=f"{SAVE_BLOCK3_SYMBOL} range",
    )
    generation_address = checked_u32_add(
        coop_address,
        descriptor.generation_offset,
        context="live save generation address",
    )
    generation_end = checked_u32_add(
        generation_address,
        4,
        context="live save generation range",
    )
    crc_end = checked_u32_add(
        coop_address,
        descriptor.crc32_offset,
        4,
        context="co-op save CRC range",
    )
    if coop_end > block3_end or generation_end > coop_end or crc_end > coop_end:
        raise ManifestError("co-op save descriptor exceeds the linked gSaveblock3 range")

    return {
        "schema_version": MANIFEST_SCHEMA_VERSION,
        "emulator": validate_emulator_contract(
            {
                "name": EMULATOR_NAME,
                "version": EMULATOR_VERSION,
                "platform": EMULATOR_PLATFORM,
                "variant": EMULATOR_VARIANT,
                "archive_sha256": EMULATOR_ARCHIVE_SHA256,
                "executable_sha256": EMULATOR_EXECUTABLE_SHA256,
            }
        ),
        "game_build": {
            "id": validate_game_build_id(GAME_BUILD_TEXT_ID),
            "numeric_id": GAME_BUILD_ID,
            "rom_sha256": rom_sha256,
        },
        "net_bridge": {
            "symbol": bridge.name,
            "address": bridge.address,
            "size": bridge.size,
            "magic": BRIDGE_MAGIC,
            "abi_version": BRIDGE_ABI_VERSION,
            "game_protocol_version": GAME_PROTOCOL_VERSION,
            "byte_order": "little",
            "checksum": {
                "algorithm": "CRC-32/IEEE",
                "covered_bytes": [0, 139],
                "stored_offset": 140,
            },
            "offsets": {
                "magic": 0,
                "abi_version": 4,
                "game_protocol_version": 6,
                "game_build_id": 8,
                "status_flags": 12,
                "last_sidecar_heartbeat": 16,
                "game_to_network": 20,
                "network_to_game": 20 + QUEUE_SIZE,
            },
            "queue": {
                "capacity": 32,
                "size": QUEUE_SIZE,
                "read_index_offset": 0,
                "write_index_offset": 2,
                "entries_offset": 4,
            },
            "message": {
                "size": MESSAGE_SIZE,
                "payload_size": 128,
                "offsets": {
                    "type": 0,
                    "length": 2,
                    "sequence": 4,
                    "session_epoch": 8,
                    "payload": 12,
                    "checksum": 140,
                },
            },
        },
        "save": {
            "block3_address": block3.address,
            "coop_offset": descriptor.save_block3_offset,
            "generation_offset": descriptor.generation_offset,
            "generation_address": generation_address,
            "crc_offset": descriptor.crc32_offset,
            "schema_version": descriptor.save_schema_version,
            "struct_size": descriptor.save_struct_size,
            "registry_version": descriptor.registry_version,
            "registry_digest": descriptor.registry_digest.hex(),
        },
    }


def render_lua(manifest: dict[str, object]) -> str:
    bridge = manifest["net_bridge"]
    build = manifest["game_build"]
    save = manifest["save"]
    assert isinstance(bridge, dict)
    assert isinstance(build, dict)
    assert isinstance(save, dict)
    offsets = bridge["offsets"]
    queue = bridge["queue"]
    message = bridge["message"]
    assert isinstance(offsets, dict)
    assert isinstance(queue, dict)
    assert isinstance(message, dict)
    message_offsets = message["offsets"]
    assert isinstance(message_offsets, dict)

    return f"""-- Generated by tools/generate_bridge_manifest.py; do not edit.
return {{
  schema_version = {manifest['schema_version']},
  game_build_id = 0x{int(build['numeric_id']):08X},
  rom_sha256 = \"{build['rom_sha256']}\",
  magic = 0x{int(bridge['magic']):08X},
  abi_version = {bridge['abi_version']},
  protocol_version = {bridge['game_protocol_version']},
  address = 0x{int(bridge['address']):08X},
  size = {bridge['size']},
  save = {{
    block3_address = 0x{int(save['block3_address']):08X},
    coop_offset = {save['coop_offset']},
    generation_offset = {save['generation_offset']},
    generation_address = 0x{int(save['generation_address']):08X},
    crc_offset = {save['crc_offset']},
    schema_version = {save['schema_version']},
    struct_size = {save['struct_size']},
    registry_version = {save['registry_version']},
    registry_digest = "{save['registry_digest']}",
  }},
  offsets = {{
    status_flags = {offsets['status_flags']},
    last_sidecar_heartbeat = {offsets['last_sidecar_heartbeat']},
    game_to_network = {offsets['game_to_network']},
    network_to_game = {offsets['network_to_game']},
  }},
  queue = {{
    capacity = {queue['capacity']},
    size = {queue['size']},
    read_index = {queue['read_index_offset']},
    write_index = {queue['write_index_offset']},
    entries = {queue['entries_offset']},
  }},
  message = {{
    size = {message['size']},
    payload_size = {message['payload_size']},
    type = {message_offsets['type']},
    length = {message_offsets['length']},
    sequence = {message_offsets['sequence']},
    session_epoch = {message_offsets['session_epoch']},
    payload = {message_offsets['payload']},
    checksum = {message_offsets['checksum']},
  }},
}}
"""


def atomic_write(path: Path, contents: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary_name = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8", newline="\n") as stream:
            stream.write(contents)
        os.replace(temporary_name, path)
    except BaseException:
        Path(temporary_name).unlink(missing_ok=True)
        raise


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--elf", type=Path, required=True)
    parser.add_argument("--rom", type=Path, required=True)
    parser.add_argument("--nm", default=os.environ.get("ARM_NM", "arm-none-eabi-nm"))
    parser.add_argument(
        "--registry",
        type=Path,
        default=Path("data/coop/regional_identities.json"),
    )
    parser.add_argument("--manifest", type=Path, default=Path("dist/bridge_manifest.json"))
    parser.add_argument("--lua", type=Path, default=Path("bridge/generated_addresses.lua"))
    args = parser.parse_args()

    symbols = inspect_elf(args.elf, args.nm)
    bridge = validate_bridge_symbol(symbols)
    block3, descriptor_symbol = validate_save_symbols(symbols)
    registry = load_registry_contract(args.registry)
    descriptor = parse_save_descriptor(
        read_save_descriptor_bytes(args.rom, descriptor_symbol)
    )
    manifest = build_manifest(
        bridge,
        block3,
        descriptor,
        registry,
        sha256_file(args.rom),
    )
    atomic_write(args.manifest, json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    atomic_write(args.lua, render_lua(manifest))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
