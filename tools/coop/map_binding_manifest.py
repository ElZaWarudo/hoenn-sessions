#!/usr/bin/env python3
"""Bind arrival map/layout IDs to the linked map tables in a shipped GBA ROM."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess

ROM_START = 0x08000000
ROM_END = 0x0A000000
SYMBOLS = ("gMapGroups", "gMapLayouts")


class MapBindingError(ValueError):
    pass


def _number(value: object, label: str) -> int:
    if type(value) is not int or value < 0:
        raise MapBindingError(f"invalid {label}")
    return value


def _read(rom: bytes, address: int, size: int) -> bytes:
    if (type(address) is not int or address < ROM_START or address % 4
            or size < 1 or address + size > ROM_END
            or address - ROM_START + size > len(rom)):
        raise MapBindingError("map pointer is null, unaligned, or outside shipped ROM")
    return rom[address - ROM_START:address - ROM_START + size]


def _pointer(rom: bytes, address: int) -> int:
    value = struct.unpack("<I", _read(rom, address, 4))[0]
    _read(rom, value, 4)
    return value


def group_lengths(groups_path: Path) -> list[int]:
    data = json.loads(groups_path.read_text(encoding="utf-8"))
    order = data.get("group_order")
    if (not isinstance(order, list) or not order or len(order) > 256
            or len(set(order)) != len(order)):
        raise MapBindingError("invalid map group order")
    lengths = []
    for name in order:
        maps = data.get(name)
        if not isinstance(maps, list) or not maps or len(maps) > 256:
            raise MapBindingError(f"invalid map group {name}")
        lengths.append(len(maps))
    return lengths


def layout_count(layouts_path: Path) -> int:
    data = json.loads(layouts_path.read_text(encoding="utf-8"))
    layouts = data.get("layouts")
    if (data.get("layouts_table_label") != "gMapLayouts" or not isinstance(layouts, list)
            or not layouts or len(layouts) > 65535):
        raise MapBindingError("invalid layout source table")
    root = layouts_path.resolve().parents[2]
    for layout in layouts:
        if (not isinstance(layout, dict) or not isinstance(layout.get("border_filepath"), str)
                or not (root / layout["border_filepath"]).is_file()):
            raise MapBindingError("layout source table contains an omitted border file")
    return len(layouts)


def symbols_from_nm(output: str) -> dict[str, tuple[int, int]]:
    found: dict[str, list[tuple[int, int]]] = {name: [] for name in SYMBOLS}
    for line in output.splitlines():
        parts = line.split()
        if parts and parts[0] in found:
            if len(parts) not in (3, 4) or parts[1] not in ("r", "R", "d", "D"):
                raise MapBindingError("map table has invalid ELF symbol")
            try:
                found[parts[0]].append((int(parts[2], 16), int(parts[3], 16) if len(parts) == 4 else 0))
            except ValueError as exc:
                raise MapBindingError("map table has invalid ELF address") from exc
    if any(len(matches) != 1 or matches[0][0] % 4 or matches[0][1] % 4
           for matches in found.values()):
        raise MapBindingError("map table has missing or invalid ELF symbol")
    return {name: matches[0] for name, matches in found.items()}


def verify_arrival(rom: bytes, manifest: dict, group: int, number: int,
                   layout_id: int) -> None:
    """Follow the game's gMapGroups lookup and compare against gMapLayouts."""
    if not isinstance(manifest, dict) or manifest.get("schema_version") != 1:
        raise MapBindingError("invalid map binding manifest schema")
    if manifest.get("rom_sha256") != hashlib.sha256(rom).hexdigest():
        raise MapBindingError("map binding manifest does not match ROM")
    lengths = manifest.get("group_lengths")
    groups = manifest.get("gMapGroups")
    layouts = manifest.get("gMapLayouts")
    if (not isinstance(lengths, list) or not lengths or len(lengths) > 256
            or any(type(n) is not int or not 1 <= n <= 256 for n in lengths)
            or not isinstance(groups, dict) or not isinstance(layouts, dict)):
        raise MapBindingError("invalid map binding manifest tables")
    group_address = _number(groups.get("address"), "group table address")
    group_size = _number(groups.get("size"), "group table size")
    layout_address = _number(layouts.get("address"), "layout table address")
    layout_size = _number(layouts.get("size"), "layout table size")
    if group_size != 4 * len(lengths) or layout_size < 4 or layout_size % 4:
        raise MapBindingError("map table size does not match manifest")
    _read(rom, group_address, group_size)
    _read(rom, layout_address, layout_size)
    if (type(group) is not int or not 0 <= group <= 127
            or type(number) is not int or not 0 <= number <= 127):
        raise MapBindingError("arrival map coordinate exceeds signed WarpData range")
    if (group >= len(lengths) or number >= lengths[group]
            or type(layout_id) is not int or not 1 <= layout_id <= layout_size // 4):
        raise MapBindingError("arrival map or layout index is outside linked tables")
    map_group = _pointer(rom, group_address + 4 * group)
    header = _pointer(rom, map_group + 4 * number)
    _read(rom, header, 0x14)
    linked_layout_id = struct.unpack("<H", _read(rom, header + 0x10, 4)[2:])[0]
    linked_layout = _pointer(rom, header)
    layout_pointer = _pointer(rom, layout_address + 4 * (layout_id - 1))
    if linked_layout_id != layout_id or linked_layout != layout_pointer:
        raise MapBindingError("arrival map/layout does not match shipped ROM")


def build_manifest(elf: Path, rom_path: Path, groups_path: Path,
                   layouts_path: Path, nm: str) -> dict:
    try:
        result = subprocess.run([nm, "--defined-only", "--print-size", "--format=posix", str(elf)],
                                capture_output=True, text=True, check=False)
    except OSError as exc:
        raise MapBindingError(f"cannot run nm: {exc}") from exc
    if result.returncode:
        raise MapBindingError(result.stderr.strip() or "nm failed")
    symbols = symbols_from_nm(result.stdout)
    rom = rom_path.read_bytes()
    lengths = group_lengths(groups_path)
    sizes = {"gMapGroups": len(lengths) * 4, "gMapLayouts": layout_count(layouts_path) * 4}
    for name, (address, reported_size) in symbols.items():
        if reported_size not in (0, sizes[name]):
            raise MapBindingError(f"linked {name} size disagrees with source table")
        size = sizes[name]
        _read(rom, address, size)
    return {
        "schema_version": 1,
        "rom_sha256": hashlib.sha256(rom).hexdigest(),
        "gMapGroups": {"address": symbols["gMapGroups"][0], "size": sizes["gMapGroups"]},
        "gMapLayouts": {"address": symbols["gMapLayouts"][0], "size": sizes["gMapLayouts"]},
        "group_lengths": lengths,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--elf", type=Path, required=True)
    parser.add_argument("--rom", type=Path, required=True)
    parser.add_argument("--groups", type=Path, default=Path("data/maps/map_groups.json"))
    parser.add_argument("--layouts", type=Path, default=Path("data/layouts/layouts.json"))
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--nm", default=os.environ.get("ARM_NM", "arm-none-eabi-nm"))
    args = parser.parse_args()
    try:
        manifest = build_manifest(args.elf, args.rom, args.groups, args.layouts, args.nm)
    except (MapBindingError, OSError, ValueError) as exc:
        parser.exit(1, f"map binding manifest: {exc}\n")
    args.manifest.parent.mkdir(parents=True, exist_ok=True)
    args.manifest.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
