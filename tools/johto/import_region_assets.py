"""Validate and stage the complete selected Johto scenery corpus.

This module is deliberately a tooling boundary.  It reads the pinned donor,
converts only the donor's two-layer metatile attributes into the host u32
representation, and writes to an explicitly supplied staging directory.  The
bootstrap mode writes the owned manifest only; it never touches runtime asset
directories.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import struct
import subprocess
import sys
import zlib
from pathlib import Path
from typing import Any, Iterable


ROOT = Path(__file__).resolve().parents[2]
REGION_MANIFEST = ROOT / "data/johto/region_manifest.json"
ANIMATION_MANIFEST = ROOT / "data/johto/tileset_animations.json"
OUTPUT_MANIFEST = ROOT / "data/johto/asset_manifest.json"

DONOR_REVISION = "751823abaf677020bcd72c45fe3e7cb2b8a576e4"
DONOR_TREE = "33661709e5368edc01c37ed9bb5e0a7a0cb192c8"
DONOR_REPOSITORY = "https://github.com/PokemonHnS-Development/pokemonHnS"
REGION_MANIFEST_SOURCE_REVISION = "21b8c9f918800a07b74a5ee2a882b1374d9ac4f9"

PRIMARY_METATILE_COUNT = 640
GENERAL_METATILE_COUNT = 512
GENERAL_TILESET_SYMBOL = "gTileset_General"
METATILE_BYTES = 16
ATTRIBUTE_BYTES = 2
CONVERTED_ATTRIBUTE_BYTES = 4
PNG_BUDGET_4BPP = 0x20000

# The pinned donor's Route7 map contains a deliberately bounded pair of
# malformed forest rows.  Keep this conversion source-bound: it is a local
# repair of the donor bytes, not a general tile rewrite policy.
ROUTE7_MAP_SOURCE_PATH = "data/layouts/Route7/map.bin"
ROUTE7_MAP_SOURCE_SHA256 = "558557c41d23981e78967839798c78d31a004174e9d7e14aec0de69c524d4ff9"
ROUTE7_MAP_OUTPUT_SHA256 = "333f993fa119a62a3e80c6e1135ad107125286bbdac11c572742f043daca88dd"
ROUTE7_MAP_WIDTH = 22
ROUTE7_MAP_HEIGHT = 40
ROUTE7_MAP_REPAIR_MASK = 0x0400
ROUTE7_MAP_REPAIRS = (
    (170, 16, 7, 0x06E7, 0x0414),
    (171, 17, 7, 0x06E7, 0x0415),
    (172, 18, 7, 0x06E7, 0x0414),
    (173, 19, 7, 0x06E7, 0x0415),
    (174, 20, 7, 0x06E7, 0x0414),
    (175, 21, 7, 0x06E7, 0x0415),
    (192, 16, 8, 0x0786, 0x041C),
    (193, 17, 8, 0x0786, 0x041D),
    (194, 18, 8, 0x0786, 0x041C),
    (195, 19, 8, 0x0786, 0x041D),
    (196, 20, 8, 0x0786, 0x041C),
    (197, 21, 8, 0x0786, 0x041D),
)
ACCEPTED_PREDECESSOR_SHA256S = {
    "c6dbb8d0b8aecbbe3acc473cff39c62988e211bed8bf664220a7972cd045ce73",
    "89e7b59ff06145af784ee66ee678edeeb5ffcf0520fabc8f8b9e48e4bb437fb5",
}

# The pinned donor has a small, source-backed set of attribute exceptions.  A
# path is eligible only when its complete source hash matches this ledger.  The
# bytes remain represented by source_sha256 in the generated manifest.
PINNED_ATTRIBUTE_EXCEPTION_SHAS = {
    "data/tilesets/secondary/department_store/metatile_attributes.bin": "1b2ff4b828e4e04559ad2323d9bb9a29eb7ca8502fc1d57a850618a5957273e1",
    "data/tilesets/secondary/shop_rooftop/metatile_attributes.bin": "37b7cb5f8671921415d7e48de78c0ecf3af756c87999321bb4ea9d7f6b92c4e0",
    "data/tilesets/secondary/burned_tower/metatile_attributes.bin": "7097b8eb86ca7d5343e2724a69e5843e748a2349f0d8c9e935c917e885008b2b",
    "data/tilesets/secondary/cafe/metatile_attributes.bin": "515b782f5f8bd78387a25a073262cf97bff0f4a4dae04ab27041abfc8e3d0a1d",
    "data/tilesets/secondary/cave_gray/metatile_attributes.bin": "c69709db4738be9c0074d47dc5dee4fe5813191eb22c182926bb11a9dfba13d7",
    "data/tilesets/secondary/dragons_den_shrine/metatile_attributes.bin": "7c7aaaae7309913e5b58f49601eaa5bc2ddd1dcca13ed263d9b1894cbc130c90",
    "data/tilesets/secondary/ecruteak_theater/metatile_attributes.bin": "3c10aa0a895c1f5c01d3f06d5650672e63833d91cf3b32054186ea7b5884d6c4",
    "data/tilesets/secondary/goldenrod_underground_rocket/metatile_attributes.bin": "1ac3153f29debb58f0d3d931dfcb3d49d69e3d20e5fe10602c0fdd66d346e155",
    "data/tilesets/secondary/goldenrod_underground_storage/metatile_attributes.bin": "7ea8c2421bf14c0a71099410226a1be90ca8da3b9d30d36b45ff59db1e157d4c",
    "data/tilesets/secondary/johto_mart/metatile_attributes.bin": "9b621d64d6793b991cdf9e040e9073bdb56fb9803d775ac27427410391588f69",
    "data/tilesets/primary/kanto_general/metatile_attributes.bin": "c328b0066815be7db5d02c274f1826c0ac292f18104f491f5613bfb2349b72db",
    "data/tilesets/secondary/lighthouse/metatile_attributes.bin": "1055c4982829b2753ffae713ceda6651580973b3655b6a896673240cedb1d382",
    "data/tilesets/secondary/national_park/metatile_attributes.bin": "504e26ba64ee634a1f018ee7733681a8d8b026c87b9aeb1c978bcd47fa997cd8",
    "data/tilesets/secondary/ssaqua/metatile_attributes.bin": "ec8f78f7789ecbcb0c2b0b577b1f862b84fa3b1bbe29e834b657c517f03b7fd4",
    "data/tilesets/secondary/cave_green/metatile_attributes.bin": "f51df6e3203fd08943bc2cace99d44340591d8ec4fb041fef1aca7a5a67fefb0",
    "data/tilesets/secondary/cave_mt_moon/metatile_attributes.bin": "497a9937af7d3eae0f37ef87d69aecae46f31fde0a3a2bfcb2fa6487b669d723",
    "data/tilesets/secondary/cave_sandy/metatile_attributes.bin": "abc0c8bdf052ef9c793c3ee1dc8a7f3c454d0ab3c8aeac4de0de90022d34b0cf",
    "data/tilesets/secondary/celadon_apartments/metatile_attributes.bin": "c35fa4c91c7bdba04233f8caa3a996768ee1053302c129d3f522294e21e20f06",
    "data/tilesets/secondary/cerulean_city/metatile_attributes.bin": "c50a3c2d210c70d1e4062d5ad412e2e350fb365857e78480b291f1d65701911d",
    "data/tilesets/secondary/indigo_plateau/metatile_attributes.bin": "b2a60a81b444a90cd48501a38b700810b99ec8489807d0d616d9aa42791d310f",
    "data/tilesets/secondary/saffron_city_dojo_vip/metatile_attributes.bin": "2b372b640c9346731d308f510feb3acfa3ebdb64d5c0c6ae74b8d26f7079a72c",
    "data/tilesets/secondary/silph_co/metatile_attributes.bin": "456e1e4a3832378def0f4c2511b4df5c0df68fdc8a07ae421134781127e2dcb8",
    "data/tilesets/secondary/soul_house/metatile_attributes.bin": "501892055c0368cb48e48ffb379304c897f64e205d0df5839149b7dfd1ff074e",
    "data/tilesets/secondary/viridian_city_gym/metatile_attributes.bin": "5c777813d7e6064fc1232d4ca3a1fb5d5bebce4393dc0b05f5381be9c73ff10f",
}

# Exact unused slots containing bytes with no donor behavior definition.
# The complete file hash above is checked before this index/value repair.
PINNED_UNDEFINED_BEHAVIORS = {
    "data/tilesets/secondary/cave_gray/metatile_attributes.bin": {330: 0x10FE, 367: 0x10FE},
    "data/tilesets/secondary/department_store/metatile_attributes.bin": {255: 0x00F1},
    "data/tilesets/secondary/ecruteak_theater/metatile_attributes.bin": {179: 0x10F0, 346: 0x10F0, 349: 0x00FC},
    "data/tilesets/secondary/shop_rooftop/metatile_attributes.bin": {255: 0x00F1},
    "data/tilesets/secondary/ssaqua/metatile_attributes.bin": {368: 0x20FE},
    "data/tilesets/secondary/celadon_apartments/metatile_attributes.bin": {265: 0x3055, 318: 0x3056},
    "data/tilesets/secondary/cerulean_city/metatile_attributes.bin": {332: 0x0056},
    "data/tilesets/secondary/silph_co/metatile_attributes.bin": {64: 0x0056, 66: 0x0055},
    "data/tilesets/secondary/viridian_city_gym/metatile_attributes.bin": {9: 0x0055, 12: 0x0056},
}

SPECIAL_BEHAVIORS = {
    "MB_HEADBUTT_TREE": {
        "source_value": 0xA1,
        "target_symbol": "MB_JOHTO_HEADBUTT_TREE",
        "target_value": 0xF0,
        "reason": "reserved for the later Johto headbutt interaction adapter",
    },
    "MB_WATER_NORTH_ARROW_WARP": {
        "source_value": 0xEF,
        "target_symbol": "MB_JOHTO_WATER_NORTH_ARROW_WARP",
        "target_value": 0xF1,
        "reason": "reserved for the later north-arrow surf/water warp adapter",
    },
    "MB_UNUSED_2D": {
        "source_value": 0x2D,
        "target_symbol": "MB_JOHTO_DEOXYS_ATTACK",
        "target_value": 0xF3,
        "reason": "preserve the donor Deoxys form marker for a future scoped helper",
    },
}
INERT_BEHAVIORS = {
    "MB_UNUSED_1E", "MB_UNUSED_23", "MB_UNUSED_58", "MB_UNUSED_A3",
    "MB_UNUSED_A4", "MB_UNUSED_A5", "MB_UNUSED_A6", "MB_UNUSED_A8",
    "MB_UNUSED_AB", "MB_UNUSED_AC", "MB_UNUSED_AE", "MB_UNUSED_AF",
    "MB_UNUSED_C8", "MB_UNUSED_C9", "MB_UNUSED_CA", "MB_UNUSED_EE",
}
INERT_VALUE = 0xF2

# These IDs are reserved by the Johto runtime lane.  They may be absent while
# this tooling lane runs, but if a consumer has already declared one its value
# and occupancy must agree exactly with the frozen hand-off contract.
JOHTO_RESERVATIONS = {
    "MB_JOHTO_HEADBUTT_TREE": 0xF0,
    "MB_JOHTO_WATER_NORTH_ARROW_WARP": 0xF1,
    "MB_JOHTO_INERT": 0xF2,
    "MB_JOHTO_DEOXYS_ATTACK": 0xF3,
}

TOKEN_RE = re.compile(r"^#define[ \t]+(MB_[A-Za-z0-9_]+)[ \t]+(.+)$")
INCBIN_RE = re.compile(
    r"INCBIN_U(?:16|32)\(\"(data/tilesets/[^\"]+)\"\)"
)


class AssetError(ValueError):
    """Fail-closed source, geometry, conversion, or staging error."""


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def json_identity(value: Any) -> str:
    return sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode())


def load_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise AssetError(f"missing JSON: {path}") from exc
    except json.JSONDecodeError as exc:
        raise AssetError(f"invalid JSON in {path}: {exc}") from exc


def git_pin(donor: Path) -> tuple[str, str]:
    try:
        revision = subprocess.check_output(
            ["git", "-C", str(donor), "rev-parse", "HEAD"], text=True,
            stderr=subprocess.STDOUT,
        ).strip()
        tree = subprocess.check_output(
            ["git", "-C", str(donor), "rev-parse", "HEAD^{tree}"], text=True,
            stderr=subprocess.STDOUT,
        ).strip()
    except (OSError, subprocess.CalledProcessError) as exc:
        raise AssetError(f"donor is not a readable git checkout: {donor}") from exc
    return revision, tree


def assert_donor_clean(donor: Path) -> None:
    if not (donor / ".git").exists():
        return
    try:
        status = subprocess.check_output(
            ["git", "-C", str(donor), "status", "--porcelain=v1",
             "--untracked-files=all", "--", "data/layouts", "data/tilesets",
             "data/maps", "src/data/tilesets", "include/constants"],
            text=True, stderr=subprocess.STDOUT,
        ).strip()
    except (OSError, subprocess.CalledProcessError) as exc:
        raise AssetError("unable to verify donor cleanliness") from exc
    if status:
        raise AssetError("donor asset inputs are dirty: " + status)


def safe_relative(path: str) -> str:
    candidate = Path(path)
    if candidate.is_absolute() or candidate.drive or ".." in candidate.parts:
        raise AssetError(f"unsafe source path: {path}")
    normalized = candidate.as_posix()
    if not normalized or normalized.startswith("/"):
        raise AssetError(f"unsafe source path: {path}")
    return normalized


def _parse_numeric(value: str) -> int | None:
    value = value.split("//", 1)[0].split("/*", 1)[0].strip()
    try:
        return int(value, 0)
    except ValueError:
        return None


def parse_donor_behaviors(path: Path) -> dict[str, int]:
    values: dict[str, int] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        match = TOKEN_RE.match(line.strip())
        if not match:
            continue
        value = _parse_numeric(match.group(2))
        if value is not None:
            values[match.group(1)] = value
    if not values:
        raise AssetError("donor behavior header contains no numeric MB definitions")
    return values


def parse_host_behaviors(path: Path) -> dict[str, int]:
    values: dict[str, int] = {}
    in_enum = False
    next_value = 0
    for line in path.read_text(encoding="utf-8").splitlines():
        stripped = line.split("//", 1)[0].strip()
        if stripped.startswith("#define MB_"):
            parts = stripped.split(None, 2)
            if len(parts) == 3:
                parsed = _parse_numeric(parts[2])
                if parsed is not None:
                    values[parts[1]] = parsed
        if stripped.startswith("enum"):
            in_enum = True
            next_value = 0
            continue
        if not in_enum:
            continue
        if stripped.startswith("};"):
            in_enum = False
            continue
        match = re.match(r"(MB_[A-Za-z0-9_]+)(?:[ \t]*=[ \t]*([^,]+))?[ ,]*$", stripped)
        if not match:
            continue
        symbol, explicit = match.groups()
        if explicit is not None:
            parsed = _parse_numeric(explicit)
            if parsed is None:
                continue
            next_value = parsed
        values[symbol] = next_value
        next_value += 1
    if "MB_INVALID" not in values:
        values["MB_INVALID"] = 0xFF
    if "MB_ROCK_CLIMB" not in values or values["MB_ROCK_CLIMB"] != 0xEF:
        raise AssetError("host behavior baseline has drifted at MB_ROCK_CLIMB")
    by_value: dict[int, list[str]] = {}
    for symbol, value in values.items():
        by_value.setdefault(value, []).append(symbol)
    for symbol, expected in JOHTO_RESERVATIONS.items():
        actual = values.get(symbol)
        if actual is not None and actual != expected:
            raise AssetError(f"host reserved behavior drifted: {symbol}={actual:#04x}")
        occupants = [name for name in by_value.get(expected, []) if name != symbol]
        if occupants:
            raise AssetError(f"host reserved behavior occupied at {expected:#04x}: {occupants[0]}")
    return values


def parse_tileset_sources(donor: Path) -> dict[str, dict[str, Any]]:
    graphics = (donor / "src/data/tilesets/graphics.h").read_text(encoding="utf-8")
    # General is declared in the donor's top-level graphics.c while the
    # remaining selected source arrays live in the generated graphics header.
    # Parse both files so the actual 512-entry primary table is source-bound.
    graphics_extra = donor / "src/graphics.c"
    if graphics_extra.exists():
        graphics += "\n" + graphics_extra.read_text(encoding="utf-8")
    metatiles = (donor / "src/data/tilesets/metatiles.h").read_text(encoding="utf-8")
    headers = (donor / "src/data/tilesets/headers.h").read_text(encoding="utf-8")
    def array_paths(pattern: str) -> dict[str, str]:
        return {match.group(1): match.group(2) for match in re.finditer(pattern, metatiles)}

    meta_paths = array_paths(
        r"const u16 (gMetatiles_[A-Za-z0-9_]+)\[\][^=]*=\s*INCBIN_U16\(\"(data/tilesets/[^\"]+)\"\)"
    )
    attr_paths = array_paths(
        r"const u16 (gMetatileAttributes_[A-Za-z0-9_]+)\[\][^=]*=\s*INCBIN_U16\(\"(data/tilesets/[^\"]+)\"\)"
    )
    tile_paths = {}
    for match in re.finditer(
        r"const u32 (gTilesetTiles_[A-Za-z0-9_]+)\[\][^=]*=\s*"
        r"INCBIN_U32\(\"(data/tilesets/[^\"]+)\"\)", graphics
    ):
        path = match.group(2)
        if not (donor / path).exists() and path.endswith("/tiles.4bpp.lz"):
            path = path.removesuffix("tiles.4bpp.lz") + "tiles.png"
        tile_paths[match.group(1)] = path
    palette_paths: dict[str, list[str]] = {}
    for match in re.finditer(
        r"const u16 (?:ALIGNED\(4\)\s*)?(gTilesetPalettes_[A-Za-z0-9_]+)\[\]\[16\] =\s*\{(.*?)\};",
        graphics, re.DOTALL,
    ):
        paths = INCBIN_RE.findall(match.group(2))
        palette_paths[match.group(1)] = [
            path[:-7] + ".pal" if not (donor / path).exists() and path.endswith(".gbapal") else path
            for path in paths[:13]
        ]

    result: dict[str, dict[str, Any]] = {}
    for match in re.finditer(
        r"const struct Tileset (gTileset_[A-Za-z0-9_]+)\s*=\s*\{(.*?)\};",
        headers, re.DOTALL,
    ):
        struct_symbol, body = match.groups()
        refs = {
            "tiles": re.search(r"\.tiles\s*=\s*(gTilesetTiles_[A-Za-z0-9_]+)", body),
            "palettes": re.search(r"\.palettes\s*=\s*(gTilesetPalettes_[A-Za-z0-9_]+)", body),
            "metatiles": re.search(r"\.metatiles\s*=\s*(gMetatiles_[A-Za-z0-9_]+)", body),
            "attributes": re.search(r"\.metatileAttributes\s*=\s*(gMetatileAttributes_[A-Za-z0-9_]+)", body),
        }
        if not all(refs.values()):
            continue
        tile_key = refs["tiles"].group(1)
        palette_key = refs["palettes"].group(1)
        meta_key = refs["metatiles"].group(1)
        attr_key = refs["attributes"].group(1)
        if tile_key not in tile_paths or palette_key not in palette_paths or meta_key not in meta_paths or attr_key not in attr_paths:
            continue
        callback = re.search(r"\.callback\s*=\s*([A-Za-z0-9_]+)", body)
        result[struct_symbol] = {
            "tiles_source": tile_paths[tile_key],
            "palette_sources": palette_paths[palette_key],
            "metatiles_source": meta_paths[meta_key],
            "attributes_source": attr_paths[attr_key],
            "callback": callback.group(1) if callback else None,
        }
    return result


def parse_png(data: bytes) -> dict[str, int]:
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise AssetError("tiles.png has invalid PNG signature")
    offset = 8
    width = height = bit_depth = color_type = None
    palette_entries = None
    while offset + 12 <= len(data):
        length = struct.unpack(">I", data[offset:offset + 4])[0]
        chunk_type = data[offset + 4:offset + 8]
        body_start = offset + 8
        body_end = body_start + length
        if body_end + 4 > len(data):
            raise AssetError("truncated PNG chunk")
        body = data[body_start:body_end]
        if chunk_type == b"IHDR":
            if length != 13:
                raise AssetError("PNG IHDR has invalid length")
            width, height, bit_depth, color_type, _, _, _ = struct.unpack(">IIBBBBB", body)
        elif chunk_type == b"PLTE":
            if length % 3:
                raise AssetError("PNG palette has invalid length")
            palette_entries = length // 3
        offset = body_end + 4
        if chunk_type == b"IEND":
            break
    if width is None or height is None or palette_entries is None:
        raise AssetError("PNG lacks IHDR or PLTE")
    if width == 0 or height == 0 or width % 8 or height % 8:
        raise AssetError(f"PNG shape is not tile aligned: {width}x{height}")
    if bit_depth not in (4, 8) or color_type != 3 or not 1 <= palette_entries <= 16:
        raise AssetError("PNG is not an indexed 4bpp-budget source")
    four_bpp_bytes = width * height // 2
    if four_bpp_bytes > PNG_BUDGET_4BPP:
        raise AssetError("PNG exceeds the 4bpp tile budget")
    return {"width": width, "height": height, "bit_depth": bit_depth,
            "color_type": color_type, "palette_entries": palette_entries,
            "four_bpp_bytes": four_bpp_bytes}


def _behavior_map(donor: Path, host: Path) -> tuple[dict[int, dict[str, Any]], list[dict[str, Any]]]:
    donor_values = parse_donor_behaviors(donor)
    host_values = parse_host_behaviors(host)
    by_value: dict[int, list[str]] = {}
    for symbol, value in donor_values.items():
        by_value.setdefault(value, []).append(symbol)
    mapping: dict[int, dict[str, Any]] = {}
    for source_symbol, source_value in donor_values.items():
        if source_symbol == "MB_INVALID" and source_value == 0xFF and "MB_INVALID" in host_values:
            mapping[source_value] = {
                "source_symbol": source_symbol, "source_value": source_value,
                "target_symbol": "MB_INVALID", "target_value": host_values["MB_INVALID"],
                "rule": "preserve_invalid", "reason": "preserve invalid sentinel",
            }
        elif source_symbol in SPECIAL_BEHAVIORS:
            special = SPECIAL_BEHAVIORS[source_symbol]
            mapping[source_value] = {
                "source_symbol": source_symbol, "source_value": source_value,
                "target_symbol": special["target_symbol"], "target_value": special["target_value"],
                "rule": "reserved_special", "reason": special["reason"],
            }
        elif source_symbol in INERT_BEHAVIORS:
            mapping[source_value] = {
                "source_symbol": source_symbol, "source_value": source_value,
                "target_symbol": "MB_JOHTO_INERT", "target_value": INERT_VALUE,
                "rule": "explicit_inert", "reason": "reviewed donor value has no named consumer",
            }
        elif source_symbol in host_values:
            if source_value not in mapping:
                mapping[source_value] = {
                    "source_symbol": source_symbol, "source_value": source_value,
                    "target_symbol": source_symbol, "target_value": host_values[source_symbol],
                    "rule": "same_symbol_host_identity", "reason": "symbolic host behavior identity",
                }
    mismatches: list[dict[str, Any]] = []
    for value, entry in sorted(mapping.items()):
        # MB_INVALID has no host behavior name, so its explicit preservation
        # is part of the audited mismatch ledger even though its byte value is
        # unchanged.
        if (entry["source_value"] != entry["target_value"]
                or entry.get("rule") == "preserve_invalid"):
            mismatches.append(dict(entry))
    # Any donor behavior used by an asset must be resolvable.  The complete
    # used set is checked by validate_metatile_assets below; this prevents an
    # unknown semantic from silently becoming MB_NORMAL.
    if not mapping:
        raise AssetError("no behavior mappings were resolved")
    return mapping, mismatches


def convert_attributes(data: bytes, mapping: dict[int, dict[str, Any]] | None = None) -> bytes:
    """Convert donor u16 attrs into host u32 behavior/layer words."""
    if len(data) % ATTRIBUTE_BYTES:
        raise AssetError("metatile attributes must contain whole little-endian u16 values")
    mapping = mapping or {
        value: {"target_value": value} for value in range(256)
    }
    output = bytearray()
    for (value,) in struct.iter_unpack("<H", data):
        if value & 0x0F00:
            raise AssetError(f"reserved metatile attribute bits are set: {value:#06x}")
        layer = value >> 12
        if layer > 2:
            raise AssetError(f"unsupported metatile attribute layer: {value:#06x}")
        behavior = value & 0xFF
        resolved = mapping.get(behavior)
        if resolved is None or "target_value" not in resolved:
            raise AssetError(f"unknown metatile behavior: {behavior:#04x}")
        output.extend(struct.pack("<I", resolved["target_value"] | (layer << 29)))
    return bytes(output)


def convert_source_attributes(path: str, data: bytes,
                              mapping: dict[int, dict[str, Any]]) -> tuple[bytes, dict[str, Any] | None]:
    """Convert a selected donor file, applying only its pinned exceptions."""
    expected_sha = PINNED_ATTRIBUTE_EXCEPTION_SHAS.get(path)
    if expected_sha is None:
        return convert_attributes(data, mapping), None
    actual_sha = sha256(data)
    if actual_sha != expected_sha:
        raise AssetError(f"pinned attribute exception hash mismatch: {path}")
    if len(data) % ATTRIBUTE_BYTES:
        raise AssetError("metatile attributes must contain whole little-endian u16 values")
    output = bytearray()
    reserved_values: dict[str, int] = {}
    reserved_count = 0
    layer3_entries: list[dict[str, Any]] = []
    undefined_entries: list[dict[str, Any]] = []
    for index, (value,) in enumerate(struct.iter_unpack("<H", data)):
        original_value = value
        reserved = value & 0x0F00
        layer = value >> 12
        if layer > 3:
            raise AssetError(f"unsupported pinned metatile attribute layer: {path}[{index}]={value:#06x}")
        if reserved:
            reserved_count += 1
            key = f"{value:#06x}"
            reserved_values[key] = reserved_values.get(key, 0) + 1
            value &= ~0x0F00
            layer = value >> 12
        behavior = value & 0xFF
        resolved = mapping.get(behavior)
        if resolved is None and (
            PINNED_UNDEFINED_BEHAVIORS.get(path, {}).get(index) == original_value
            or (expected_sha is not None and behavior not in mapping)
        ):
            resolved = {"target_value": INERT_VALUE}
            undefined_entries.append({"index": index, "source_value": f"{original_value:#06x}",
                                      "target_behavior": "MB_JOHTO_INERT",
                                      "reason": "unused slot has no defined donor behavior"})
        if resolved is None or "target_value" not in resolved:
            raise AssetError(f"unknown metatile behavior: {behavior:#04x}")
        if layer == 3:
            layer3_entries.append({"index": index, "source_value": f"{(value | 0x3000):#06x}", "behavior": f"{behavior:#04x}"})
        output.extend(struct.pack("<I", resolved["target_value"] | (layer << 29)))
    if not reserved_count and not layer3_entries and not undefined_entries:
        return bytes(output), None
    return bytes(output), {
        "operation": "pinned_attribute_exceptions",
        "reserved_mask": "0x0f00",
        "reserved_bits_cleared": reserved_count,
        "reserved_source_values": reserved_values,
        "layer3_preserved": len(layer3_entries),
        "layer3_entries": layer3_entries,
        "undefined_behavior_repairs": undefined_entries,
        "reason": "source-backed unused bits are stripped; source layer 3 is preserved as 3<<29",
    }


def convert_source_map(path: str, data: bytes) -> tuple[bytes, dict[str, Any] | None]:
    """Convert a selected map source using only the reviewed source repair.

    The helper is intentionally public so the later scenery importer can use
    the same source/output hash and repair policy when it stages map assets.
    Every expected source word is checked before any output bytes are changed.
    """
    if path != ROUTE7_MAP_SOURCE_PATH:
        return data, None
    actual_sha = sha256(data)
    if actual_sha != ROUTE7_MAP_SOURCE_SHA256:
        raise AssetError(f"Route7 map repair hash mismatch: {path}")
    expected_size = ROUTE7_MAP_WIDTH * ROUTE7_MAP_HEIGHT * 2
    if len(data) != expected_size:
        raise AssetError(f"Route7 map repair geometry mismatch: {path}")
    output = bytearray(data)
    cells: list[dict[str, Any]] = []
    for index, x, y, expected_old, replacement in ROUTE7_MAP_REPAIRS:
        if index != y * ROUTE7_MAP_WIDTH + x:
            raise AssetError(f"Route7 map repair index drifted: {index}")
        offset = index * 2
        (actual_old,) = struct.unpack_from("<H", data, offset)
        if actual_old != expected_old:
            raise AssetError(
                f"Route7 map repair source word mismatch: index {index} "
                f"expected {expected_old:#06x}, got {actual_old:#06x}"
            )
        if (actual_old & ROUTE7_MAP_REPAIR_MASK) != ROUTE7_MAP_REPAIR_MASK:
            raise AssetError(f"Route7 map repair collision/elevation bit missing: index {index}")
        if (replacement & ROUTE7_MAP_REPAIR_MASK) != ROUTE7_MAP_REPAIR_MASK:
            raise AssetError(f"Route7 map repair replacement bit missing: index {index}")
        struct.pack_into("<H", output, offset, replacement)
        cells.append({
            "index": index,
            "x": x,
            "y": y,
            "source_word": f"{expected_old:#06x}",
            "output_word": f"{replacement:#06x}",
            "preserved_mask": f"{ROUTE7_MAP_REPAIR_MASK:#06x}",
        })
    converted = bytes(output)
    output_sha = sha256(converted)
    if output_sha != ROUTE7_MAP_OUTPUT_SHA256:
        raise AssetError(f"Route7 map repair output drifted: {output_sha}")
    if sum(left != right for left, right in zip(data, converted)) != len(ROUTE7_MAP_REPAIRS) * 2:
        raise AssetError("Route7 map repair changed bytes outside the twelve approved cells")
    return converted, {
        "operation": "route7_forest_boundary_repair",
        "source_sha256": actual_sha,
        "output_sha256": output_sha,
        "source_size": len(data),
        "output_size": len(converted),
        "width": ROUTE7_MAP_WIDTH,
        "height": ROUTE7_MAP_HEIGHT,
        "changed_cells": cells,
        "changed_cell_count": len(cells),
        "preserved_mask": f"{ROUTE7_MAP_REPAIR_MASK:#06x}",
        "reason": "replace twelve hash-bound Route7 forest-boundary words; preserve collision/elevation bits",
    }


def validate_layout_assets(
    donor: Path,
    layout: dict[str, Any],
    primary_count: int,
    secondary_count: int,
    unsupported_primary: set[int] | None = None,
    unsupported_secondary: set[int] | None = None,
) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    width = layout.get("width")
    height = layout.get("height")
    if not isinstance(width, int) or not isinstance(height, int) or width <= 0 or height <= 0:
        raise AssetError(f"invalid layout dimensions: {layout.get('id')}")
    block_path = safe_relative(layout.get("blockdata_filepath", ""))
    border_path = safe_relative(layout.get("border_filepath", ""))
    block_data = (donor / block_path).read_bytes()
    border_data = (donor / border_path).read_bytes()
    if len(block_data) != width * height * 2:
        raise AssetError(f"layout block geometry mismatch: {layout['id']}")
    if len(border_data) != 8:
        raise AssetError(f"layout border must be 8 bytes: {layout['id']}")
    unsupported_primary = unsupported_primary or set()
    unsupported_secondary = unsupported_secondary or set()

    def validate_words(data: bytes) -> None:
        for (word,) in struct.iter_unpack("<H", data):
            tile = word & 0x03FF
            if tile < PRIMARY_METATILE_COUNT:
                if tile >= primary_count:
                    raise AssetError(
                        f"layout primary tile index exceeds actual table bounds: {layout['id']}={tile}"
                    )
                if tile in unsupported_primary:
                    raise AssetError(
                        f"layout references unsupported primary slot: {layout['id']}={tile}"
                    )
            else:
                secondary = tile - PRIMARY_METATILE_COUNT
                if secondary >= secondary_count:
                    raise AssetError(
                        f"layout secondary tile index exceeds actual table bounds: {layout['id']}={secondary}"
                    )
                if secondary in unsupported_secondary:
                    raise AssetError(
                        f"layout references unsupported secondary slot: {layout['id']}={secondary}"
                    )

    # Both map.bin and border.bin are selected input channels and must obey
    # the same split-table bounds/no-use policy.  Route7's source-bound repair
    # is applied before validating tile IDs so the repaired bytes are what get
    # staged and checked against the generated manifest.
    converted_block, repair = convert_source_map(block_path, block_data)
    validate_words(converted_block)
    validate_words(border_data)
    assets = [
        {"path": block_path, "source_sha256": sha256(block_data), "output_sha256": sha256(converted_block), "source_size": len(block_data), "output_size": len(converted_block), "conversion": "identity" if repair is None else "route7-forest-boundary-repair"},
        {"path": border_path, "source_sha256": sha256(border_data), "output_sha256": sha256(border_data), "source_size": len(border_data), "output_size": len(border_data), "conversion": "identity"},
    ]
    if repair is not None:
        assets[0]["repair"] = repair
    return assets, {"width": width, "height": height}


def validate_tileset_assets(donor: Path, symbol: str, source: dict[str, Any], mapping: dict[int, dict[str, Any]]) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    required = [source.get("tiles_source"), source.get("metatiles_source"), source.get("attributes_source"), *(source.get("palette_sources") or [])]
    if not all(isinstance(path, str) for path in required) or len(required) != 16:
        raise AssetError(f"incomplete tileset source mapping: {symbol}")
    assets: list[dict[str, Any]] = []
    metatile_count = attribute_count = None
    converted_attributes = b""
    unsupported_indices: set[int] = set()
    for index, raw_path in enumerate(required):
        path = safe_relative(raw_path)
        file_data = (donor / path).read_bytes()
        if index == 0:
            png_info = parse_png(file_data)
            asset = {"path": path, "source_sha256": sha256(file_data), "output_sha256": sha256(file_data), "source_size": len(file_data), "output_size": len(file_data), "conversion": "identity", "png": png_info}
        elif index == 1:
            if len(file_data) % METATILE_BYTES:
                raise AssetError(f"metatiles length is not 16-byte aligned: {symbol}")
            metatile_count = len(file_data) // METATILE_BYTES
            asset = {"path": path, "source_sha256": sha256(file_data), "output_sha256": sha256(file_data), "source_size": len(file_data), "output_size": len(file_data), "conversion": "identity", "metatile_count": metatile_count}
        elif index == 2:
            if len(file_data) % ATTRIBUTE_BYTES:
                raise AssetError(f"attribute length is not u16 aligned: {symbol}")
            attribute_count = len(file_data) // ATTRIBUTE_BYTES
            unsupported_indices = set()
            for attribute_index, (value,) in enumerate(struct.iter_unpack("<H", file_data)):
                behavior_value = (value & ~0x0F00) & 0xFF
                if (
                    (value >> 12) == 3
                    or PINNED_UNDEFINED_BEHAVIORS.get(path, {}).get(attribute_index) == value
                    or (path in PINNED_ATTRIBUTE_EXCEPTION_SHAS and behavior_value not in mapping)
                ):
                    unsupported_indices.add(attribute_index)
            converted_attributes, normalization = convert_source_attributes(path, file_data, mapping)
            asset = {"path": path, "source_sha256": sha256(file_data), "output_sha256": sha256(converted_attributes), "source_size": len(file_data), "output_size": len(converted_attributes), "conversion": "u16-attribute-to-u32"}
            if normalization is not None:
                asset["normalization"] = normalization
        else:
            if path.endswith(".pal"):
                try:
                    palette_lines = file_data.decode("ascii").splitlines()
                    if palette_lines[:2] != ["JASC-PAL", "0100"]:
                        raise ValueError
                    declared_count = int(palette_lines[2])
                    colors = [tuple(int(part) for part in line.split()) for line in palette_lines[3:]]
                except (UnicodeDecodeError, ValueError):
                    raise AssetError(f"palette must be a valid 16-color JASC palette: {path}")
                if not 1 <= declared_count <= 16 or len(colors) != declared_count or any(len(color) != 3 or any(value < 0 or value > 255 for value in color) for color in colors):
                    raise AssetError(f"palette must contain 1-16 declared RGB colors: {path}")
            elif len(file_data) != 32:
                raise AssetError(f"palette must contain 16 little-endian colors: {path}")
            asset = {"path": path, "source_sha256": sha256(file_data), "output_sha256": sha256(file_data), "source_size": len(file_data), "output_size": len(file_data), "conversion": "identity"}
        assets.append(asset)
    if metatile_count != attribute_count:
        raise AssetError(f"metatile/attribute count mismatch: {symbol}")
    return assets, {
        "metatile_count": metatile_count,
        "attribute_count": attribute_count,
        "callback": source.get("callback"),
        "unsupported_indices": unsupported_indices,
    }


def closed_tileset_callback(
    animation: dict[str, Any],
    source_symbol: str,
    kind: str,
    donor_callback: str | None,
) -> str:
    """Resolve one donor callback through the explicit animation ledger."""
    suffix = source_symbol.removeprefix("gTileset_")
    registrations = animation.get("tileset_registration")
    if not isinstance(registrations, dict):
        raise AssetError("tileset animation registration ledger is missing")
    registration = registrations.get(kind)
    if not isinstance(registration, dict):
        raise AssetError(f"tileset animation registration ledger is missing {kind}")
    candidates = [suffix, suffix.replace("_", "")]
    if suffix.startswith("Johto_"):
        bare = suffix.removeprefix("Johto_")
        candidates.extend((bare, "Johto" + bare))
    key = next((candidate for candidate in candidates if candidate in registration), None)
    active = donor_callback not in (None, "NULL")
    if active and key is None:
        raise AssetError(f"active donor callback has no closed mapping: {source_symbol}")
    callback = registration[key] if key is not None else None
    if callback is not None and not re.fullmatch(r"InitTilesetAnim_[A-Za-z0-9_]+", callback):
        raise AssetError(f"invalid callback mapping: {source_symbol}")
    return callback or "NULL"


def build_manifest(donor: str | Path) -> dict[str, Any]:
    donor_path = Path(donor).resolve()
    revision, tree = git_pin(donor_path)
    if revision != DONOR_REVISION or tree != DONOR_TREE:
        raise AssetError(f"donor pin mismatch: expected {DONOR_REVISION}/{DONOR_TREE}, got {revision}/{tree}")
    assert_donor_clean(donor_path)
    region = load_json(REGION_MANIFEST)
    animation = load_json(ANIMATION_MANIFEST)
    if region.get("provenance", {}).get("donor_revision") != DONOR_REVISION or region.get("provenance", {}).get("donor_tree") != DONOR_TREE:
        raise AssetError("region manifest donor provenance drifted")
    selected_maps = region.get("maps")
    if not isinstance(selected_maps, list) or len(selected_maps) != 407:
        raise AssetError("region manifest must contain 407 selected maps")
    layouts_source = load_json(donor_path / "data/layouts/layouts.json").get("layouts")
    if not isinstance(layouts_source, list):
        raise AssetError("donor layouts table is missing")
    layouts_by_id = {entry.get("id"): entry for entry in layouts_source if isinstance(entry, dict)}
    tile_sources = parse_tileset_sources(donor_path)
    mapping, mismatches = _behavior_map(donor_path / "include/constants/metatile_behaviors.h", ROOT / "include/constants/metatile_behaviors.h")
    original_tile_symbols = {
        tile
        for selected in selected_maps[:239]
        for tile in (
            (selected.get("layout") or {}).get("primary_tileset"),
            (selected.get("layout") or {}).get("secondary_tileset"),
        )
    }
    if len(original_tile_symbols) != 66:
        raise AssetError(f"original tileset prefix drifted: {len(original_tile_symbols)}")

    def target_tileset_symbol(source_symbol: str) -> str:
        suffix = source_symbol.removeprefix("gTileset_")
        if source_symbol in original_tile_symbols:
            return f"gTileset_JohtoImported_{suffix}"
        return f"gTileset_KantoLaterImported_{suffix}"

    layouts: list[dict[str, Any]] = []
    seen_layouts: set[str] = set()
    tile_symbols: set[str] = set()
    for selected in selected_maps:
        layout_info = selected.get("layout") or {}
        layout_symbol = layout_info.get("symbol")
        if not isinstance(layout_symbol, str) or layout_symbol in seen_layouts:
            raise AssetError(f"duplicate or missing selected layout: {layout_symbol}")
        seen_layouts.add(layout_symbol)
        primary_symbol = layout_info.get("primary_tileset")
        secondary_symbol = layout_info.get("secondary_tileset")
        tile_symbols.update((primary_symbol, secondary_symbol))
        layout = layouts_by_id.get(layout_symbol)
        if layout is None:
            raise AssetError(f"selected layout is not in pinned donor table: {layout_symbol}")
        if layout.get("primary_tileset") != primary_symbol or layout.get("secondary_tileset") != secondary_symbol:
            raise AssetError(f"layout tileset identity drifted: {layout_symbol}")
        layout_record = {
            "ordinal": len(layouts), "symbol": layout_symbol, "source_map": selected.get("source_map"),
            "name": layout.get("name"), "width": layout.get("width"), "height": layout.get("height"),
            "primary_tileset": primary_symbol, "secondary_tileset": secondary_symbol,
        }
        # Keep the accepted original prefix byte-for-byte shaped as before;
        # later layouts carry their explicit target identity and source
        # namespace separately for the following runtime importer.
        if selected.get("era") == "KANTO_LATER":
            target_layout = (selected.get("identity_namespace") or {}).get("layout")
            if not isinstance(target_layout, str) or not target_layout:
                raise AssetError(f"later map is missing target layout identity: {selected.get('source_map')}")
            layout_record.update({
                "era": "KANTO_LATER",
                "identity_namespace": dict(selected.get("identity_namespace") or {}),
                "target_layout": target_layout,
                "target_primary_tileset": target_tileset_symbol(primary_symbol),
                "target_secondary_tileset": target_tileset_symbol(secondary_symbol),
            })
        layouts.append(layout_record)
    if len(layouts) != 407 or len(tile_symbols) != 97:
        raise AssetError(f"selected asset counts drifted: {len(layouts)} layouts / {len(tile_symbols)} tilesets")
    later_tile_symbols = tile_symbols - original_tile_symbols
    if len(later_tile_symbols) != 31:
        raise AssetError(f"later tileset tail drifted: {len(later_tile_symbols)}")

    tile_records: dict[str, dict[str, Any]] = {}
    unsupported_by_symbol: dict[str, set[int]] = {}
    for symbol in sorted(tile_symbols):
        source = tile_sources.get(symbol)
        if source is None:
            raise AssetError(f"selected tileset has no generated source mapping: {symbol}")
        source_assets, info = validate_tileset_assets(donor_path, symbol, source, mapping)
        primary = source_assets[0]["path"].startswith("data/tilesets/primary/")
        if primary:
            expected_count = GENERAL_METATILE_COUNT if symbol == GENERAL_TILESET_SYMBOL else PRIMARY_METATILE_COUNT
            if info["metatile_count"] != expected_count:
                raise AssetError(
                    f"primary tileset must contain {expected_count} metatiles: {symbol}"
                )
        callback = closed_tileset_callback(
            animation,
            symbol,
            "primary" if primary else "secondary",
            info["callback"],
        )
        recorded_callback = info["callback"] if symbol in original_tile_symbols else callback
        tile_record = {
            "symbol": symbol if symbol in original_tile_symbols else target_tileset_symbol(symbol),
            "kind": "primary" if primary else "secondary",
            "callback": recorded_callback,
            "runtime_ready": symbol not in original_tile_symbols and info["callback"] not in (None, "NULL"),
            "assets": source_assets,
        }
        if symbol not in original_tile_symbols:
            tile_record["source_symbol"] = symbol
        tile_records[symbol] = tile_record
        unsupported_by_symbol[symbol] = info["unsupported_indices"]
    for layout_record in layouts:
        primary_symbol = layout_record["primary_tileset"]
        secondary_symbol = layout_record["secondary_tileset"]
        primary = tile_records[primary_symbol]["assets"][1]["metatile_count"]
        secondary = tile_records[secondary_symbol]["assets"][1]["metatile_count"]
        layout_source = layouts_by_id[layout_record["symbol"]]
        layout_assets, geometry = validate_layout_assets(
            donor_path,
            layout_source,
            primary,
            secondary,
            unsupported_by_symbol[primary_symbol],
            unsupported_by_symbol[secondary_symbol],
        )
        layout_record["assets"] = layout_assets
        layout_record.update(geometry)
    special_mappings = []
    for source_symbol, special in SPECIAL_BEHAVIORS.items():
        item = dict(special)
        item["source_symbol"] = source_symbol
        item["rule"] = "reserved_special"
        special_mappings.append(item)
    for source_symbol in sorted(INERT_BEHAVIORS):
        item = {"source_symbol": source_symbol, "source_value": parse_donor_behaviors(donor_path / "include/constants/metatile_behaviors.h")[source_symbol], "target_symbol": "MB_JOHTO_INERT", "target_value": INERT_VALUE, "rule": "explicit_inert", "reason": "reviewed donor value has no named consumer"}
        special_mappings.append(item)
    special_mappings.sort(key=lambda item: (item["source_value"], item["source_symbol"]))
    if len(mismatches) != 22:
        raise AssetError(f"behavior mismatch resolution count drifted: {len(mismatches)}")
    tile_order = sorted(original_tile_symbols) + sorted(later_tile_symbols)
    if len(tile_order) != 97 or tile_order[:66] != sorted(original_tile_symbols):
        raise AssetError("tileset prefix/tail ordering drifted")
    general_layout_count = sum(
        layout["symbol"] in {
            "LAYOUT_FUCHSIA_CITY_SAFARI_ZONE_BEACH",
            "LAYOUT_FUCHSIA_CITY_SAFARI_ZONE_BRUSH",
            "LAYOUT_FUCHSIA_CITY_SAFARI_ZONE_MOUNTAIN",
        }
        for layout in layouts
    )
    if general_layout_count != 3:
        raise AssetError("General readiness layout set drifted")
    return {
        "schema_version": 1,
        "provenance": {
            "repository": DONOR_REPOSITORY, "donor_revision": revision, "donor_tree": tree,
            "region_manifest_path": "data/johto/region_manifest.json",
            "region_manifest_sha256": json_identity(region),
            "region_manifest_source_revision": REGION_MANIFEST_SOURCE_REVISION,
        },
        "selection": {"layout_count": len(layouts), "tileset_count": len(tile_records), "asset_count": sum(len(item["assets"]) for item in layouts) + sum(len(item["assets"]) for item in tile_records.values())},
        "conversion": {
            "source_attribute_format": "little-endian u16 behavior/layer",
            "output_attribute_format": "little-endian u32 behavior | layer << 29",
            "primary_metatile_boundary": PRIMARY_METATILE_COUNT,
            "general_primary_metatile_count": GENERAL_METATILE_COUNT,
            "secondary_metatile_id_base": PRIMARY_METATILE_COUNT,
            "map_tile_index_mask": "0x03ff",
            "mismatch_count": len(mismatches),
            "mismatch_resolutions": mismatches,
            "special_reservations": special_mappings,
            "preserve_invalid": {"symbol": "MB_INVALID", "value": 0xFF},
        },
        "runtime_ready": True,
        "pending_runtime": [],
        "runtime_readiness": {
            "ready": True,
            "general_primary_table_count": GENERAL_METATILE_COUNT,
            "secondary_id_base": PRIMARY_METATILE_COUNT,
            "pending_general_layouts": [],
            "global_writer": {
                "status": "closed",
                "source_evidence": "docs/orchestration/runs/johto-region-20260908/later-general-script-tile-audit.json",
                "reason": "runtime validates actual primary length and suppresses Fortree-only long-grass writes on imported General layouts",
            },
        },
        "layouts": layouts,
        "tilesets": [tile_records[symbol] for symbol in tile_order],
    }


def canonical_json(value: Any) -> str:
    return json.dumps(value, indent=2, ensure_ascii=True, sort_keys=False) + "\n"


def _ensure_output_root(path: Path, donor: Path) -> Path:
    root = path.resolve()
    if root == ROOT or ROOT in root.parents:
        raise AssetError("staging output must be outside the repository")
    donor = donor.resolve()
    if root == donor or root in donor.parents or donor in root.parents:
        raise AssetError("staging output must not overlap the donor checkout")
    # Worktrees use either a .git directory or a .git file.  Walk the existing
    # ancestor chain so a new child path inside any repository is rejected too.
    for ancestor in (root, *root.parents):
        if (ancestor / ".git").exists():
            raise AssetError(f"staging output must not overlap a repository: {ancestor}")
    return root


def stage_assets(donor: str | Path, output_dir: str | Path, manifest: dict[str, Any] | None = None) -> list[str]:
    donor_path = Path(donor).resolve()
    output_root = _ensure_output_root(Path(output_dir), donor_path)
    fresh_manifest = build_manifest(donor_path)
    if manifest is not None and canonical_json(manifest) != canonical_json(fresh_manifest):
        raise AssetError("supplied asset manifest does not match the pinned donor")
    manifest = fresh_manifest
    plan: list[tuple[Path, bytes]] = []
    for group in ("layouts", "tilesets"):
        for entry in manifest[group]:
            for asset in entry["assets"]:
                relative = safe_relative(asset["path"])
                source = donor_path / relative
                raw = source.read_bytes()
                if asset["conversion"] == "u16-attribute-to-u32":
                    mapping, _ = _behavior_map(donor_path / "include/constants/metatile_behaviors.h", ROOT / "include/constants/metatile_behaviors.h")
                    data, _ = convert_source_attributes(relative, raw, mapping)
                elif asset["conversion"] == "route7-forest-boundary-repair":
                    data, _ = convert_source_map(relative, raw)
                else:
                    data = raw
                if sha256(raw) != asset["source_sha256"] or sha256(data) != asset["output_sha256"]:
                    raise AssetError(f"source drift while staging: {relative}")
                destination = (output_root / relative).resolve()
                if output_root not in destination.parents:
                    raise AssetError(f"unsafe staging destination: {relative}")
                plan.append((destination, data))
    for destination, data in plan:
        if destination.exists() and destination.read_bytes() != data:
            raise AssetError(f"refusing to overwrite unrelated staging file: {destination}")
    for destination, data in plan:
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)
    return [str(path.relative_to(output_root).as_posix()) for path, _ in plan]


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--donor", required=True)
    parser.add_argument("--output-dir")
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--bootstrap", action="store_true")
    mode.add_argument("--write", action="store_true")
    mode.add_argument("--check", action="store_true")
    mode.add_argument("--stage", action="store_true")
    args = parser.parse_args(argv)
    try:
        manifest = build_manifest(args.donor)
        expected = canonical_json(manifest)
        if args.bootstrap:
            if OUTPUT_MANIFEST.exists():
                raise AssetError("bootstrap refuses to replace an existing asset manifest")
            OUTPUT_MANIFEST.parent.mkdir(parents=True, exist_ok=True)
            OUTPUT_MANIFEST.write_text(expected, encoding="utf-8", newline="\n")
        elif args.write:
            if OUTPUT_MANIFEST.exists():
                existing = OUTPUT_MANIFEST.read_bytes()
                expected_bytes = expected.encode("utf-8")
                normalized_existing = existing.replace(b"\r\n", b"\n")
                if normalized_existing == expected_bytes:
                    pass
                elif sha256(normalized_existing) not in ACCEPTED_PREDECESSOR_SHA256S:
                    raise AssetError("--write refuses to replace a manifest outside the accepted predecessor or intended output")
            else:
                raise AssetError("--write requires the accepted predecessor or an existing intended output")
            OUTPUT_MANIFEST.parent.mkdir(parents=True, exist_ok=True)
            if OUTPUT_MANIFEST.read_bytes().replace(b"\r\n", b"\n") != expected_bytes:
                OUTPUT_MANIFEST.write_text(expected, encoding="utf-8", newline="\n")
        elif args.check:
            if not OUTPUT_MANIFEST.exists() or OUTPUT_MANIFEST.read_text(encoding="utf-8") != expected:
                raise AssetError("checked-in asset manifest is stale")
        else:
            if not args.output_dir:
                raise AssetError("--stage requires --output-dir")
            staged = stage_assets(args.donor, args.output_dir, manifest)
            print(f"staged {len(staged)} assets")
        print(f"Johto assets layouts={manifest['selection']['layout_count']} tilesets={manifest['selection']['tileset_count']}")
        return 0
    except (OSError, AssetError) as error:
        print(f"Johto asset import failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
