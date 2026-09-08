"""Import the pinned Johto scenery corpus into the host's FRLG tables.

The importer deliberately treats the checked-in region and asset manifests as
the selection authority.  It performs all donor validation and plans every
write before changing the checkout; ``--check`` runs the same plan without
writing anything.
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
HOST_PREFIX_SHA256 = "1cce6340b79e53391d21e2845f4325ba444e66787dbc20898da93d3a18d721a4"
PREVIEW_SHA256 = "525dc826ebd0d57f4a1ee789a1ad717a6340eddbb750782e4e6e6efc025e685b"


class ImportError(RuntimeError):
    pass


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


def _safe(path: str) -> str:
    p = Path(path)
    if p.is_absolute() or ".." in p.parts or p.as_posix() != path:
        raise ImportError(f"unsafe donor path: {path}")
    return path


def _region_assets(donor: Path) -> tuple[dict[str, Any], dict[str, Any]]:
    sys.path.insert(0, str(Path(__file__).parent))
    import import_region_assets as assets

    fresh = assets.build_manifest(donor)
    checked = _load(ASSET_MANIFEST)
    if _canonical(fresh) != _canonical(checked):
        raise ImportError("checked-in asset manifest does not match pinned donor")
    region = _load(REGION_MANIFEST)
    if region.get("provenance", {}).get("donor_revision") != DONOR_REVISION or region.get("provenance", {}).get("donor_tree") != DONOR_TREE:
        raise ImportError("region manifest provenance drifted")
    if fresh["selection"] != {"layout_count": 239, "tileset_count": 66, "asset_count": 1534}:
        raise ImportError("selection counts are not 239/66/1534")
    return fresh, assets


def _source_assets(manifest: dict[str, Any], donor: Path, assets: Any) -> list[tuple[Path, bytes, str, str]]:
    mapping, _ = assets._behavior_map(
        donor / "include/constants/metatile_behaviors.h",
        ROOT / "include/constants/metatile_behaviors.h",
    )
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
                converted = raw
                if record["conversion"] not in ("identity", "u16-attribute-to-u32"):
                    raise ImportError(f"unsupported asset conversion: {relative}")
                if record["conversion"] == "u16-attribute-to-u32":
                    converted, _ = assets.convert_source_attributes(relative, raw, mapping)
                if _sha(raw) != record["source_sha256"] or _sha(converted) != record["output_sha256"]:
                    raise ImportError(f"pinned asset hash mismatch: {relative}")
                destination = (SCENERY_ROOT / relative).resolve()
                if SCENERY_ROOT.resolve() not in destination.parents:
                    raise ImportError(f"unsafe scenery destination: {relative}")
                plan.append((destination, converted, record["source_sha256"], record["output_sha256"]))
    if len(plan) != 1534:
        raise ImportError(f"planned {len(plan)} assets, expected 1534")
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


def _tileset_metadata(donor: Path, symbol: str, source: dict[str, Any], animation: dict[str, Any]) -> tuple[str | None, dict[str, Any]]:
    text = (donor / "src/data/tilesets/headers.h").read_text(encoding="utf-8")
    match = re.search(rf"const struct Tileset {re.escape(symbol)}\s*=\s*\{{(.*?)\}};", text, re.S)
    if not match:
        raise ImportError(f"missing donor Tileset definition: {symbol}")
    body = _active_body(match.group(1))
    allowed_fields = {"isCompressed", "isSecondary", "tiles", "palettes", "metatiles", "metatileAttributes", "callback", "swapPalettes", "lightPalettes", "customLightColor"}
    unknown = set(re.findall(r"\.(\w+)\s*=", body)) - allowed_fields
    if unknown:
        raise ImportError(f"unsupported tileset fields: {symbol}: {sorted(unknown)}")
    suffix = symbol.removeprefix("gTileset_")
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
    if donor_callback and registration_key not in registration:
        raise ImportError(f"active donor callback has no closed mapping: {symbol}")
    if registration_key in registration and registration[registration_key] is None:
        callback = None
    if callback is not None and not re.fullmatch(r"InitTilesetAnim_[A-Za-z0-9_]+", callback):
        raise ImportError(f"invalid callback mapping: {symbol}")
    metadata: dict[str, Any] = {
        "swapPalettes": 0,
        "lightPalettes": 0,
        "customLightColor": 0,
    }
    for key in metadata:
        found = re.search(rf"\.({key})\s*=\s*([^,]+)", body)
        if found:
            metadata[key] = _number(found.group(2))
    metadata["callback_source"] = donor_callback
    metadata["callback_rule"] = "closed_tileset_registration" if registration_key in registration else "donor_null"
    return callback, metadata


def _target_path(relative: str, extension: str | None = None) -> str:
    path = "data/johto/scenery/" + relative
    if extension and path.endswith(extension[0]):
        path = path[:-len(extension[0])] + extension[1]
    return path


def _tileset_header(manifest: dict[str, Any], donor: Path, animation: dict[str, Any]) -> tuple[str, list[dict[str, Any]]]:
    lines = [
        "/* Generated by tools/johto/import_scenery.py; do not edit. */",
        '#include "tileset_anims.h"',
        "",
    ]
    registrations: list[dict[str, Any]] = []
    for tile in manifest["tilesets"]:
        donor_symbol = tile["symbol"]
        suffix = donor_symbol.removeprefix("gTileset_")
        target = f"gTileset_JohtoImported_{suffix}"
        tiles = next(x["path"] for x in tile["assets"] if x["path"].endswith("/tiles.png"))
        metas = next(x["path"] for x in tile["assets"] if x["path"].endswith("/metatiles.bin"))
        attrs = next(x["path"] for x in tile["assets"] if x["path"].endswith("/metatile_attributes.bin"))
        palettes = [x["path"] for x in tile["assets"] if "/palettes/" in x["path"]]
        callback, metadata = _tileset_metadata(donor, donor_symbol, tile, animation)
        target_tiles = _target_path(tiles, (".png", ".4bpp.fastSmol"))
        target_metas = _target_path(metas)
        target_attrs = _target_path(attrs)
        target_pals = [_target_path(x, (".pal", ".gbapal")) for x in palettes]
        lines.append(f"const u32 gTilesetTiles_JohtoImported_{suffix}[] = INCBIN_U32(\"{target_tiles}\");")
        lines.append(f"const u16 ALIGNED(4) gTilesetPalettes_JohtoImported_{suffix}[][16] =")
        lines.append("{")
        lines.extend(f"    INCBIN_U16(\"{path}\")," for path in target_pals)
        lines.extend(["};", f"const u16 gMetatiles_JohtoImported_{suffix}[] = INCBIN_U16(\"{target_metas}\");", f"const u32 gMetatileAttributes_JohtoImported_{suffix}[] = INCBIN_U32(\"{target_attrs}\");", ""])
        lines.append(f"const struct Tileset {target} =")
        lines.append("{")
        lines.extend([
            "    .isCompressed = TRUE,",
            f"    .swapPalettes = {metadata['swapPalettes']},",
            f"    .isSecondary = {'TRUE' if tile['kind'] == 'secondary' else 'FALSE'},",
            f"    .lightPalettes = {metadata['lightPalettes']},",
            f"    .customLightColor = {metadata['customLightColor']},",
            f"    .tiles = gTilesetTiles_JohtoImported_{suffix},",
            f"    .palettes = gTilesetPalettes_JohtoImported_{suffix},",
            f"    .metatiles = gMetatiles_JohtoImported_{suffix},",
            f"    .metatileAttributes = (const u16 *)gMetatileAttributes_JohtoImported_{suffix},",
            f"    .callback = {callback or 'NULL'},",
            "};",
            "",
        ])
        registrations.append({
            "source_symbol": donor_symbol,
            "target_symbol": target,
            "kind": tile["kind"],
            "callback": callback,
            "metadata": metadata,
            "source_assets": tile["assets"],
        })
    return "\n".join(lines), registrations


def _layout_records(manifest: dict[str, Any], current: dict[str, Any]) -> tuple[dict[str, Any], list[dict[str, Any]], list[dict[str, Any]]]:
    if current.get("layouts_table_label") != "gMapLayouts" or not isinstance(current.get("layouts"), list):
        raise ImportError("host layout table has unsupported schema")
    if set(current) != {"layouts_table_label", "layouts"} or len(current["layouts"]) not in (786, 1024):
        raise ImportError("host layout table shape drifted")
    base = current["layouts"][:786].copy()
    if _identity(base[:785]) != HOST_PREFIX_SHA256:
        raise ImportError("pre-existing host layout identity/order/content drifted")
    if len(current["layouts"]) == 786 and _identity(base[785]) != PREVIEW_SHA256:
        raise ImportError("pre-import New Bark preview drifted")
    if len(base) != 786:
        raise ImportError(f"host layout baseline has {len(base)} records, expected 786")
    new_bark_indexes = [i for i, x in enumerate(base) if x.get("id") == "LAYOUT_NEW_BARK_TOWN" and x.get("name") == "NewBarkTown_Layout"]
    if new_bark_indexes != [785]:
        raise ImportError(f"New Bark identity/ordinal drifted: {new_bark_indexes}")
    donor_by_symbol = {e["symbol"]: e for e in manifest["layouts"]}
    imported: list[dict[str, Any]] = []
    registration: list[dict[str, Any]] = []
    for selected in manifest["layouts"]:
        donor_id = selected["symbol"]
        suffix = donor_id.removeprefix("LAYOUT_")
        target_id = donor_id if donor_id == "LAYOUT_NEW_BARK_TOWN" else f"LAYOUT_JOHTO_{suffix}"
        target_name = "NewBarkTown_Layout" if donor_id == "LAYOUT_NEW_BARK_TOWN" else f"Johto_{selected['name']}"
        record = {
            "id": target_id,
            "name": target_name,
            "width": selected["width"],
            "height": selected["height"],
            "primary_tileset": f"gTileset_JohtoImported_{selected['primary_tileset'].removeprefix('gTileset_')}",
            "secondary_tileset": f"gTileset_JohtoImported_{selected['secondary_tileset'].removeprefix('gTileset_')}",
            "border_filepath": f"data/johto/scenery/{selected['assets'][1]['path']}",
            "blockdata_filepath": f"data/johto/scenery/{selected['assets'][0]['path']}",
            "layout_version": "frlg",
            "border_width": 2,
            "border_height": 2,
        }
        registration.append({
            "source_layout_id": donor_id,
            "source_map": selected.get("source_map"),
            "source_name": selected["name"],
            "target_layout_id": target_id,
            "target_name": target_name,
            "ordinal": 785 if donor_id == "LAYOUT_NEW_BARK_TOWN" else 786 + len(imported),
            "map_layout_id": 786 if donor_id == "LAYOUT_NEW_BARK_TOWN" else 787 + len(imported),
            "primary_tileset": record["primary_tileset"],
            "secondary_tileset": record["secondary_tileset"],
            "width": record["width"],
            "height": record["height"],
            "border_width": 2,
            "border_height": 2,
            "assets": selected["assets"],
        })
        if donor_id == "LAYOUT_NEW_BARK_TOWN":
            base[785] = record
        else:
            imported.append(record)
    if len(imported) != 238:
        raise ImportError(f"planned {len(imported)} appended layouts, expected 238")
    if len(current["layouts"]) == 1024 and current["layouts"][785:] != (base + imported)[785:]:
        raise ImportError("imported layout identity/order/content drifted")
    return {"layouts_table_label": current["layouts_table_label"], "layouts": base + imported}, registration, base


def _registration(manifest: dict[str, Any], layout_registration: list[dict[str, Any]], tile_registration: list[dict[str, Any]]) -> dict[str, Any]:
    return {
        "schema_version": 1,
        "provenance": {
            "donor_revision": DONOR_REVISION,
            "donor_tree": DONOR_TREE,
            "region_manifest_sha256": _sha(REGION_MANIFEST.read_bytes()),
            "asset_manifest_sha256": _sha(ASSET_MANIFEST.read_bytes()),
            "selection": {"layout_count": 239, "tileset_count": 66, "asset_count": 1534},
        },
        "scope": {
            "purpose": "Johto scenery registration",
            "runtime": "layouts and tilesets only",
            "excluded": ["map headers", "groups", "scripts", "warps", "object wiring", "full-world readiness"],
        },
        "layouts": layout_registration,
        "tilesets": tile_registration,
    }


def _plan(donor: Path) -> tuple[dict[Path, bytes], dict[Path, str], dict[str, Any]]:
    manifest, assets = _region_assets(donor)
    source_plan = _source_assets(manifest, donor, assets)
    current = _load(LAYOUTS_JSON)
    layouts, layout_registration, _ = _layout_records(manifest, current)
    animation = _load(ANIMATION_MANIFEST)
    header, tile_registration = _tileset_header(manifest, donor, animation)
    registration = _registration(manifest, layout_registration, tile_registration)
    outputs: dict[Path, bytes] = {path: data for path, data, _, _ in source_plan}
    text_outputs = {
        HEADER: header.encode("utf-8"),
        LAYOUTS_JSON: _canonical(layouts).encode("utf-8"),
        REGISTRATION_JSON: _canonical(registration).encode("utf-8"),
    }
    outputs.update(text_outputs)
    existing_tilesets = TILESETS_C.read_text(encoding="utf-8")
    include_line = '#include "data/tilesets/johto_imported.h"'
    if include_line not in existing_tilesets:
        outputs[TILESETS_C] = (existing_tilesets.rstrip() + "\n" + include_line + "\n").encode("utf-8")
    elif existing_tilesets.count(include_line) != 1:
        raise ImportError("tilesets.c has duplicate Johto scenery include")
    return outputs, {path: _sha(data) for path, data in outputs.items()}, registration


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--donor-root", type=Path, required=True)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--write", action="store_true")
    mode.add_argument("--check", action="store_true")
    args = parser.parse_args(argv)
    try:
        outputs, hashes, _ = _plan(args.donor_root.resolve())
        for path, data in outputs.items():
            if args.check and not path.is_file():
                raise ImportError(f"missing generated output: {path}")
            existing_bytes = path.read_bytes() if path.exists() else None
            if existing_bytes is not None and path.suffix in (".h", ".c", ".json"):
                existing_bytes = existing_bytes.replace(b"\r\n", b"\n")
            if existing_bytes is not None and existing_bytes != data:
                if args.check:
                    raise ImportError(f"stale generated output: {path}")
                if path == LAYOUTS_JSON:
                    existing = _load(path)
                    if (len(existing.get("layouts", [])) == 786
                            and not any(str(item.get("id", "")).startswith("LAYOUT_JOHTO_")
                                        for item in existing.get("layouts", []))):
                        continue
                if path == TILESETS_C and '#include "data/tilesets/johto_imported.h"' not in path.read_text(encoding="utf-8"):
                    continue
                raise ImportError(f"refusing to overwrite drifted output: {path}")
        if args.write:
            for path, data in outputs.items():
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(data)
        print(f"{'checked' if args.check else 'wrote'} {len(outputs)} owned outputs")
        return 0
    except ImportError as exc:
        print(f"import_scenery: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
