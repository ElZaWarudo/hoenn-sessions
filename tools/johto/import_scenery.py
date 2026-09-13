"""Import the pinned Johto and later-Kanto static scenery corpus.

The importer is append-only at the host boundary. It validates the complete
donor manifest, keeps the existing 1024 layout rows, 239 registrations, 66
tilesets and their generated header bytes unchanged, then appends the accepted
later-Kanto records and assets. Every output is planned and checked before a
``--write`` mutates a byte; ``--check`` is side-effect free.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import struct
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
DONOR_REVISION = "751823abaf677020bcd72c45fe3e7cb2b8a576e4"
DONOR_TREE = "33661709e5368edc01c37ed9bb5e0a7a0cb192c8"
ASSET_MANIFEST = ROOT / "data/johto/asset_manifest.json"
REGION_MANIFEST = ROOT / "data/johto/region_manifest.json"
ANIMATION_MANIFEST = ROOT / "data/johto/tileset_animations.json"
LAYOUTS_JSON = ROOT / "data/layouts/layouts.json"
REGISTRATION_JSON = ROOT / "data/johto/scenery_registration.json"
HEADER = ROOT / "src/data/tilesets/johto_imported.h"
TILESETS_C = ROOT / "src/tilesets.c"
SCENERY_ROOT = ROOT / "data/johto/scenery"

LAYOUT_PREFIX_COUNT = 1024
LAYOUT_TAIL_COUNT = 168
OLD_LAYOUT_COUNT = 239
OLD_TILESET_COUNT = 66
TOTAL_LAYOUT_COUNT = 407
LAYOUT_TABLE_COUNT = LAYOUT_PREFIX_COUNT + LAYOUT_TAIL_COUNT
TOTAL_TILESET_COUNT = 97
TOTAL_ASSET_COUNT = 2366
OLD_ASSET_COUNT = 1534
NEW_ASSET_COUNT = 832
PRIMARY_TILE_BOUNDARY = 640
GENERAL_PRIMARY_COUNT = 512
GENERAL_SECONDARY_COUNT = 640

# These identities are computed over compact, sorted-key JSON. They bind the
# trusted predecessor and make arbitrary drift fail closed before --write.
HOST_LAYOUT_TABLE_SHA256 = "f7002263177bd513570004a9ef0ac8fb1da1395a270a9b01819f3cb52286ac5f"
HOST_LAYOUT_PREFIX_SHA256 = "1cce6340b79e53391d21e2845f4325ba444e66787dbc20898da93d3a18d721a4"
HOST_REGISTRATION_LAYOUT_PREFIX_SHA256 = "5d7f4645fb507a6874e70baf62069679331a21e490ff1fe4c779f0a7bc86ba02"
HOST_REGISTRATION_TILE_PREFIX_SHA256 = "c3afe942d5046cbdefb9e8b6bca45e3784339df9bcda453908546aed27b83a79"
HOST_REGISTRATION_SHA256 = "92e651adaa02e75b3983c8b50b3ae95ec3577143aeff3d23c73fe57d1efe3e1f"
PREDECESSOR_COMPLETE_REGISTRATION_SHA256 = "206aa035b3aacb69e9f339e460965e71cad48e9c33e2147c24c113a86e863352"
PREDECESSOR_RAW_PROVENANCE_REGISTRATION_SHA256 = "79062b8c72f0434caa7acd9d61cf3ea073597d2c67da55fdf3917389c7016b63"
HOST_HEADER_SHA256 = "1b6e60315a3d51c5e799a69ec5b1089ce4687f4532e2303c19b84826d1f20db7"
PREDECESSOR_COMPLETE_HEADER_SHA256 = "db5bb3d9377631035856c6d2aaffa7465951fb3673a2f962bf5f0974c9936bc5"

PENDING_GENERAL_LAYOUTS: tuple[str, ...] = ()
PREDECESSOR_GENERAL_LAYOUTS = (
    "LAYOUT_FUCHSIA_CITY_SAFARI_ZONE_BEACH",
    "LAYOUT_FUCHSIA_CITY_SAFARI_ZONE_BRUSH",
    "LAYOUT_FUCHSIA_CITY_SAFARI_ZONE_MOUNTAIN",
)
PREDECESSOR_ASSET_MANIFEST_SHA256 = "1cbb94d8aeeb0cd4d491d21c0f6fe724b3107d45657a4de21d5f6e7d398fcd2e"

REGISTRATION_KEYS = {
    "schema_version",
    "provenance",
    "scope",
    "layouts",
    "tilesets",
    "runtime_readiness",
}
PREDECESSOR_REGISTRATION_KEYS = REGISTRATION_KEYS - {"runtime_readiness"}
PROVENANCE_KEYS = {
    "donor_revision",
    "donor_tree",
    "region_manifest_sha256",
    "asset_manifest_sha256",
    "selection",
}
SCOPE = {
    "purpose": "Johto scenery registration",
    "runtime": "layouts and tilesets only",
    "excluded": [
        "map headers",
        "groups",
        "scripts",
        "warps",
        "object wiring",
        "full-world readiness",
    ],
}
PENDING_RUNTIME = [
    "source-faithful later tileset animation frames and callback registration",
    "map headers, groups, scripts, warps, transport and campaign registration",
]
RUNTIME_REGISTRATION_KEYS = {"ready", "static_scenery", "pending", "general_pending_layouts"}


class ImportError(RuntimeError):
    """A deterministic importer preflight or conversion failure."""


def _load(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise ImportError(f"cannot read {path}: {exc}") from exc


def _sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _canonical(value: Any) -> str:
    return json.dumps(value, indent=2, ensure_ascii=True, sort_keys=False) + "\n"


def _identity(value: Any) -> str:
    return _sha(json.dumps(value, sort_keys=True, separators=(",", ":")).encode())


def _json_identity(path: Path) -> str:
    return _identity(_load(path))


def _safe(path: str) -> str:
    p = Path(path)
    if p.is_absolute() or ".." in p.parts or p.as_posix() != path:
        raise ImportError(f"unsafe donor path: {path}")
    return path


def _region_assets(donor: Path) -> tuple[dict[str, Any], Any]:
    sys.path.insert(0, str(Path(__file__).parent))
    import import_region_assets as assets

    try:
        fresh = assets.build_manifest(donor)
    except Exception as exc:
        raise ImportError(f"donor manifest validation failed: {exc}") from exc
    checked = _load(ASSET_MANIFEST)
    if _canonical(fresh) != _canonical(checked):
        raise ImportError("checked-in asset manifest does not match pinned donor")
    region = _load(REGION_MANIFEST)
    provenance = region.get("provenance", {})
    if provenance.get("donor_revision") != DONOR_REVISION or provenance.get("donor_tree") != DONOR_TREE:
        raise ImportError("region manifest provenance drifted")
    if fresh.get("selection") != {
        "layout_count": TOTAL_LAYOUT_COUNT,
        "tileset_count": TOTAL_TILESET_COUNT,
        "asset_count": TOTAL_ASSET_COUNT,
    }:
        raise ImportError("selection counts are not 407/97/2366")
    return fresh, assets


def _tile_source(tile: dict[str, Any]) -> str:
    source = tile.get("source_symbol")
    symbol = source if source is not None else tile.get("symbol")
    if not isinstance(symbol, str) or not symbol.startswith("gTileset_"):
        raise ImportError(f"invalid tileset source identity: {symbol!r}")
    return symbol


def _tile_counts(manifest: dict[str, Any]) -> dict[str, int]:
    counts: dict[str, int] = {}
    for tile in manifest["tilesets"]:
        source = _tile_source(tile)
        try:
            metatiles = next(item["metatile_count"] for item in tile["assets"] if "metatile_count" in item)
        except (KeyError, StopIteration) as exc:
            raise ImportError(f"missing metatile count: {source}") from exc
        if not isinstance(metatiles, int) or metatiles <= 0:
            raise ImportError(f"invalid metatile count: {source}")
        counts[source] = metatiles
    if len(counts) != TOTAL_TILESET_COUNT:
        raise ImportError(f"tileset source identity collision: {len(counts)}")
    return counts


def _validate_table_words(data: bytes, primary_count: int, secondary_count: int, label: str) -> None:
    if len(data) % 2:
        raise ImportError(f"odd-sized map or border asset: {label}")
    for index, (word,) in enumerate(struct.iter_unpack("<H", data)):
        tile = word & 0x03FF
        if tile < PRIMARY_TILE_BOUNDARY:
            if tile >= primary_count:
                raise ImportError(f"primary table gap/out-of-range reference: {label}[{index}]={tile}")
        else:
            secondary = tile - PRIMARY_TILE_BOUNDARY
            if secondary >= secondary_count:
                raise ImportError(f"secondary table out-of-range reference: {label}[{index}]={secondary}")


def _manifest_asset_paths(manifest: dict[str, Any]) -> tuple[set[str], set[str]]:
    """Return the accepted predecessor and complete scenery path sets."""
    try:
        groups = (manifest["layouts"], manifest["tilesets"])
        all_paths = {
            _safe(record["path"])
            for group in groups
            for entry in group
            for record in entry["assets"]
        }
        old_paths = {
            _safe(record["path"])
            for group, limit in ((manifest["layouts"], OLD_LAYOUT_COUNT), (manifest["tilesets"], OLD_TILESET_COUNT))
            for entry in group[:limit]
            for record in entry["assets"]
        }
    except (KeyError, TypeError, ValueError) as exc:
        raise ImportError(f"asset manifest path schema is incomplete: {exc}") from exc
    if len(all_paths) != TOTAL_ASSET_COUNT:
        raise ImportError(f"asset manifest has {len(all_paths)} unique paths, expected {TOTAL_ASSET_COUNT}")
    if len(old_paths) != OLD_ASSET_COUNT:
        raise ImportError(f"asset manifest predecessor has {len(old_paths)} unique paths, expected {OLD_ASSET_COUNT}")
    if not old_paths <= all_paths:
        raise ImportError("asset manifest predecessor paths are not a subset of the complete corpus")
    return old_paths, all_paths


def _existing_scenery_paths() -> set[str]:
    if not SCENERY_ROOT.is_dir():
        return set()
    root = SCENERY_ROOT.resolve()
    paths: set[str] = set()
    for path in root.rglob("*"):
        if path.is_file():
            try:
                relative = path.resolve().relative_to(root).as_posix()
            except ValueError as exc:
                raise ImportError(f"scenery asset escapes output root: {path}") from exc
            if relative == ".gitattributes":
                continue
            paths.add(relative)
    return paths


def _source_assets(manifest: dict[str, Any], donor: Path, assets: Any) -> list[tuple[Path, bytes, str, str]]:
    try:
        mapping, _ = assets._behavior_map(
            donor / "include/constants/metatile_behaviors.h",
            ROOT / "include/constants/metatile_behaviors.h",
        )
    except Exception as exc:
        raise ImportError(f"metatile behavior mapping failed: {exc}") from exc
    counts = _tile_counts(manifest)
    layout_by_asset: dict[str, tuple[int, int]] = {}
    for entry in manifest["layouts"]:
        primary = entry["primary_tileset"]
        secondary = entry["secondary_tileset"]
        if primary not in counts or secondary not in counts:
            raise ImportError(f"layout references unknown tileset: {entry.get('symbol')}")
        for record in entry["assets"]:
            layout_by_asset[record["path"]] = (counts[primary], counts[secondary])

    plan: list[tuple[Path, bytes, str, str]] = []
    seen: set[str] = set()
    for group in ("layouts", "tilesets"):
        for entry in manifest[group]:
            for record in entry["assets"]:
                relative = _safe(record["path"])
                if relative in seen:
                    raise ImportError(f"duplicate selected asset: {relative}")
                seen.add(relative)
                source = donor / relative
                try:
                    raw = source.read_bytes()
                except OSError as exc:
                    raise ImportError(f"missing donor asset: {relative}") from exc
                if _sha(raw) != record["source_sha256"] or len(raw) != record["source_size"]:
                    raise ImportError(f"pinned source hash/size mismatch: {relative}")
                converted = raw
                repair = None
                try:
                    if relative in layout_by_asset and relative.endswith("/map.bin"):
                        converted, repair = assets.convert_source_map(relative, raw)
                        primary_count, secondary_count = layout_by_asset[relative]
                        _validate_table_words(converted, primary_count, secondary_count, relative)
                    elif relative in layout_by_asset and relative.endswith("/border.bin"):
                        primary_count, secondary_count = layout_by_asset[relative]
                        _validate_table_words(raw, primary_count, secondary_count, relative)
                    elif record["conversion"] == "u16-attribute-to-u32":
                        converted, _ = assets.convert_source_attributes(relative, raw, mapping)
                    elif record["conversion"] != "identity":
                        raise ImportError(f"unsupported asset conversion: {relative}")
                except ImportError:
                    raise
                except Exception as exc:
                    raise ImportError(f"asset conversion failed: {relative}: {exc}") from exc
                expected_conversion = "route7-forest-boundary-repair" if repair is not None else record["conversion"]
                if record["conversion"] != expected_conversion:
                    raise ImportError(f"conversion provenance mismatch: {relative}")
                if _sha(converted) != record["output_sha256"] or len(converted) != record["output_size"]:
                    raise ImportError(f"pinned output hash/size mismatch: {relative}")
                destination = (SCENERY_ROOT / relative).resolve()
                if SCENERY_ROOT.resolve() not in destination.parents:
                    raise ImportError(f"unsafe scenery destination: {relative}")
                plan.append((destination, converted, record["source_sha256"], record["output_sha256"]))
    if len(plan) != TOTAL_ASSET_COUNT:
        raise ImportError(f"planned {len(plan)} assets, expected {TOTAL_ASSET_COUNT}")
    predecessor_paths, complete_paths = _manifest_asset_paths(manifest)
    existing_paths = _existing_scenery_paths()
    if existing_paths not in (predecessor_paths, complete_paths):
        missing = sorted(complete_paths - existing_paths)
        unexpected = sorted(existing_paths - complete_paths)
        detail = []
        if missing:
            detail.append(f"missing={missing[0]}")
        if unexpected:
            detail.append(f"unexpected={unexpected[0]}")
        raise ImportError(
            "existing scenery asset path set is neither the exact 1534-file "
            "predecessor nor the exact 2366-file intended output"
            + (f" ({', '.join(detail)})" if detail else "")
        )
    return plan


def _active_body(body: str) -> str:
    body = re.sub(r"/\*.*?\*/", "", body, flags=re.S)
    return "\n".join(line.split("//", 1)[0] for line in body.splitlines())


def _number(expr: str, primary_boundary: int = 7) -> int:
    expr = expr.strip()
    macro = re.fullmatch(r"SWAP_PAL\((\d+)\)", expr)
    if macro:
        value = int(macro.group(1))
        return 1 << (value if value < primary_boundary else value - primary_boundary)
    if re.fullmatch(r"0[xX][0-9a-fA-F]+|\d+", expr):
        return int(expr, 0)
    raise ImportError(f"unsupported tileset metadata expression: {expr}")


def _tileset_metadata(donor: Path, source_symbol: str, source: dict[str, Any], animation: dict[str, Any], *, allow_unregistered_callback: bool = False) -> tuple[str | None, dict[str, Any]]:
    text = (donor / "src/data/tilesets/headers.h").read_text(encoding="utf-8")
    match = re.search(rf"const struct Tileset {re.escape(source_symbol)}\s*=\s*\{{(.*?)\}};", text, re.S)
    if not match:
        raise ImportError(f"missing donor Tileset definition: {source_symbol}")
    body = _active_body(match.group(1))
    allowed_fields = {"isCompressed", "isSecondary", "tiles", "palettes", "metatiles", "metatileAttributes", "callback", "swapPalettes", "lightPalettes", "customLightColor"}
    unknown = set(re.findall(r"\.(\w+)\s*=", body)) - allowed_fields
    if unknown:
        raise ImportError(f"unsupported tileset fields: {source_symbol}: {sorted(unknown)}")
    suffix = source_symbol.removeprefix("gTileset_")
    primary = animation.get("tileset_registration", {}).get("primary", {})
    secondary = animation.get("tileset_registration", {}).get("secondary", {})
    registration = primary if source.get("kind") == "primary" else secondary
    donor_callback = source.get("callback")
    if donor_callback == "NULL":
        donor_callback = None
    registration_candidates = [suffix, suffix.replace("_", "")]
    if suffix.startswith("Johto_"):
        registration_candidates.extend((suffix.removeprefix("Johto_"), "Johto" + suffix.removeprefix("Johto_")))
    registration_key = next((key for key in registration_candidates if key in registration), registration_candidates[0])
    callback = registration.get(registration_key, donor_callback)
    if donor_callback and registration_key not in registration and not allow_unregistered_callback:
        raise ImportError(f"active donor callback has no closed mapping: {source_symbol}")
    if registration_key in registration and registration[registration_key] is None:
        callback = None
    if callback is not None and not re.fullmatch(r"InitTilesetAnim_[A-Za-z0-9_]+", callback):
        raise ImportError(f"invalid callback mapping: {source_symbol}")
    metadata: dict[str, Any] = {"swapPalettes": 0, "lightPalettes": 0, "customLightColor": 0}
    for key in metadata:
        found = re.search(rf"\.({key})\s*=\s*([^,]+)", body)
        if found:
            metadata[key] = _number(found.group(2))
    metadata["callback_source"] = donor_callback
    metadata["callback_rule"] = "closed_tileset_registration" if registration_key in registration else "donor_null"
    return callback, metadata


def _target_path(relative: str, extension: tuple[str, str] | None = None) -> str:
    path = "data/johto/scenery/" + relative
    if extension and path.endswith(extension[0]):
        path = path[:-len(extension[0])] + extension[1]
    return path


def _render_tileset(tile: dict[str, Any], donor: Path, animation: dict[str, Any]) -> tuple[str, dict[str, Any]]:
    source_symbol = _tile_source(tile)
    target = tile.get("symbol")
    if not isinstance(target, str) or not target.startswith("gTileset_"):
        raise ImportError(f"invalid tileset target identity: {target!r}")
    suffix = target.removeprefix("gTileset_")
    tiles = next(x["path"] for x in tile["assets"] if x["path"].endswith("/tiles.png"))
    metas = next(x["path"] for x in tile["assets"] if x["path"].endswith("/metatiles.bin"))
    attrs = next(x["path"] for x in tile["assets"] if x["path"].endswith("/metatile_attributes.bin"))
    palettes = [x["path"] for x in tile["assets"] if "/palettes/" in x["path"]]
    callback, metadata = _tileset_metadata(donor, source_symbol, tile, animation, allow_unregistered_callback=True)
    metadata["callback_source"] = tile.get("callback")
    if source_symbol != "gTileset_General":
        # The remaining animation callbacks belong to a separate runtime package.
        metadata["callback_rule"] = "runtime_pending" if tile.get("callback") not in (None, "NULL") else metadata["callback_rule"]
        callback = None
    lines = [
        f"const u32 gTilesetTiles_{suffix}[] = INCBIN_U32(\"{_target_path(tiles, ('.png', '.4bpp.fastSmol'))}\");",
        f"const u16 ALIGNED(4) gTilesetPalettes_{suffix}[][16] =",
        "{",
        *(f"    INCBIN_U16(\"{_target_path(path, ('.pal', '.gbapal'))}\")," for path in palettes),
        "};",
        f"const u16 gMetatiles_{suffix}[] = INCBIN_U16(\"{_target_path(metas)}\");",
        f"const u32 gMetatileAttributes_{suffix}[] = INCBIN_U32(\"{_target_path(attrs)}\");",
        "",
        f"const struct Tileset {target} =",
        "{",
        "    .isCompressed = TRUE,",
        f"    .swapPalettes = {metadata['swapPalettes']},",
        f"    .isSecondary = {'TRUE' if tile['kind'] == 'secondary' else 'FALSE'},",
        f"    .lightPalettes = {metadata['lightPalettes']},",
        f"    .customLightColor = {metadata['customLightColor']},",
        f"    .tiles = gTilesetTiles_{suffix},",
        f"    .palettes = gTilesetPalettes_{suffix},",
        f"    .metatiles = gMetatiles_{suffix},",
        f"    .metatileAttributes = (const u16 *)gMetatileAttributes_{suffix},",
        f"    .callback = {callback or 'NULL'},",
        "};",
        "",
    ]
    registration = {
        "source_symbol": source_symbol,
        "target_symbol": target,
        "kind": tile["kind"],
        "callback": callback,
        "metadata": metadata,
        "source_assets": tile["assets"],
    }
    return "\n".join(lines), registration


def _tileset_header(manifest: dict[str, Any], donor: Path, animation: dict[str, Any]) -> tuple[str, list[dict[str, Any]]]:
    try:
        current = HEADER.read_text(encoding="utf-8").replace("\r\n", "\n")
    except OSError as exc:
        raise ImportError(f"cannot read generated tileset header: {exc}") from exc
    marker = "const u32 gTilesetTiles_KantoLaterImported_"
    marker_index = -1
    if _sha(current.encode()) == HOST_HEADER_SHA256:
        base = current
    else:
        marker_index = current.find(marker)
        if marker_index < 0:
            raise ImportError("generated tileset header predecessor drifted")
        candidate = current[:marker_index].rstrip() + "\n"
        if _sha(candidate.encode()) != HOST_HEADER_SHA256:
            raise ImportError("generated Johto tileset header prefix drifted")
        base = candidate
    old_symbols = re.findall(r"^const struct Tileset (gTileset_\w+) =", base, flags=re.M)
    expected_old = [
        f"gTileset_JohtoImported_{tile['symbol'].removeprefix('gTileset_')}"
        for tile in manifest["tilesets"][:OLD_TILESET_COUNT]
    ]
    if old_symbols != expected_old:
        raise ImportError("generated Johto tileset header identity/order drifted")
    later = manifest["tilesets"][OLD_TILESET_COUNT:]
    if len(later) != TOTAL_TILESET_COUNT - OLD_TILESET_COUNT:
        raise ImportError("later tileset header tail count drifted")
    blocks: list[str] = []
    registrations: list[dict[str, Any]] = []
    for tile in later:
        block, registration = _render_tileset(tile, donor, animation)
        blocks.append(block)
        registrations.append(registration)
    output = base.rstrip() + "\n\n" + "\n".join(blocks)
    if marker_index >= 0 and current != output and _sha(current.encode()) != PREDECESSOR_COMPLETE_HEADER_SHA256:
        raise ImportError("existing complete tileset header drifted")
    return output, registrations


def _later_layout_record(selected: dict[str, Any], index: int) -> tuple[dict[str, Any], dict[str, Any]]:
    identity = selected.get("identity_namespace")
    if not isinstance(identity, dict) or not all(isinstance(identity.get(key), str) and identity[key] for key in ("map", "layout", "script")):
        raise ImportError(f"later layout identity is incomplete: {selected.get('symbol')}")
    target_id = selected.get("target_layout")
    if target_id != identity["layout"]:
        raise ImportError(f"later layout target mismatch: {selected.get('symbol')}")
    primary = selected.get("target_primary_tileset")
    secondary = selected.get("target_secondary_tileset")
    if not isinstance(primary, str) or not isinstance(secondary, str):
        raise ImportError(f"later layout tileset target missing: {selected.get('symbol')}")
    name = f"KantoLater_{selected['name']}"
    record = {
        "id": target_id,
        "name": name,
        "width": selected["width"],
        "height": selected["height"],
        "primary_tileset": primary,
        "secondary_tileset": secondary,
        "border_filepath": f"data/johto/scenery/{selected['assets'][1]['path']}",
        "blockdata_filepath": f"data/johto/scenery/{selected['assets'][0]['path']}",
        "layout_version": "frlg",
        "border_width": 2,
        "border_height": 2,
    }
    registration = {
        "source_layout_id": selected["symbol"],
        "source_map": selected.get("source_map"),
        "source_name": selected["name"],
        "target_layout_id": target_id,
        "target_name": name,
        "ordinal": LAYOUT_PREFIX_COUNT + index,
        "map_layout_id": LAYOUT_PREFIX_COUNT + index + 1,
        "primary_tileset": primary,
        "secondary_tileset": secondary,
        "width": selected["width"],
        "height": selected["height"],
        "border_width": 2,
        "border_height": 2,
        "assets": selected["assets"],
        "era": "KANTO_LATER",
        "identity_namespace": dict(identity),
    }
    return record, registration


def _layout_records(manifest: dict[str, Any], current: dict[str, Any]) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    if set(current) != {"layouts_table_label", "layouts"} or current.get("layouts_table_label") != "gMapLayouts" or not isinstance(current["layouts"], list):
        raise ImportError("host layout table has unsupported schema")
    rows = current["layouts"]
    if len(rows) not in (LAYOUT_PREFIX_COUNT, LAYOUT_TABLE_COUNT):
        raise ImportError(f"host layout table shape drifted: {len(rows)}")
    if _identity(rows[:LAYOUT_PREFIX_COUNT]) != HOST_LAYOUT_TABLE_SHA256:
        raise ImportError("pre-existing host layout table prefix drifted")
    if _identity(rows[:785]) != HOST_LAYOUT_PREFIX_SHA256:
        raise ImportError("pre-existing host layout prefix drifted")
    later_records: list[dict[str, Any]] = []
    later_registration: list[dict[str, Any]] = []
    for index, selected in enumerate(manifest["layouts"][OLD_LAYOUT_COUNT:]):
        record, registration = _later_layout_record(selected, index)
        later_records.append(record)
        later_registration.append(registration)
    if len(later_records) != LAYOUT_TAIL_COUNT:
        raise ImportError(f"planned {len(later_records)} appended layouts, expected {LAYOUT_TAIL_COUNT}")
    output_rows = rows[:LAYOUT_PREFIX_COUNT] + later_records
    if len(rows) == LAYOUT_TABLE_COUNT and rows[LAYOUT_PREFIX_COUNT:] != later_records:
        raise ImportError("existing later layout tail drifted")
    return {"layouts_table_label": "gMapLayouts", "layouts": output_rows}, later_registration


def _registration(manifest: dict[str, Any], later_layout_registration: list[dict[str, Any]], later_tile_registration: list[dict[str, Any]]) -> dict[str, Any]:
    current = _load(REGISTRATION_JSON)
    if not isinstance(current, dict):
        raise ImportError("host scenery registration has unsupported schema")
    keys = set(current)
    predecessor = (
        keys == PREDECESSOR_REGISTRATION_KEYS
        and type(current.get("schema_version")) is int
        and current["schema_version"] == 1
    )
    accepted_general_predecessor = False
    if predecessor:
        if _identity(current) != HOST_REGISTRATION_SHA256:
            raise ImportError("existing scenery registration predecessor drifted")
    else:
        if keys != REGISTRATION_KEYS:
            raise ImportError("host scenery registration has unsupported schema")
        if type(current.get("schema_version")) is not int or current["schema_version"] != 1:
            raise ImportError("host scenery registration schema version drifted")
        provenance = current.get("provenance")
        if not isinstance(provenance, dict) or set(provenance) != PROVENANCE_KEYS:
            raise ImportError("host scenery registration provenance schema drifted")
        if provenance.get("donor_revision") != DONOR_REVISION or provenance.get("donor_tree") != DONOR_TREE:
            raise ImportError("host scenery registration donor provenance drifted")
        runtime_readiness = current.get("runtime_readiness")
        accepted_general_predecessor = (
            isinstance(runtime_readiness, dict)
            and tuple(runtime_readiness.get("general_pending_layouts", ())) == PREDECESSOR_GENERAL_LAYOUTS
            and provenance.get("asset_manifest_sha256") == PREDECESSOR_ASSET_MANIFEST_SHA256
            and _identity(current) == PREDECESSOR_COMPLETE_REGISTRATION_SHA256
        )
        accepted_raw_provenance_predecessor = (
            _identity(current) == PREDECESSOR_RAW_PROVENANCE_REGISTRATION_SHA256
        )
        accepted_manifest_predecessor = accepted_general_predecessor or accepted_raw_provenance_predecessor
        if not accepted_manifest_predecessor and (
            provenance.get("region_manifest_sha256") != _json_identity(REGION_MANIFEST)
            or provenance.get("asset_manifest_sha256") != _json_identity(ASSET_MANIFEST)
        ):
            raise ImportError("host scenery registration manifest provenance drifted")
        if provenance.get("selection") != {"layout_count": TOTAL_LAYOUT_COUNT, "tileset_count": TOTAL_TILESET_COUNT, "asset_count": TOTAL_ASSET_COUNT}:
            raise ImportError("host scenery registration selection drifted")
        if current.get("scope") != SCOPE:
            raise ImportError("host scenery registration scope drifted")
        if not isinstance(runtime_readiness, dict) or set(runtime_readiness) != RUNTIME_REGISTRATION_KEYS:
            raise ImportError("host scenery runtime-readiness schema drifted")
        if runtime_readiness.get("ready") is not False or runtime_readiness.get("static_scenery") is not True:
            raise ImportError("host scenery runtime-readiness gate drifted")
        if not accepted_general_predecessor and (runtime_readiness.get("pending") != PENDING_RUNTIME or runtime_readiness.get("general_pending_layouts") != list(PENDING_GENERAL_LAYOUTS)):
            raise ImportError("host scenery runtime-readiness dependencies drifted")
    layouts = current.get("layouts")
    tilesets = current.get("tilesets")
    if not isinstance(layouts, list) or not isinstance(tilesets, list):
        raise ImportError("host scenery registration tables are missing")
    expected_counts = (
        (OLD_LAYOUT_COUNT, OLD_TILESET_COUNT)
        if predecessor
        else (TOTAL_LAYOUT_COUNT, TOTAL_TILESET_COUNT)
    )
    if (len(layouts), len(tilesets)) != expected_counts:
        raise ImportError("host scenery registration counts drifted")
    if _identity(layouts[:OLD_LAYOUT_COUNT]) != HOST_REGISTRATION_LAYOUT_PREFIX_SHA256:
        raise ImportError("existing scenery layout registration prefix drifted")
    if _identity(tilesets[:OLD_TILESET_COUNT]) != HOST_REGISTRATION_TILE_PREFIX_SHA256:
        raise ImportError("existing scenery tileset registration prefix drifted")
    if len(layouts) == TOTAL_LAYOUT_COUNT and len(tilesets) == TOTAL_TILESET_COUNT:
        if not accepted_general_predecessor and (layouts[OLD_LAYOUT_COUNT:] != later_layout_registration or tilesets[OLD_TILESET_COUNT:] != later_tile_registration):
            raise ImportError("existing later scenery registration tail drifted")
    output = dict(current)
    output["provenance"] = dict(current["provenance"])
    output["provenance"]["donor_revision"] = DONOR_REVISION
    output["provenance"]["donor_tree"] = DONOR_TREE
    output["provenance"]["region_manifest_sha256"] = _json_identity(REGION_MANIFEST)
    output["provenance"]["asset_manifest_sha256"] = _json_identity(ASSET_MANIFEST)
    output["provenance"]["selection"] = {"layout_count": TOTAL_LAYOUT_COUNT, "tileset_count": TOTAL_TILESET_COUNT, "asset_count": TOTAL_ASSET_COUNT}
    output["layouts"] = layouts[:OLD_LAYOUT_COUNT] + later_layout_registration
    output["tilesets"] = tilesets[:OLD_TILESET_COUNT] + later_tile_registration
    output["runtime_readiness"] = {
        "ready": False,
        "static_scenery": True,
        "pending": list(PENDING_RUNTIME),
        "general_pending_layouts": list(PENDING_GENERAL_LAYOUTS),
    }
    if len(layouts) == TOTAL_LAYOUT_COUNT and len(tilesets) == TOTAL_TILESET_COUNT and current != output and not accepted_manifest_predecessor:
        raise ImportError("existing complete scenery registration drifted")
    return output


def _plan(donor: Path) -> tuple[dict[Path, bytes], dict[Path, str], dict[str, Any]]:
    manifest, assets = _region_assets(donor)
    source_plan = _source_assets(manifest, donor, assets)
    current_layouts = _load(LAYOUTS_JSON)
    layouts, later_layout_registration = _layout_records(manifest, current_layouts)
    animation = _load(ANIMATION_MANIFEST)
    header, later_tile_registration = _tileset_header(manifest, donor, animation)
    registration = _registration(manifest, later_layout_registration, later_tile_registration)
    outputs: dict[Path, bytes] = {path: data for path, data, _, _ in source_plan}
    outputs.update({
        HEADER: header.encode("utf-8"),
        LAYOUTS_JSON: _canonical(layouts).encode("utf-8"),
        REGISTRATION_JSON: _canonical(registration).encode("utf-8"),
    })
    try:
        tilesets_c = TILESETS_C.read_text(encoding="utf-8")
    except OSError as exc:
        raise ImportError(f"cannot read tileset table dependency: {exc}") from exc
    include_line = '#include "data/tilesets/johto_imported.h"'
    if tilesets_c.count(include_line) != 1:
        raise ImportError("tilesets.c must contain exactly one Johto scenery include")
    return outputs, {path: _sha(data) for path, data in outputs.items()}, registration


def _trusted_predecessor(path: Path, existing: bytes) -> bool:
    """Return whether a generated text output is the exact accepted input."""
    normalized = existing.replace(b"\r\n", b"\n")
    if path == HEADER:
        return _sha(normalized) in (HOST_HEADER_SHA256, PREDECESSOR_COMPLETE_HEADER_SHA256)
    if path == LAYOUTS_JSON:
        try:
            document = json.loads(normalized.decode("utf-8"))
            return _identity(document.get("layouts")) == HOST_LAYOUT_TABLE_SHA256
        except (UnicodeDecodeError, json.JSONDecodeError):
            return False
    if path == REGISTRATION_JSON:
        try:
            return _identity(json.loads(normalized.decode("utf-8"))) in (
                HOST_REGISTRATION_SHA256,
                PREDECESSOR_COMPLETE_REGISTRATION_SHA256,
                PREDECESSOR_RAW_PROVENANCE_REGISTRATION_SHA256,
            )
        except (UnicodeDecodeError, json.JSONDecodeError):
            return False
    return False


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--donor-root", type=Path, required=True)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--write", action="store_true")
    mode.add_argument("--check", action="store_true")
    args = parser.parse_args(argv)
    try:
        outputs, _, _ = _plan(args.donor_root.resolve())
        mismatches: list[tuple[Path, bytes]] = []
        for path, data in outputs.items():
            existing = path.read_bytes() if path.exists() else None
            if existing is not None and path.suffix in (".h", ".c", ".json"):
                existing = existing.replace(b"\r\n", b"\n")
            if existing is None:
                if args.check:
                    raise ImportError(f"missing generated output: {path}")
                continue
            if existing != data:
                mismatches.append((path, existing))
        if mismatches:
            if args.check or any(not _trusted_predecessor(path, existing) for path, existing in mismatches):
                raise ImportError(f"refusing to overwrite drifted output: {mismatches[0][0]}")
        if args.write:
            for path, data in outputs.items():
                if not path.exists() or any(path == mismatch_path for mismatch_path, _ in mismatches):
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(data)
        print(f"{'checked' if args.check else 'wrote'} {len(outputs)} owned outputs")
        return 0
    except ImportError as exc:
        print(f"import_scenery: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
