"""Prepare authenticated, isolated Cormoria tileset definitions outside the live tree."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

from tools.cormoria import import_world, stage_general_foundation

ROOT = Path(__file__).resolve().parents[2]
SOURCE_FILES = ("src/data/tilesets/headers.h", "src/data/tilesets/graphics.h",
                "src/data/tilesets/metatiles.h")
DECLARATION = re.compile(r"(?m)^const (?:u16|u32|struct Tileset) (g[A-Za-z0-9_]+)(?:\[.*?\])?\s*=\s*", re.DOTALL)
ASSET = re.compile(r'INCBIN_U(?:16|32)\("([^"]+)"\)')
IDENTIFIER = re.compile(r"\b[A-Za-z_][A-Za-z_0-9]*\b")
TILE_RULE = re.compile(
    r"(?m)^\$\(TILESETGFXDIR\)/([a-z0-9_/]+/tiles)\.4bpp: %\.4bpp: %\.png\n"
    r"\s*\$\(GFX\) \$< \$@ -num_tiles (\d+) -Wnum_tiles\s*$")


class TilesetRegistrationError(ValueError):
    """Pinned tileset identity or staged bytes cannot be authenticated."""


def _sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _source(stage: Path, relative: str, indexed: dict[str, dict]) -> bytes:
    safe = import_world.safe_relative(relative)
    path = stage / "source" / safe
    if not path.is_file() or not path.resolve().is_relative_to(stage.resolve()):
        raise TilesetRegistrationError(f"missing or escaping staged source: {relative}")
    row = indexed.get("source/" + relative)
    if row is None:
        raise TilesetRegistrationError(f"source omitted from staged hashes: {relative}")
    data = path.read_bytes()
    if len(data) != row["bytes"] or _sha(data) != row["sha256"]:
        raise TilesetRegistrationError(f"staged source hash mismatch: {relative}")
    return data


def _declarations(source: str, wanted: set[str] | None = None) -> dict[str, str]:
    found: dict[str, str] = {}
    for match in DECLARATION.finditer(source):
        if wanted is not None and match[1] not in wanted:
            continue
        end = source.find(";", match.end())
        if end < 0:
            raise TilesetRegistrationError(f"unterminated declaration: {match[1]}")
        block = source[match.start():end + 1]
        if match[1] in found:
            raise TilesetRegistrationError(f"duplicate declaration: {match[1]}")
        found[match[1]] = block
    return found


def _asset_record(requested: str, assets: dict[str, dict], files: dict[str, dict],
                  source_files: dict[str, dict], stage: Path) -> dict[str, str]:
    row = assets.get(requested)
    if row is None:
        raise TilesetRegistrationError(f"asset recipe missing: {requested}")
    source = row["source"]
    data = _source(stage, source, files)
    pinned = source_files.get(source)
    if pinned is None or len(data) != pinned["bytes"] or _sha(data) != pinned["sha256"]:
        raise TilesetRegistrationError(f"donor asset hash mismatch: {source}")
    target = "data/tilesets/cormoria/" + requested.removeprefix("data/tilesets/")
    if target == requested or not target.startswith("data/tilesets/cormoria/"):
        raise TilesetRegistrationError(f"asset not isolated: {requested}")
    return {"requested": requested, "origin": "stage", "source": "source/" + source,
            "source_sha256": _sha(data), "conversion": row["conversion"], "target": target}


def _general_inputs(supplement: Path, root: Path) -> dict[str, bytes]:
    supplement = supplement.resolve(strict=True)
    committed = json.loads((root / "tools/cormoria/general_foundation_manifest.json").read_text(encoding="utf-8"))
    staged = json.loads((supplement / "foundation_manifest.json").read_text(encoding="utf-8"))
    if (staged != committed or staged.get("provenance") != {
            "repository": stage_general_foundation.REPOSITORY,
            "revision": stage_general_foundation.REVISION,
            "tree": stage_general_foundation.TREE} or
            [row["path"] for row in staged["files"]] != list(stage_general_foundation.PATHS)):
        raise TilesetRegistrationError("General supplement provenance or inventory drifted")
    result: dict[str, bytes] = {}
    for row in staged["files"]:
        relative = import_world.safe_relative(row["path"])
        path = (supplement / "source" / relative).resolve()
        if not path.is_relative_to(supplement) or not path.is_file():
            raise TilesetRegistrationError(f"missing or escaping General source: {relative}")
        data = path.read_bytes()
        if len(data) != row["bytes"] or _sha(data) != row["sha256"]:
            raise TilesetRegistrationError(f"General source hash mismatch: {relative}")
        result[row["path"]] = data
    return result


def _general_asset_record(requested: str, general: dict[str, bytes]) -> dict[str, str]:
    if requested.endswith(".gbapal"):
        source = requested.removesuffix(".gbapal") + ".pal"
        conversion = "pal->gbapal"
    elif requested.endswith(".4bpp.lz"):
        source = requested.removesuffix(".4bpp.lz") + ".png"
        conversion = "png->4bpp->lz"
    else:
        raise TilesetRegistrationError(f"unsupported General asset: {requested}")
    if source not in general:
        raise TilesetRegistrationError(f"General asset omitted from supplement: {source}")
    return {"requested": requested, "origin": "general-foundation", "source": "source/" + source,
            "source_sha256": _sha(general[source]), "conversion": conversion,
            "target": "data/tilesets/cormoria/" + requested.removeprefix("data/tilesets/")}


def _tile_rules(path: Path, sources: dict) -> dict[str, int]:
    record = next((row for row in sources["files"] if row["path"] == "graphics_file_rules.mk"), None)
    data = path.read_bytes()
    if record is None or len(data) != record["bytes"] or _sha(data) != record["sha256"]:
        raise TilesetRegistrationError("donor graphics rules hash mismatch")
    text = data.decode("utf-8").replace("\r\n", "\n")
    rules: dict[str, int] = {}
    for match in TILE_RULE.finditer(text):
        requested = "data/tilesets/" + match[1] + ".4bpp.lz"
        if requested in rules:
            raise TilesetRegistrationError(f"duplicate donor tile rule: {requested}")
        rules[requested] = int(match[2])
    if not rules:
        raise TilesetRegistrationError("donor tile rules were not recognized")
    return rules


def _function_body(source: str, name: str) -> str:
    match = re.search(rf"\bvoid\s+{re.escape(name)}\s*\([^)]*\)\s*\{{", source)
    if match is None:
        raise TilesetRegistrationError(f"missing animation function: {name}")
    start = match.end()
    depth = 1
    end = start
    while depth and end < len(source):
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    if depth:
        raise TilesetRegistrationError(f"unterminated animation function: {name}")
    return re.sub(r"\s+", "", source[start:end - 1])


def _prove_snow_rebinding(donor_anims: Path, source_files: dict[str, dict], host_anims: str,
                          stage: Path, staged_files: dict[str, dict], root: Path) -> str:
    relative = "src/tileset_anims.c"
    record = source_files.get(relative)
    data = donor_anims.read_bytes()
    if record is None or len(data) != record["bytes"] or _sha(data) != record["sha256"]:
        raise TilesetRegistrationError("donor animation source hash mismatch")
    donor = data.decode("utf-8")
    if (_function_body(donor, "TilesetAnim_Snow") != _function_body(donor, "TilesetAnim_General")
            or _function_body(donor, "TilesetAnim_General") != _function_body(host_anims, "TilesetAnim_General")
            or _function_body(donor, "InitTilesetAnim_Snow").replace("TilesetAnim_Snow", "TilesetAnim_General")
               != _function_body(donor, "InitTilesetAnim_General")
            or _function_body(donor, "InitTilesetAnim_General") != _function_body(host_anims, "InitTilesetAnim_General")):
        raise TilesetRegistrationError("Snow and General animation behavior is not source-equivalent")
    animation_paths = sorted(path for path in source_files if
                             path.startswith("data/tilesets/primary/general/anim/") and path.endswith(".png"))
    if len(animation_paths) != 26:
        raise TilesetRegistrationError("General animation frame inventory drifted")
    for relative in animation_paths:
        staged = _source(stage, relative, staged_files)
        if (_sha(staged) != source_files[relative]["sha256"] or
                not (root / relative).is_file() or (root / relative).read_bytes() != staged):
            raise TilesetRegistrationError(f"General animation frame differs from host: {relative}")
    return record["sha256"]


def render(stage: Path, general_foundation: Path, donor_rules: Path, donor_anims: Path,
           root: Path = ROOT) -> dict[str, bytes]:
    region, _, sources = import_world.load_manifests(root)
    stage_info = json.loads((stage / "staging_manifest.json").read_text(encoding="utf-8"))
    if stage_info.get("manifest_sha256") != import_world.PINNED_SHA256 or stage_info.get("provenance") != region["provenance"]:
        raise TilesetRegistrationError("stage provenance or manifest binding drifted")
    files = {row["path"]: row for row in stage_info["files"]}
    if len(files) != len(stage_info["files"]):
        raise TilesetRegistrationError("duplicate staged file record")
    source_files = {row["path"]: row for row in sources["files"]}
    tile_rules = _tile_rules(donor_rules, sources)
    host_anims = (root / "src/tileset_anims.c").read_text(encoding="utf-8")
    snow_source_sha = _prove_snow_rebinding(donor_anims, source_files, host_anims,
                                            stage, files, root)
    if ("src/graphics.c" in source_files
            or "data/tilesets/primary/general/tiles.png" in source_files
            or any(path.startswith("data/tilesets/primary/general/palettes/")
                   for path in source_files)):
        raise TilesetRegistrationError("General base graphics entered pinned source manifest; resolve binding explicitly")
    general = _general_inputs(general_foundation, root)
    texts = {}
    for relative in SOURCE_FILES:
        data = _source(stage, relative, files)
        pinned = source_files.get(relative)
        if pinned is None or len(data) != pinned["bytes"] or _sha(data) != pinned["sha256"]:
            raise TilesetRegistrationError(f"donor source hash mismatch: {relative}")
        texts[relative] = _declarations(data.decode("utf-8"))

    records = {row["source_symbol"]: row for row in region["tilesets"]}
    if len(records) != len(region["tilesets"]):
        raise TilesetRegistrationError("duplicate tileset manifest symbol")
    assets = {row["requested"]: row for row in sources["assets"]}
    if len(assets) != len(sources["assets"]):
        raise TilesetRegistrationError("duplicate asset recipe")

    headers = texts[SOURCE_FILES[0]]
    graphics = texts[SOURCE_FILES[1]]
    metatiles = texts[SOURCE_FILES[2]]
    general_graphics = _declarations(general["src/graphics.c"].decode("utf-8"),
                                     {"gTilesetPalettes_General", "gTilesetTiles_General"})
    if any(("gTilesetTiles_General" in graphics, "gTilesetPalettes_General" in graphics)):
        raise TilesetRegistrationError("General base graphics unexpectedly entered staged tileset declarations")

    selected = sorted(region["tilesets"], key=lambda row: row["source_symbol"])
    if len(selected) != 59:
        raise TilesetRegistrationError("pinned tileset count drifted")
    requested: set[str] = set()
    blocks: list[str] = []
    emitted: set[str] = set()
    callbacks: dict[str, list[str]] = {}
    for row in selected:
        symbol = row["source_symbol"]
        if (records.get(symbol) is not row or row.get("target_symbol") != "Cormoria_" + symbol
                or row.get("record") != SOURCE_FILES[0] or not symbol.startswith("gTileset_")
                or len(row.get("dependencies", [])) not in (2, 4)):
            raise TilesetRegistrationError(f"tileset manifest binding drifted: {symbol}")
        block = headers.get(symbol)
        if block is None:
            raise TilesetRegistrationError(f"missing donor header definition: {symbol}")
        identifiers = set(IDENTIFIER.findall(block))
        dependencies = list(row["dependencies"])
        if not set(dependencies).issubset(identifiers):
            raise TilesetRegistrationError(f"unsupported donor header: {symbol}")
        for field in ("tiles", "palettes", "metatiles", "metatileAttributes"):
            match = re.search(rf"\.{field}\s*=\s*(g[A-Za-z0-9_]+)", block)
            if match is None:
                raise TilesetRegistrationError(f"missing {field} binding: {symbol}")
            dependency = match[1]
            if dependency not in dependencies and not (symbol == "gTileset_General" and
                    dependency in {"gTilesetTiles_General", "gTilesetPalettes_General"}):
                raise TilesetRegistrationError(f"unlisted donor dependency: {symbol} -> {dependency}")
            if dependency in emitted:
                continue
            if dependency.startswith(("gMetatiles_", "gMetatileAttributes_")):
                definition = metatiles.get(dependency)
            elif dependency in general_graphics:
                definition = general_graphics[dependency]
            else:
                definition = graphics.get(dependency)
            if definition is None:
                raise TilesetRegistrationError(f"missing donor definition: {dependency}")
            blocks.append(definition)
            requested.update(ASSET.findall(definition))
            emitted.add(dependency)
        callback_match = re.search(r"\.callback\s*=\s*([A-Za-z_][A-Za-z_0-9]*)", block)
        if callback_match is None or [callback_match[1]] != row.get("callbacks"):
            raise TilesetRegistrationError(f"callback binding drifted: {symbol}")
        callbacks[symbol] = row["callbacks"]
        blocks.append(block)
        emitted.add(symbol)

    recipes = [(_general_asset_record(path, general) if path.startswith("data/tilesets/primary/general/")
                and (path.endswith(".gbapal") or path.endswith(".4bpp.lz")) else
                _asset_record(path, assets, files, source_files, stage)) for path in sorted(requested)]
    for recipe in recipes:
        if recipe["requested"] in tile_rules:
            if recipe["conversion"] != "png->4bpp->lz":
                raise TilesetRegistrationError(f"donor tile rule conversion drifted: {recipe['requested']}")
            recipe["num_tiles"] = tile_rules[recipe["requested"]]
    targets = {row["target"] for row in recipes}
    if len(targets) != len(recipes):
        raise TilesetRegistrationError("isolated asset target collision")
    bindings = {symbol: "Cormoria_" + symbol for symbol in emitted}
    target_by_requested = {row["requested"]: row["target"] for row in recipes}
    def convert(block: str) -> str:
        block = IDENTIFIER.sub(lambda match: bindings.get(match[0], match[0]), block)
        return ASSET.sub(lambda match: f'INCBIN_U{"16" if "U16" in match[0] else "32"}("{target_by_requested[match[1]]}")', block)
    missing = sorted({callback for uses in callbacks.values() for callback in uses
                      if callback not in {"NULL", "InitTilesetAnim_Snow"} and not re.search(
                          rf"\bvoid\s+{re.escape(callback)}\s*\([^)]*\)\s*\{{", host_anims)})
    # A newly ported host callback may have a definition before its public
    # header is updated. Prototypes make that link dependency explicit.
    declarations = "".join(f"extern void {callback}(void);\n" for callback in
                           sorted({callback for uses in callbacks.values() for callback in uses
                                   if callback not in {"NULL", "InitTilesetAnim_Snow"}}))
    code = ("/* Authenticated Cormoria tileset preview; requires listed asset conversions. */\n"
            "#include \"global.h\"\n#include \"global.fieldmap.h\"\n"
            + declarations + "\n"
            + "\n\n".join(convert(block).replace(".callback = InitTilesetAnim_Snow,",
                                               ".callback = InitTilesetAnim_General,")
                           for block in blocks) + "\n").encode("utf-8")
    plan = {"schema_version": 1, "world_id": "cormoria", "provenance": region["provenance"],
            "graphics_rules_sha256": _sha(donor_rules.read_bytes()),
            "callback_rebindings": [{"tileset": "Cormoria_gTileset_Snow",
                                     "donor_callback": "InitTilesetAnim_Snow",
                                     "host_callback": "InitTilesetAnim_General",
                                     "donor_animation_source_sha256": snow_source_sha,
                                     "evidence": "initializers and animation timer bodies are source-equivalent; 26 General animation PNGs match host bytes"}],
            "general_foundation": {"revision": stage_general_foundation.REVISION,
                                   "manifest_sha256": _sha((root / "tools/cormoria/general_foundation_manifest.json")
                                                              .read_bytes().replace(b"\r\n", b"\n"))},
            "registered_tilesets": [row["target_symbol"] for row in selected],
            "unresolved": [{"callback": callback, "tilesets": [row["target_symbol"] for row in selected
                                                            if callback in row["callbacks"]],
                            "reason": "callback has no host definition; preserve donor behavior before linking"}
                           for callback in missing],
            "recipes": recipes}
    return {"tilesets.c": code,
            "asset_plan.json": (json.dumps(plan, indent=2, ensure_ascii=False) + "\n").encode("utf-8")}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stage", type=Path, required=True)
    parser.add_argument("--general-foundation", type=Path, required=True)
    parser.add_argument("--donor-rules", type=Path, required=True)
    parser.add_argument("--donor-anims", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        stage = args.stage.resolve(strict=True)
        output = args.output.resolve()
        root = ROOT.resolve()
        foundation = args.general_foundation.resolve(strict=True)
        donor_rules = args.donor_rules.resolve(strict=True)
        donor_anims = args.donor_anims.resolve(strict=True)
        if (output.exists() or output == root or output.is_relative_to(root)
                or output.is_relative_to(stage) or output.is_relative_to(foundation)):
            raise TilesetRegistrationError("output must be fresh and outside source and stage trees")
        rendered = render(stage, foundation, donor_rules, donor_anims)
        output.mkdir(parents=True)
        for name, data in rendered.items():
            (output / name).write_bytes(data)
    except (OSError, ValueError, KeyError) as exc:
        print(f"Cormoria tileset preview: {exc}", file=sys.stderr)
        return 1
    print(f"Prepared {len(rendered)} Cormoria tileset preview files at {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
