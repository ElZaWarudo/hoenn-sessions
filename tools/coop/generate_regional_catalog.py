#!/usr/bin/env python3
"""Generate the complete region-qualified co-op map catalog.

The map-group JSON is the engine's authoritative ordering for numeric map
coordinates.  Region names come from each map's JSON header, while Sevii is
derived from the same section table used by ``src/regions.c``.  Keeping those
inputs here makes a catalog drift or an ambiguous map fail at generation time
instead of becoming a runtime identity collision.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
MAP_GROUPS_PATH = ROOT / "data" / "maps" / "map_groups.json"
LAYOUTS_PATH = ROOT / "data" / "layouts" / "layouts.json"
MAP_SECTIONS_PATH = ROOT / "src" / "data" / "region_map" / "region_map_sections.json"
REGIONS_C_PATH = ROOT / "src" / "regions.c"
OUTPUT_PATH = ROOT / "coop" / "crates" / "coop-protocol" / "src" / "generated_map_catalog.rs"
CONNECTIONS_OUTPUT_PATH = ROOT / "coop" / "crates" / "coop-protocol" / "src" / "generated_map_connections.rs"

ENGINE_REGIONS = {"REGION_HOENN", "REGION_KANTO", "REGION_JOHTO"}
SEVII_SUBREGIONS = ("SEVII123", "SEVII45", "SEVII67")
KANTO_ENGINE_JOHTO_SECTIONS = {
    "MAPSEC_JOHTO_ROUTE_26",
    "MAPSEC_JOHTO_ROUTE_27",
    "MAPSEC_JOHTO_ROUTE_28",
}
EXPECTED_MAP_COUNT = 1344
OUTDOOR_MAP_TYPES = {
    "MAP_TYPE_ROUTE",
    "MAP_TYPE_TOWN",
    "MAP_TYPE_UNDERWATER",
    "MAP_TYPE_CITY",
    "MAP_TYPE_OCEAN_ROUTE",
}


class CatalogError(ValueError):
    """A source inconsistency that must block catalog generation."""


@dataclass(frozen=True)
class CatalogRecord:
    region: str
    map_key: str
    group: int
    number: int
    width: int
    height: int
    allow_escaping: bool
    map_data: dict[str, Any]
    source_dir: Path


def load_json(path: Path) -> Any:
    try:
        with path.open(encoding="utf-8") as source:
            return json.load(source)
    except (OSError, json.JSONDecodeError) as error:
        raise CatalogError(f"cannot read {path}: {error}") from error


def source_sections() -> tuple[dict[str, int], set[str], str]:
    sections_data = load_json(MAP_SECTIONS_PATH)
    sections = sections_data.get("map_sections")
    if not isinstance(sections, list) or not sections:
        raise CatalogError("region map section source has no map_sections array")

    section_numbers: dict[str, int] = {}
    for number, section in enumerate(sections):
        section_id = section.get("id") if isinstance(section, dict) else None
        if not isinstance(section_id, str) or not section_id:
            raise CatalogError(f"invalid map section at index {number}")
        if section_id in section_numbers:
            raise CatalogError(f"duplicate map section {section_id}")
        section_numbers[section_id] = number

    region_source = REGIONS_C_PATH.read_text(encoding="utf-8")
    sevii_sections: set[str] = set()
    for subregion in SEVII_SUBREGIONS:
        match = re.search(
            rf"\[KANTO_SUBREGION_{subregion}\]\s*=\s*\{{([^}}]*)\}}",
            region_source,
            re.DOTALL,
        )
        if match is None:
            raise CatalogError(f"missing Sevii authority for {subregion}")
        sevii_sections.update(
            section
            for section in re.findall(r"MAPSEC_[A-Z0-9_]+", match.group(1))
            if section != "MAPSEC_NONE"
        )

    unknown_sevii = sorted(sevii_sections.difference(section_numbers))
    if unknown_sevii:
        raise CatalogError(
            "regions.c names unknown map sections: " + ", ".join(unknown_sevii)
        )

    try:
        section_numbers["MAPSEC_PALLET_TOWN"]
        section_numbers["MAPSEC_SPECIAL_AREA"]
    except KeyError as error:
        raise CatalogError(f"missing section boundary {error.args[0]}") from error

    return section_numbers, sevii_sections, "MAPSEC_SPECIAL_AREA"


def protocol_region(
    engine_region: str,
    section_id: str,
    section_numbers: dict[str, int],
    sevii_sections: set[str],
    special_area: str,
) -> str:
    if engine_region not in ENGINE_REGIONS:
        raise CatalogError(f"unsupported map engine region {engine_region}")
    if section_id not in section_numbers or section_id == "MAPSEC_NONE":
        raise CatalogError(f"unknown map section {section_id}")

    if engine_region == "REGION_JOHTO":
        if section_id in KANTO_ENGINE_JOHTO_SECTIONS:
            raise CatalogError(
                f"geographic Kanto section {section_id} requires REGION_KANTO"
            )
        johto_start = section_numbers["MAPSEC_NEW_BARK_TOWN"]
        johto_end = section_numbers["MAPSEC_JOHTO_SS_AQUA"]
        if not johto_start <= section_numbers[section_id] <= johto_end:
            raise CatalogError(f"Johto map section {section_id} is not registered")
        return "Johto"
    if (
        section_numbers["MAPSEC_NEW_BARK_TOWN"]
        <= section_numbers[section_id]
        <= section_numbers["MAPSEC_JOHTO_SS_AQUA"]
        and not (
            engine_region == "REGION_KANTO"
            and section_id in KANTO_ENGINE_JOHTO_SECTIONS
        )
    ):
        raise CatalogError(f"Johto map section {section_id} contradicts its engine region")

    kanto_start = section_numbers["MAPSEC_PALLET_TOWN"]
    section_number = section_numbers[section_id]
    if engine_region == "REGION_HOENN":
        if kanto_start <= section_number < section_numbers[special_area]:
            raise CatalogError(
                f"Hoenn map section {section_id} contradicts its engine region"
            )
        return "Hoenn"

    if section_id in sevii_sections:
        return "Sevii"
    if section_id in KANTO_ENGINE_JOHTO_SECTIONS:
        return "Kanto"
    if kanto_start <= section_number <= section_numbers[special_area]:
        return "Kanto"
    raise CatalogError(
        f"Kanto map section {section_id} is outside Kanto/Sevii authority"
    )


def canonical_map_key(map_id: Any, source: Path) -> str:
    if not isinstance(map_id, str) or not map_id.startswith("MAP_"):
        raise CatalogError(f"{source} has no MAP_ map id")
    key = map_id.removeprefix("MAP_")
    if not key or not re.fullmatch(r"[A-Z0-9_]+", key):
        raise CatalogError(f"{source} has a non-canonical map key {key!r}")
    return key


def safe_map_source(directory_name: Any) -> Path:
    """Return one map source path, rejecting traversal and absolute names."""
    if not isinstance(directory_name, str) or not directory_name:
        raise CatalogError(f"invalid map directory name {directory_name!r}")
    if (
        directory_name in {".", ".."}
        or "/" in directory_name
        or "\\" in directory_name
        or ":" in directory_name
    ):
        raise CatalogError(
            f"map directory must be one safe repository-relative component: {directory_name!r}"
        )

    maps_root = (ROOT / "data" / "maps").resolve()
    source = (maps_root / directory_name / "map.json").resolve()
    try:
        source.relative_to(maps_root)
    except ValueError as error:
        raise CatalogError(
            f"map directory escapes data/maps: {directory_name!r}"
        ) from error
    return source


def run_path_safety_self_test() -> None:
    """Keep the traversal guard deterministic and executable with every run."""
    for unsafe in ("../outside", "..", ".", "nested/map", "nested\\map", "/tmp/map"):
        try:
            safe_map_source(unsafe)
        except CatalogError:
            continue
        raise CatalogError(f"path-safety self-test accepted {unsafe!r}")


def load_layouts() -> dict[str, tuple[int, int]]:
    layouts_data = load_json(LAYOUTS_PATH)
    layouts = layouts_data.get("layouts")
    if not isinstance(layouts, list) or not layouts:
        raise CatalogError("layout source has no layouts array")
    result: dict[str, tuple[int, int]] = {}
    for index, layout in enumerate(layouts):
        if not isinstance(layout, dict):
            raise CatalogError(f"invalid layout at index {index}")
        layout_id = layout.get("id")
        width = layout.get("width")
        height = layout.get("height")
        if (
            not isinstance(layout_id, str)
            or not layout_id
            or not isinstance(width, int)
            or isinstance(width, bool)
            or not isinstance(height, int)
            or isinstance(height, bool)
            or width <= 0
            or height <= 0
        ):
            raise CatalogError(f"invalid dimensions for layout at index {index}")
        if layout_id in result:
            raise CatalogError(f"duplicate layout {layout_id}")
        result[layout_id] = (width, height)
    return result


def _build_catalog_records() -> list[CatalogRecord]:
    groups_data = load_json(MAP_GROUPS_PATH)
    group_order = groups_data.get("group_order")
    if not isinstance(group_order, list) or not group_order:
        raise CatalogError("map group source has no group_order array")

    section_numbers, sevii_sections, special_area = source_sections()
    layouts = load_layouts()
    entries: list[CatalogRecord] = []
    seen_names: set[str] = set()
    seen_keys: set[tuple[str, str]] = set()
    seen_coordinates: set[tuple[int, int]] = set()

    for group_number, group_name in enumerate(group_order):
        if not isinstance(group_name, str) or not group_name:
            raise CatalogError(f"invalid map group at index {group_number}")
        maps = groups_data.get(group_name)
        if not isinstance(maps, list):
            raise CatalogError(f"map group {group_name} has no map list")
        for map_number, directory_name in enumerate(maps):
            if not isinstance(directory_name, str) or not directory_name:
                raise CatalogError(
                    f"invalid map name in {group_name} at index {map_number}"
                )
            if directory_name in seen_names:
                raise CatalogError(f"map {directory_name} appears more than once")
            seen_names.add(directory_name)

            source = safe_map_source(directory_name)
            map_data = load_json(source)
            map_id = canonical_map_key(map_data.get("id"), source)
            layout_id = map_data.get("layout")
            if not isinstance(layout_id, str) or layout_id not in layouts:
                raise CatalogError(f"{source} names unknown layout {layout_id!r}")
            width, height = layouts[layout_id]
            allow_escaping = map_data.get("allow_escaping")
            if not isinstance(allow_escaping, bool):
                raise CatalogError(f"{source} has an invalid allow_escaping value")
            map_region = map_data.get("region", "REGION_HOENN")
            if not isinstance(map_region, str) or not map_region:
                raise CatalogError(f"{source} has an invalid explicit region")
            section_id = map_data.get("region_map_section")
            if section_id not in section_numbers:
                raise CatalogError(f"{source} names unknown map section {section_id!r}")
            region = protocol_region(
                map_region,
                section_id,
                section_numbers,
                sevii_sections,
                special_area,
            )

            key = (region, map_id)
            if key in seen_keys:
                raise CatalogError(f"duplicate region-qualified map key {region}:{map_id}")
            seen_keys.add(key)
            coordinates = (group_number, map_number)
            if coordinates in seen_coordinates:
                raise CatalogError(
                    f"duplicate numeric map coordinates {group_number}:{map_number}"
                )
            seen_coordinates.add(coordinates)
            entries.append(
                CatalogRecord(
                    region,
                    map_id,
                    group_number,
                    map_number,
                    width,
                    height,
                    allow_escaping,
                    map_data,
                    source.parent,
                )
            )

    if len(entries) != EXPECTED_MAP_COUNT:
        raise CatalogError(
            f"expected {EXPECTED_MAP_COUNT} maps from map_groups.json, found {len(entries)}"
        )
    return entries


def build_entries() -> list[tuple[str, str, int, int]]:
    return [
        (record.region, record.map_key, record.group, record.number)
        for record in _build_catalog_records()
    ]


SETESCAPEWARP_RE = re.compile(
    r"^\s*setescapewarp\s+(MAP_[A-Z0-9_]+)\s*,\s*(-?\d+)\s*,\s*(-?\d+)"
    r"(?:\s*,\s*(-?\d+))?\s*(?:@.*)?$"
)
WARP_ID_NONE = 255
# Scripted escape warps whose ``setescapewarp`` lives outside the map that is
# later escaped from.  Each entry names the shared script file, the label
# prefix whose ``setescapewarp`` lines apply, and the escape-enabled map that
# inherits them.  Vanilla Emerald sets these on Underwater_Route105/125/127/129
# before the dive into the Marine Cave, which does not run UpdateEscapeWarp.
SHARED_SCRIPTED_ESCAPE_SEEDS = (
    (
        ROOT / "data" / "scripts" / "abnormal_weather.inc",
        "AbnormalWeather_Underwater_SetupEscapeWarpRoute",
        "MARINE_CAVE_ENTRANCE",
    ),
)


def parse_setescapewarps(
    path: Path, label_prefix: str | None = None
) -> list[tuple[str, int, int]]:
    """Return ``(map_id, x, y)`` for literal WARP_ID_NONE ``setescapewarp`` lines.

    A warp-id form resolves through the destination map's warp table at run
    time and is not a fixed coordinate, so it is rejected rather than guessed.
    """
    if not path.exists():
        return []
    results: list[tuple[str, int, int]] = []
    in_scope = label_prefix is None
    for line in path.read_text(encoding="utf-8").splitlines():
        label = re.match(r"^([A-Za-z0-9_]+)::", line)
        if label and label_prefix is not None:
            in_scope = label.group(1).startswith(label_prefix)
            continue
        if "setescapewarp" not in line or not in_scope:
            continue
        match = SETESCAPEWARP_RE.match(line)
        if match is None:
            raise CatalogError(f"{path} has an unparsed setescapewarp: {line.strip()}")
        map_id, first, second, third = match.groups()
        if third is None:
            results.append((map_id, int(first), int(second)))
        elif int(first) == WARP_ID_NONE:
            results.append((map_id, int(second), int(third)))
        else:
            raise CatalogError(f"{path} uses an unsupported warp-id setescapewarp")
    return results


def build_escape_targets(
    records: list[CatalogRecord],
) -> tuple[list[tuple[int, int, int, int]], dict[str, tuple[int, int]]]:
    """Resolve vanilla escape endpoints from map topology and escape scripts.

    Mirrors ``UpdateEscapeWarp``: stepping on a warp from an outdoor map into a
    non-outdoor map seeds ``escapeWarp`` one tile below the warp tile, and
    every other transition preserves it.  Literal ``setescapewarp`` commands
    in a map's own scripts, plus the listed shared scripts, seed the maps they
    run for.  Script-driven endpoints not covered by those sources (for example
    a ``setescapewarp`` computed from variables, or a warp-id form) are not
    inferred, so the server rejects them rather than trusting an arbitrary
    client endpoint.
    """
    by_id = {f"MAP_{record.map_key}": record for record in records}
    by_key = {record.map_key: record for record in records}
    targets_by_map: dict[str, set[tuple[int, int, int, int]]] = {
        record.map_key: set() for record in records
    }

    def outdoor(record: CatalogRecord) -> bool:
        return record.map_data.get("map_type") in OUTDOOR_MAP_TYPES

    def add_target(destination: CatalogRecord, map_id: str, x: Any, y: Any) -> None:
        target = by_id.get(map_id)
        if (
            target is None
            or not isinstance(x, int)
            or isinstance(x, bool)
            or not isinstance(y, int)
            or isinstance(y, bool)
            or not 0 <= target.group <= 127
            or not 0 <= target.number <= 127
            or not 0 <= x < min(target.width, 128)
            or not 0 <= y < min(target.height, 128)
        ):
            return
        targets_by_map[destination.map_key].add((target.group, target.number, x, y))

    # An outdoor step-warp into a non-outdoor map seeds the inherited endpoint.
    for source in records:
        if not outdoor(source):
            continue
        for warp in source.map_data.get("warp_events") or []:
            if not isinstance(warp, dict):
                continue
            destination = by_id.get(warp.get("dest_map"))
            if destination is not None and not outdoor(destination):
                y = warp.get("y")
                add_target(
                    destination,
                    f"MAP_{source.map_key}",
                    warp.get("x"),
                    y + 1 if isinstance(y, int) and not isinstance(y, bool) else None,
                )

    # Literal scripted escape warps seed the map whose scripts set them.
    for record in records:
        for map_id, x, y in parse_setescapewarps(record.source_dir / "scripts.inc"):
            add_target(record, map_id, x, y)
    for path, label_prefix, map_key in SHARED_SCRIPTED_ESCAPE_SEEDS:
        seeds = parse_setescapewarps(path, label_prefix)
        if not seeds or map_key not in by_key:
            raise CatalogError(f"shared escape seed {label_prefix} for {map_key} is stale")
        for map_id, x, y in seeds:
            add_target(by_key[map_key], map_id, x, y)

    # Every other transition preserves escapeWarp.  Propagation stops at
    # outdoor maps that forbid escaping so ordinary routes do not accumulate
    # every stale endpoint in the region; escaping from a map reached only
    # through such a route stays closed at the server boundary.
    changed = True
    while changed:
        changed = False
        for source in records:
            inherited = targets_by_map[source.map_key]
            if not inherited:
                continue
            edges = [
                (warp.get("dest_map"), True)
                for warp in source.map_data.get("warp_events") or []
                if isinstance(warp, dict)
            ] + [
                (connection.get("map"), False)
                for connection in source.map_data.get("connections") or []
                if isinstance(connection, dict)
            ]
            for edge, is_warp in edges:
                destination = by_id.get(edge)
                if destination is None:
                    continue
                if outdoor(destination) and not destination.allow_escaping:
                    continue
                if is_warp and outdoor(source) and not outdoor(destination):
                    continue  # UpdateEscapeWarp replaced the endpoint above.
                before = len(targets_by_map[destination.map_key])
                targets_by_map[destination.map_key].update(inherited)
                changed |= len(targets_by_map[destination.map_key]) != before

    flattened: list[tuple[int, int, int, int]] = []
    ranges: dict[str, tuple[int, int]] = {}
    for record in records:
        values = sorted(targets_by_map[record.map_key]) if record.allow_escaping else []
        start = len(flattened)
        flattened.extend(values)
        ranges[record.map_key] = (start, len(values))
    if len(flattened) > 0xFFFF:
        raise CatalogError("generated escape endpoint table exceeds u16 indexing")
    return flattened, ranges


def render(
    entries: list[tuple[str, str, int, int]],
    records: list[CatalogRecord] | None = None,
) -> str:
    records = records or _build_catalog_records()
    records_by_key = {record.map_key: record for record in records}
    escape_targets, ranges = build_escape_targets(records)
    lines = [
        "// This file is generated by tools/coop/generate_regional_catalog.py.",
        "// Do not edit it by hand; run the generator after changing map sources.",
        "",
        "pub const GENERATED_MAP_ESCAPE_TARGETS: &[MapEscapeTarget] = &[",
    ]
    for group, number, x, y in escape_targets:
        lines.append(
            "    MapEscapeTarget { "
            f"map_group: {group}, map_number: {number}, x: {x}, y: {y} "
            "},"
        )
    lines.extend(
        [
            "];",
            "",
            "pub const GENERATED_MAP_CATALOG: &[MapCatalogEntry] = &[",
        ]
    )
    for region, map_key, group_number, map_number in entries:
        record = records_by_key[map_key]
        target_start, target_len = ranges[map_key]
        lines.extend(
            [
                "    MapCatalogEntry {",
                f"        region: RegionId::{region},",
                f'        map: "{map_key}",',
                f"        map_group: {group_number},",
                f"        map_number: {map_number},",
                f"        width: {record.width},",
                f"        height: {record.height},",
                f"        allow_escaping: {str(record.allow_escaping).lower()},",
                f"        escape_targets_start: {target_start},",
                f"        escape_targets_len: {target_len},",
                "    },",
            ]
        )
    lines.extend(["];"])
    return "\n".join(lines) + "\n"


def build_connections(
    entries: list[tuple[str, str, int, int]],
) -> list[tuple[int, int, int, int]]:
    """Resolve cardinal engine connections to exact catalog coordinates."""
    groups_data = load_json(MAP_GROUPS_PATH)
    by_id = {f"MAP_{map_key}": (region, group, number)
             for region, map_key, group, number in entries}
    entries_by_coordinates = {(group, number): (region, map_key)
                              for region, map_key, group, number in entries}
    if len(by_id) != len(entries):
        raise CatalogError("map IDs are not unique across the catalog")
    result: set[tuple[int, int, int, int]] = set()
    for group, group_name in enumerate(groups_data["group_order"]):
        for number, directory_name in enumerate(groups_data[group_name]):
            source = safe_map_source(directory_name)
            map_data = load_json(source)
            connections = map_data.get("connections", [])
            if connections is None or connections == 0:
                connections = []
            if not isinstance(connections, list):
                raise CatalogError(f"invalid connection list in {source}")
            for connection in connections:
                if not isinstance(connection, dict):
                    raise CatalogError(f"invalid connection in {source}")
                direction = connection.get("direction")
                if direction not in ("up", "down", "left", "right"):
                    continue  # Dive/emerge and warps do not share a map edge.
                target = connection.get("map")
                if not isinstance(target, str):
                    raise CatalogError(f"invalid connection target in {source}")
                if target not in by_id:
                    raise CatalogError(f"{source} connects to unknown map {target!r}")
                target_region, target_group, target_number = by_id[target]
                source_region = entries_by_coordinates[(group, number)][0]
                if source_region == target_region:
                    result.add((group, number, target_group, target_number))
    return sorted(result)


def render_connections(connections: list[tuple[int, int, int, int]]) -> str:
    lines = [
        "// This file is generated by tools/coop/generate_regional_catalog.py.",
        "// Do not edit it by hand; run the generator after changing map sources.",
        "",
        "pub const GENERATED_MAP_CONNECTIONS: &[MapConnectionEntry] = &[",
    ]
    for group, number, target_group, target_number in connections:
        lines.append(
            "    MapConnectionEntry { "
            f"from_group: {group}, from_number: {number}, "
            f"to_group: {target_group}, to_number: {target_number} "
            "},"
        )
    lines.append("];")
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="verify the checked-in generated catalog without writing it",
    )
    args = parser.parse_args()

    try:
        run_path_safety_self_test()
        records = _build_catalog_records()
        entries = [
            (record.region, record.map_key, record.group, record.number)
            for record in records
        ]
        expected = render(entries, records)
        expected_connections = render_connections(build_connections(entries))
        actual = OUTPUT_PATH.read_text(encoding="utf-8") if OUTPUT_PATH.exists() else None
        actual_connections = (CONNECTIONS_OUTPUT_PATH.read_text(encoding="utf-8")
                              if CONNECTIONS_OUTPUT_PATH.exists() else None)
        if args.check:
            if actual != expected or actual_connections != expected_connections:
                print(f"generated catalog is out of date: {OUTPUT_PATH}", file=sys.stderr)
                return 1
            return 0
        OUTPUT_PATH.write_text(expected, encoding="utf-8", newline="\n")
        CONNECTIONS_OUTPUT_PATH.write_text(expected_connections, encoding="utf-8", newline="\n")
        return 0
    except (CatalogError, OSError) as error:
        print(f"regional catalog generation failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
