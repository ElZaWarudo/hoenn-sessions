"""Prepare the append-only Cormoria map-section source from an authenticated stage.

The default mode writes to stdout. An installed Cormoria allocation is
validated in place, and any later region's section tail is preserved.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path
from typing import Any

from tools.cormoria import import_world
from tools.johto.region_manifest import EXPECTED_SECTION_TAIL_IDS

ROOT = Path(__file__).resolve().parents[2]
HOST_SECTIONS = Path("src/data/region_map/region_map_sections.json")
DONOR_SECTIONS = Path("source/src/data/region_map/region_map_sections.json")
HOST_COUNT = 250
# SHA-256 of the canonical JSON array of all 250 existing section IDs. A tail-
# only check would allow older host IDs to be silently renamed or reordered.
HOST_IDS_SHA256 = "6b27919e3f5c0148bb98ebc5297ab649fff7e8539a87621caa9588e4b12fee9f"
# Two donor display names are stale copies from other locations. Keep their
# source spelling as a drift check so future donor changes are reviewed.
DISPLAY_NAME_CORRECTIONS = {
    "MAPSEC_CORMORIA_IVY_RIVER": ("Lily Pond", "Ivy River"),
    "MAPSEC_CORMORIA_CHAMPIONSHIP_CORRIDOR": ("SIX ISLAND", "Champion Corridor"),
}


class SectionRegistrationError(ValueError):
    """A staged or host section does not match the pinned allocation."""


def _load(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict) or not isinstance(value.get("map_sections"), list):
        raise SectionRegistrationError(f"invalid map section document: {path}")
    return value


def _staged_sections(stage: Path, sources: dict[str, Any]) -> list[dict[str, Any]]:
    relative = str(DONOR_SECTIONS.relative_to("source")).replace("\\", "/")
    record = next((item for item in sources["files"] if item["path"] == relative), None)
    if record is None:
        raise SectionRegistrationError("donor section source is absent from pinned manifest")
    path = stage / DONOR_SECTIONS
    if not path.is_file() or not path.resolve().is_relative_to(stage.resolve()):
        raise SectionRegistrationError("staged donor section source is missing or escapes stage")
    data = path.read_bytes()
    if len(data) != record["bytes"] or hashlib.sha256(data).hexdigest() != record["sha256"]:
        raise SectionRegistrationError("staged donor section source hash differs from pinned manifest")
    return _load(path)["map_sections"]


def append_sections(host: list[dict[str, Any]], donor: list[dict[str, Any]],
                    planned: list[dict[str, Any]]) -> list[dict[str, Any]]:
    if len(host) < HOST_COUNT:
        raise SectionRegistrationError("host section baseline must have 250 records")
    host_ids = [item.get("id") for item in host[:HOST_COUNT]]
    ids_bytes = json.dumps(host_ids, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    if (hashlib.sha256(ids_bytes).hexdigest() != HOST_IDS_SHA256
            or host_ids[210:HOST_COUNT] != list(EXPECTED_SECTION_TAIL_IDS)):
        raise SectionRegistrationError("host section identity or order drifted")
    by_source: dict[str, dict[str, Any]] = {}
    for item in donor:
        source = item.get("map_section")
        if not isinstance(source, str) or source in by_source:
            raise SectionRegistrationError("donor section source has invalid or duplicate IDs")
        by_source[source] = item
    if len(planned) != 51:
        raise SectionRegistrationError("expected 51 allocated Cormoria sections")
    result = list(host[:HOST_COUNT])
    for offset, allocation in enumerate(planned):
        source = allocation["source_symbol"]
        target = allocation["target_symbol"]
        if allocation["target_id"] != HOST_COUNT + offset or target in host_ids:
            raise SectionRegistrationError(f"section allocation drift: {target}")
        record = by_source.get(source)
        if record is None or set(record) != {"map_section", "name", "x", "y", "width", "height"}:
            raise SectionRegistrationError(f"missing or unsupported donor section: {source}")
        if not isinstance(record["name"], str):
            raise SectionRegistrationError(f"invalid section name: {source}")
        name = record["name"]
        if target in DISPLAY_NAME_CORRECTIONS:
            stale, name = DISPLAY_NAME_CORRECTIONS[target]
            if record["name"] != stale:
                raise SectionRegistrationError(f"display name correction drift: {target}")
        for key in ("x", "y", "width", "height"):
            if type(record[key]) is not int or record[key] < (1 if key in ("width", "height") else 0):
                raise SectionRegistrationError(f"invalid section geometry: {source}")
        result.append({"id": target, "name": name, "x": record["x"],
                       "y": record["y"], "width": record["width"], "height": record["height"]})
        host_ids.append(target)
    if len(host) > HOST_COUNT:
        if len(host) < len(result) or host[:len(result)] != result:
            raise SectionRegistrationError("installed Cormoria section allocation drifted")
        later = host[len(result):]
        later_ids = [entry.get("id") for entry in later]
        if len(set(later_ids)) != len(later_ids) or set(later_ids) & set(host_ids):
            raise SectionRegistrationError("later region reuses a section ID")
        result.extend(later)
    return result


def build_sections(stage: Path, root: Path = ROOT) -> dict[str, Any]:
    region, _, sources = import_world.load_manifests(root)
    host = _load(root / HOST_SECTIONS)["map_sections"]
    donor = _staged_sections(stage.resolve(), sources)
    return {"map_sections": append_sections(host, donor, region["sections"])}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stage", required=True, type=Path)
    parser.add_argument("--output", type=Path, help="write an explicit output path; never overwrites the live source")
    args = parser.parse_args(argv)
    try:
        rendered = json.dumps(build_sections(args.stage), indent=2, ensure_ascii=False) + "\n"
        if args.output is None:
            sys.stdout.write(rendered)
        else:
            if args.output.resolve() == (ROOT / HOST_SECTIONS).resolve() or args.output.exists():
                raise SectionRegistrationError("refusing to overwrite live or existing section source")
            args.output.write_text(rendered, encoding="utf-8", newline="\n")
    except (OSError, KeyError, ValueError, json.JSONDecodeError) as exc:
        print(f"Cormoria section registration: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
