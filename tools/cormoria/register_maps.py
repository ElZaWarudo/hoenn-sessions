"""Reproduce the pinned Cormoria map/layout allocation from staged donor data.

The generator also verifies an already installed Cormoria prefix without
assuming that no later ROM region has been appended to the registries.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import tempfile
from pathlib import Path
from typing import Any

from tools.cormoria import berry_plots, import_world
from tools import shared_item_registry

ROOT = Path(__file__).resolve().parents[2]
HOST_GROUPS = Path("data/maps/map_groups.json")
HOST_LAYOUTS = Path("data/layouts/layouts.json")
HOST_GROUP_COUNT = 79
HOST_MAP_COUNT = 1344
HOST_LAYOUT_COUNT = 1192
HOST_GROUPS_SHA256 = "6dc9292668aebd7d17170be30dac364610be8852709ea38b1590f3bc26e0e023"
HOST_LAYOUT_IDS_SHA256 = "72850322fa85f29d5266f02184db5731ca1561dd6dcd832ef60b87bf2f9528eb"
REGION_MAP_SOURCE = "src/data/region_map/region_map_layout.h"
REGION_MAP_PNG = "graphics/pokenav/region_map/map.png"
REGION_MAP_TILEMAP = "graphics/pokenav/region_map/map.bin"
REGION_MAP_LAYOUT = "src/data/region_map/region_map_layout_cormoria.h"
REGION_MAP_ASSET_DIR = "graphics/pokenav/region_map"
SECTION_TOKEN = re.compile(r"MAPSEC_[A-Z0-9_]+")


class MapRegistrationError(ValueError):
    """The staged campaign or host registry differs from its pinned allocation."""


def _load(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise MapRegistrationError(f"invalid JSON object: {path}")
    return value


def _sha(value: Any) -> str:
    return hashlib.sha256(json.dumps(value, separators=(",", ":"), ensure_ascii=False).encode()).hexdigest()


def _host_registries(root: Path) -> tuple[dict[str, Any], dict[str, Any]]:
    groups = _load(root / HOST_GROUPS)
    layouts = _load(root / HOST_LAYOUTS)
    order = groups.get("group_order")
    rows = layouts.get("layouts")
    if (not isinstance(order, list) or len(order) < HOST_GROUP_COUNT
            or len(set(order)) != len(order)
            or any(not isinstance(groups.get(name), list) for name in order)
            or sum(len(groups[name]) for name in order[:HOST_GROUP_COUNT]) != HOST_MAP_COUNT
            or _sha([(name, groups[name]) for name in order[:HOST_GROUP_COUNT]]) != HOST_GROUPS_SHA256):
        raise MapRegistrationError("host map-group identity or order drifted")
    if (not isinstance(rows, list) or len(rows) < HOST_LAYOUT_COUNT
            or _sha([row.get("id") for row in rows[:HOST_LAYOUT_COUNT]]) != HOST_LAYOUT_IDS_SHA256):
        raise MapRegistrationError("host layout identity or order drifted")
    base_groups = dict(groups)
    for name in order[HOST_GROUP_COUNT:]:
        base_groups.pop(name)
    base_groups["group_order"] = order[:HOST_GROUP_COUNT]
    base_layouts = dict(layouts)
    base_layouts["layouts"] = rows[:HOST_LAYOUT_COUNT]
    return base_groups, base_layouts


def _staged_bytes(stage: Path, relative: str) -> bytes:
    path = stage / import_world.safe_relative(relative)
    if not path.is_file() or not path.resolve().is_relative_to(stage.resolve()):
        raise MapRegistrationError(f"missing or escaping staged file: {relative}")
    return path.read_bytes()


def _ordered_maps(region: dict[str, Any]) -> list[dict[str, Any]]:
    by_source = {item["source_name"]: item for item in region["maps"]}
    if len(by_source) != 165:
        raise MapRegistrationError("expected 165 unique donor maps")
    ordered = []
    for group_offset, group in enumerate(region["groups"]):
        if group["host_group"] != HOST_GROUP_COUNT + group_offset:
            raise MapRegistrationError("Cormoria map-group allocation drifted")
        for index, source_name in enumerate(group["maps"]):
            item = by_source.get(source_name)
            if item is None or item["group"] != group["host_group"] or item["index"] != index:
                raise MapRegistrationError(f"Cormoria map position drifted: {source_name}")
            ordered.append(item)
    if len(ordered) != len(by_source) or len(set(item["source_name"] for item in ordered)) != len(ordered):
        raise MapRegistrationError("Cormoria map-group membership drifted")
    return ordered


def _adapt_object_ranges(mapped: dict[str, Any]) -> None:
    # The donor's Pelluca boat specifies 20, but the shared object template
    # packs each movement radius into four bits. Its donor ROM uses 20 & 15 = 4.
    if mapped["name"] == "Cormoria_PellucaCity":
        boat = mapped["object_events"][3]
        if (boat.get("graphics_id") != "OBJ_EVENT_GFX_MR_BRINEYS_BOAT"
                or boat.get("x") != 28 or boat.get("y") != 43
                or boat.get("movement_range_x") != 0
                or boat.get("movement_range_y") != 20):
            raise MapRegistrationError("Pelluca boat range binding drifted")
        boat["movement_range_y"] = 4
    for event in mapped["object_events"]:
        for axis in ("movement_range_x", "movement_range_y"):
            radius = event.get(axis)
            if type(radius) is not int or not 0 <= radius <= 15:
                raise MapRegistrationError(f"object movement radius cannot be packed: {mapped['name']}")


def _adapt_berry_plots(mapped: dict[str, Any], bindings: dict[str, str]) -> list[str]:
    found = []
    for event in mapped["object_events"]:
        if event["movement_type"] != "MOVEMENT_TYPE_BERRY_TREE_GROWTH":
            continue
        source = event["trainer_sight_or_berry_tree_id"]
        if source not in bindings:
            raise MapRegistrationError(f"unmapped Cormoria berry plot: {mapped['name']}: {source}")
        event["trainer_sight_or_berry_tree_id"] = bindings[source]
        found.append(source)
    return found


def build_registration(stage: Path, root: Path = ROOT, *,
                       previous_item_registry: bytes | None = None,
                       trusted_previous_sha256: str | None = None) -> dict[str, bytes]:
    region, symbols, sources = import_world.load_manifests(root)
    shared_item_registry.validate(root, previous_bytes=previous_item_registry,
                                  trusted_previous_sha256=trusted_previous_sha256)
    groups, layouts = _host_registries(root)
    _, source_index = import_world.selected_paths(region, symbols, sources)
    lookup = import_world.identities(region, symbols)
    berry_bindings = berry_plots.bindings(root)
    berry_events = []
    ordered = _ordered_maps(region)
    stage = stage.resolve(strict=True)
    staged_index = _load(stage / "staging_manifest.json")
    if (staged_index.get("provenance") != region["provenance"]
            or staged_index.get("manifest_sha256") != import_world.PINNED_SHA256
            or staged_index.get("map_count") != len(ordered)
            or staged_index.get("layout_count") != len(region["layouts"])):
        raise MapRegistrationError("staged campaign identity differs from pinned manifests")

    output: dict[str, bytes] = {}
    new_groups = dict(groups)
    new_groups["group_order"] = list(groups["group_order"])
    host_names = {name for group in groups["group_order"] for name in groups[group]}
    for group in region["groups"]:
        target = group["target"]
        if target in new_groups:
            raise MapRegistrationError(f"map group collides with host: {target}")
        names = [item["target_name"] for item in ordered if item["group"] == group["host_group"]]
        if len(names) != len(group["maps"]) or any(name in host_names for name in names):
            raise MapRegistrationError(f"invalid map names in {target}")
        new_groups["group_order"].append(target)
        new_groups[target] = names

    for item in ordered:
        source = f"data/maps/{item['source_name']}/map.json"
        original = json.loads(import_world.source_bytes(stage / "source", source, source_index))
        if original.get("id") != item["source_id"] or original.get("layout") != item["layout"]:
            raise MapRegistrationError(f"source map identity drifted: {source}")
        mapped = import_world.rewrite(original, lookup)
        mapped.update(name=item["target_name"], id=item["target_id"], rom_world="cormoria")
        relative = f"maps/{item['target_name']}/map.json"
        if _staged_bytes(stage, relative) != import_world.canonical(mapped):
            raise MapRegistrationError(f"staged map differs from authenticated source: {relative}")
        _adapt_object_ranges(mapped)
        berry_events.extend(_adapt_berry_plots(mapped, berry_bindings))
        # The map header's engine-region byte selects the ROM's geography.
        # Section IDs are display coordinates and must not stand in for it.
        mapped["region"] = "REGION_CORMORIA"
        rendered = import_world.canonical(mapped)
        destination = f"data/maps/{item['target_name']}/map.json"
        if destination in output:
            raise MapRegistrationError(f"duplicate map path: {destination}")
        if (root / destination).exists() and _load(root / destination) != mapped:
            raise MapRegistrationError(f"installed map differs from pinned donor: {destination}")
        output[destination] = rendered

    if len(berry_events) != 28 or len(set(berry_events)) != 25 or set(berry_events) != set(berry_bindings) - set(berry_plots.UNUSED_SOURCE_NAMES):
        raise MapRegistrationError("Cormoria berry event set differs from pinned donor maps")

    new_layouts = dict(layouts)
    new_layouts["layouts"] = list(layouts["layouts"])
    layout_ids = {entry["id"] for entry in layouts["layouts"]}
    for item in region["layouts"]:
        staged = dict(item)
        staged.update(id=item["target_id"], name=item["target_name"],
                      primary_tileset=lookup.get(item["primary_tileset"], item["primary_tileset"]),
                      secondary_tileset=lookup.get(item["secondary_tileset"], item["secondary_tileset"]),
                      rom_world="cormoria")
        relative = f"layouts/{item['target_name']}/layout.json"
        if _staged_bytes(stage, relative) != import_world.canonical(staged):
            raise MapRegistrationError(f"staged layout differs from pinned manifest: {relative}")
        if item["target_id"] in layout_ids:
            raise MapRegistrationError(f"layout ID collides with host: {item['target_id']}")
        layout_ids.add(item["target_id"])
        staged.pop("target_id")
        staged.pop("target_name")
        for field in ("border_filepath", "blockdata_filepath"):
            source = item[field]
            binary = import_world.source_bytes(stage / "source", source, source_index)
            destination = "data/cormoria/layouts/" + source.removeprefix("data/layouts/")
            if destination in output:
                raise MapRegistrationError(f"duplicate layout binary: {destination}")
            staged[field] = destination
            output[destination] = binary
        new_layouts["layouts"].append(staged)

    installed_groups = _load(root / HOST_GROUPS)
    installed_layouts = _load(root / HOST_LAYOUTS)
    generated_order = new_groups["group_order"]
    generated_layouts = new_layouts["layouts"]
    if len(installed_groups["group_order"]) > HOST_GROUP_COUNT:
        if (installed_groups["group_order"][:len(generated_order)] != generated_order
                or any(installed_groups[name] != new_groups[name]
                       for name in generated_order[HOST_GROUP_COUNT:])):
            raise MapRegistrationError("installed Cormoria map allocation drifted")
        for name in installed_groups["group_order"][len(generated_order):]:
            new_groups["group_order"].append(name)
            new_groups[name] = installed_groups[name]
    if len(installed_layouts["layouts"]) > HOST_LAYOUT_COUNT:
        if installed_layouts["layouts"][:len(generated_layouts)] != generated_layouts:
            raise MapRegistrationError("installed Cormoria layout allocation drifted")
        later = installed_layouts["layouts"][len(generated_layouts):]
        if any(entry["id"] in layout_ids for entry in later):
            raise MapRegistrationError("later layout reuses a Cormoria ID")
        new_layouts["layouts"].extend(later)
    output[str(HOST_GROUPS).replace("\\", "/")] = import_world.canonical(new_groups)
    output[str(HOST_LAYOUTS).replace("\\", "/")] = import_world.canonical(new_layouts)
    for relative, data in output.items():
        if relative.startswith("data/cormoria/layouts/"):
            installed = root / relative
            if installed.exists() and installed.read_bytes() != data:
                raise MapRegistrationError(f"installed layout binary drifted: {relative}")
    return output


def build_region_map(stage: Path, root: Path = ROOT) -> dict[str, bytes]:
    """Translate the pinned donor's 28x15 map without changing host section IDs."""
    region, symbols, sources = import_world.load_manifests(root)
    _, source_index = import_world.selected_paths(region, symbols, sources)
    source = import_world.source_bytes(stage / "source", REGION_MAP_SOURCE, source_index)
    rows = [SECTION_TOKEN.findall(line) for line in source.decode("utf-8").splitlines()]
    rows = [row for row in rows if row]
    if len(rows) != 15 or any(len(row) != 28 for row in rows):
        raise MapRegistrationError("donor region map is not a 28x15 grid")
    mapping = {item["source_symbol"]: item["target_symbol"] for item in region["sections"]}
    mapping["MAPSEC_NONE"] = "MAPSEC_NONE"
    # CERAM_PEAK is drawn on the donor map, but has no imported map or section
    # allocation. Its cell belongs to Mt. Ceram; retain the geography without
    # inventing another location identity in the shared section registry.
    if [cell for row in rows for cell in row].count("MAPSEC_CERAM_PEAK") != 1:
        raise MapRegistrationError("donor-only Ceram Peak cell drifted")
    mapping["MAPSEC_CERAM_PEAK"] = "MAPSEC_CORMORIA_MT_CERAM"
    try:
        translated = [[mapping[cell] for cell in row] for row in rows]
    except KeyError as exc:
        raise MapRegistrationError(f"unmapped donor region-map section: {exc.args[0]}") from exc
    layout = ("// Generated from the pinned Dreamstone Mysteries region map; do not edit.\n"
              "// CERAM_PEAK is a visual alias of Mt. Ceram; no new section ID is allocated.\n"
              "static const mapsec_u16_t sRegionMapSections_Cormoria[MAP_HEIGHT][MAP_WIDTH] = {\n"
              + "".join("    {" + ", ".join(row) + "},\n" for row in translated)
              + "};\n")
    png = import_world.source_bytes(stage / "source", REGION_MAP_PNG, source_index)
    tilemap = import_world.source_bytes(stage / "source", REGION_MAP_TILEMAP, source_index)
    if png[:8] != b"\x89PNG\r\n\x1a\n" or len(tilemap) != 1280:
        raise MapRegistrationError("donor region-map graphics format drifted")
    return {
        REGION_MAP_LAYOUT: layout.encode("utf-8"),
        f"{REGION_MAP_ASSET_DIR}/map_cormoria.png": png,
        f"{REGION_MAP_ASSET_DIR}/map_cormoria.bin": tilemap,
    }


def write_registration(stage: Path, output: Path, root: Path = ROOT, *,
                       previous_item_registry: bytes | None = None,
                       trusted_previous_sha256: str | None = None) -> int:
    output = output.resolve()
    root = root.resolve()
    stage = stage.resolve(strict=True)
    if output == root or output.is_relative_to(root) or output.is_relative_to(stage) or output.exists():
        raise MapRegistrationError("output must be new and outside the host and staged trees")
    files = build_registration(stage, root, previous_item_registry=previous_item_registry,
                               trusted_previous_sha256=trusted_previous_sha256)
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="cormoria-map-registration-", dir=output.parent) as temporary:
        candidate = Path(temporary) / "result"
        for relative, data in files.items():
            path = candidate / import_world.safe_relative(relative)
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        candidate.rename(output)
    return len(files)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stage", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--previous-shared-items", type=Path)
    parser.add_argument("--trusted-previous-sha256")
    args = parser.parse_args(argv)
    try:
        prior = args.previous_shared_items.read_bytes() if args.previous_shared_items else None
        count = write_registration(args.stage, args.output, previous_item_registry=prior,
                                   trusted_previous_sha256=args.trusted_previous_sha256)
    except (OSError, KeyError, ValueError, json.JSONDecodeError) as exc:
        print(f"Cormoria map registration: {exc}", file=sys.stderr)
        return 1
    print(f"Prepared {count} authenticated Cormoria map/layout files at {args.output}; runtime adapters pending")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
