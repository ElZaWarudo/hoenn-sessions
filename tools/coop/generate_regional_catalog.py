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
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
MAP_GROUPS_PATH = ROOT / "data" / "maps" / "map_groups.json"
MAP_SECTIONS_PATH = ROOT / "src" / "data" / "region_map" / "region_map_sections.json"
REGIONS_C_PATH = ROOT / "src" / "regions.c"
OUTPUT_PATH = ROOT / "coop" / "crates" / "coop-protocol" / "src" / "generated_map_catalog.rs"

ENGINE_REGIONS = {"REGION_HOENN", "REGION_KANTO"}
SEVII_SUBREGIONS = ("SEVII123", "SEVII45", "SEVII67")


class CatalogError(ValueError):
    """A source inconsistency that must block catalog generation."""


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


def build_entries() -> list[tuple[str, str, int, int]]:
    groups_data = load_json(MAP_GROUPS_PATH)
    group_order = groups_data.get("group_order")
    if not isinstance(group_order, list) or not group_order:
        raise CatalogError("map group source has no group_order array")

    section_numbers, sevii_sections, special_area = source_sections()
    entries: list[tuple[str, str, int, int]] = []
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
            entries.append((region, map_id, group_number, map_number))

    if len(entries) != 935:
        raise CatalogError(f"expected 935 maps from map_groups.json, found {len(entries)}")
    return entries


def render(entries: list[tuple[str, str, int, int]]) -> str:
    lines = [
        "// This file is generated by tools/coop/generate_regional_catalog.py.",
        "// Do not edit it by hand; run the generator after changing map sources.",
        "",
        "pub const GENERATED_MAP_CATALOG: &[MapCatalogEntry] = &[",
    ]
    for region, map_key, group_number, map_number in entries:
        lines.extend(
            [
                "    MapCatalogEntry {",
                f"        region: RegionId::{region},",
                f'        map: "{map_key}",',
                f"        map_group: {group_number},",
                f"        map_number: {map_number},",
                "    },",
            ]
        )
    lines.extend(["];"])
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
        expected = render(build_entries())
        actual = OUTPUT_PATH.read_text(encoding="utf-8") if OUTPUT_PATH.exists() else None
        if args.check:
            if actual != expected:
                print(f"generated catalog is out of date: {OUTPUT_PATH}", file=sys.stderr)
                return 1
            return 0
        OUTPUT_PATH.write_text(expected, encoding="utf-8", newline="\n")
        return 0
    except (CatalogError, OSError) as error:
        print(f"regional catalog generation failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
