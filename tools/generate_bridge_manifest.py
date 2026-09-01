#!/usr/bin/env python3
"""Generate the mGBA bridge manifest and Lua address module from a linked ELF."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import tempfile
from dataclasses import dataclass
from pathlib import Path


BRIDGE_SYMBOL = "gCoopNetBridge"
BRIDGE_MAGIC = 0x504B434F
BRIDGE_ABI_VERSION = 1
GAME_PROTOCOL_VERSION = 1
GAME_BUILD_ID = 0x00010000
BRIDGE_SIZE = 9244
MESSAGE_SIZE = 144
QUEUE_SIZE = 4612
EWRAM_START = 0x02000000
EWRAM_END = 0x02040000


class ManifestError(RuntimeError):
    """Raised when linked artifacts do not satisfy the bridge ABI."""


@dataclass(frozen=True)
class Symbol:
    name: str
    address: int
    size: int
    kind: str


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
            # names. Only ambiguity in the ABI anchor itself is unsafe.
            if name == BRIDGE_SYMBOL:
                raise ManifestError(f"duplicate bridge symbol in nm output: {name}")
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


def validate_bridge_symbol(symbols: dict[str, Symbol]) -> Symbol:
    try:
        bridge = symbols[BRIDGE_SYMBOL]
    except KeyError as error:
        raise ManifestError(f"linked ELF does not define {BRIDGE_SYMBOL}") from error

    if bridge.size != BRIDGE_SIZE:
        raise ManifestError(
            f"{BRIDGE_SYMBOL} has size {bridge.size}, expected ABI size {BRIDGE_SIZE}"
        )
    if bridge.address < EWRAM_START or bridge.address + bridge.size > EWRAM_END:
        raise ManifestError(
            f"{BRIDGE_SYMBOL} range 0x{bridge.address:08X}.."
            f"0x{bridge.address + bridge.size:08X} is outside EWRAM"
        )
    return bridge


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def build_manifest(bridge: Symbol, rom_sha256: str) -> dict[str, object]:
    if len(rom_sha256) != 64 or any(character not in "0123456789abcdef" for character in rom_sha256):
        raise ManifestError("ROM SHA-256 must be 64 lowercase hexadecimal characters")

    return {
        "schema_version": 1,
        "game_build": {
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
    }


def render_lua(manifest: dict[str, object]) -> str:
    bridge = manifest["net_bridge"]
    build = manifest["game_build"]
    assert isinstance(bridge, dict)
    assert isinstance(build, dict)
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
    parser.add_argument("--manifest", type=Path, default=Path("dist/bridge_manifest.json"))
    parser.add_argument("--lua", type=Path, default=Path("bridge/generated_addresses.lua"))
    args = parser.parse_args()

    symbols = inspect_elf(args.elf, args.nm)
    bridge = validate_bridge_symbol(symbols)
    manifest = build_manifest(bridge, sha256_file(args.rom))
    atomic_write(args.manifest, json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    atomic_write(args.lua, render_lua(manifest))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
