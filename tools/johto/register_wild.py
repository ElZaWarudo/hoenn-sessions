"""Generate the Johto wild encounter data consumed by the host runtime.

The accepted JSON fragment remains the source of truth.  This tool verifies
the pinned donor and region manifest before rendering the three runtime
artifacts, and check mode fails closed on missing or stale outputs.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
import tempfile
from pathlib import Path
from typing import Any

from tools.johto import import_wild


ROOT = Path(__file__).resolve().parents[2]
DONOR_SOURCE = import_wild.DONOR_SOURCE
ACCEPTED_SOURCE = ROOT / "data/johto/wild_encounters.json"
INFO_OUTPUT = ROOT / "src/data/johto/wild_info.h"
HEADERS_OUTPUT = ROOT / "src/data/johto/wild_headers.inc"
RUNTIME_OUTPUT = ROOT / "data/johto/wild_runtime.json"
OUTPUTS = (INFO_OUTPUT, HEADERS_OUTPUT, RUNTIME_OUTPUT)

DAY = "day"
NIGHT = "night"
FIELD_ORDER = import_wild.FIELD_ORDER
FIELD_MEMBERS = {
    "land_mons": "landMonsInfo",
    "water_mons": "waterMonsInfo",
    "rock_smash_mons": "rockSmashMonsInfo",
    "fishing_mons": "fishingMonsInfo",
}
TIME_MEMBERS = (
    ("TIME_MORNING", DAY),
    ("TIME_DAY", DAY),
    ("TIME_EVENING", NIGHT),
    ("TIME_NIGHT", NIGHT),
)


class WildRuntimeError(ValueError):
    """Raised when accepted data cannot be wired without guessing."""


def _read_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise WildRuntimeError(f"required JSON is missing: {path}") from exc
    except json.JSONDecodeError as exc:
        raise WildRuntimeError(f"invalid JSON in {path}: {exc}") from exc


def _sha256(path: Path) -> str:
    try:
        digest = hashlib.sha256()
        with path.open("rb") as source:
            for block in iter(lambda: source.read(1024 * 1024), b""):
                digest.update(block)
        return digest.hexdigest()
    except OSError as exc:
        raise WildRuntimeError(f"cannot hash required file {path}: {exc}") from exc


def _symbol(label: str, field_type: str) -> str:
    if not re.fullmatch(r"g[A-Za-z0-9_]+", label):
        raise WildRuntimeError(f"invalid source label {label!r}")
    return f"sJohtoWild_{label[1:]}_{field_type}"


def _info_symbol(label: str, field_type: str) -> str:
    return f"{_symbol(label, field_type)}Info"


def _validate_accepted(donor_root: Path) -> tuple[dict[str, Any], dict[str, Any], list[dict[str, Any]]]:
    """Verify accepted data against the pinned donor and return its rows."""

    import_wild._verify_pinned_donor(donor_root)
    accepted = _read_json(ACCEPTED_SOURCE)
    if not isinstance(accepted, dict):
        raise WildRuntimeError("accepted wild encounter data must be an object")
    if accepted.get("schema") != "johto-wild-encounters-v1":
        raise WildRuntimeError("accepted wild encounter schema drifted")
    if accepted.get("region") != "johto" or accepted.get("runtime_ready") is not False:
        raise WildRuntimeError("accepted source runtime status or region drifted")

    approved, manifest_hash = import_wild._load_manifest(ROOT)
    species = import_wild._species_symbols(ROOT)
    donor_rows, fields, source_count = import_wild._parse_selected(
        donor_root, ROOT, approved, species
    )
    groups = accepted.get("wild_encounter_groups")
    if not isinstance(groups, list) or len(groups) != 1:
        raise WildRuntimeError("accepted data must contain exactly one wild group")
    group = groups[0]
    if not isinstance(group, dict) or group.get("label") != "gJohtoWildMonHeaders":
        raise WildRuntimeError("accepted Johto wild group label drifted")
    if group.get("for_maps") is not True:
        raise WildRuntimeError("accepted Johto wild group must be map keyed")
    if group.get("fields") != fields:
        raise WildRuntimeError("accepted host probability vectors or field schema drifted")
    if group.get("encounters") != donor_rows:
        raise WildRuntimeError("accepted table payload differs from pinned donor")

    provenance = accepted.get("provenance")
    if not isinstance(provenance, dict):
        raise WildRuntimeError("accepted provenance is missing")
    donor_hash = _sha256(donor_root / DONOR_SOURCE)
    if provenance.get("donor_source_sha256") != donor_hash:
        raise WildRuntimeError("accepted donor source hash drifted")
    if provenance.get("region_manifest_sha256") != manifest_hash:
        raise WildRuntimeError("accepted region manifest hash drifted")

    selection = accepted.get("selection")
    if not isinstance(selection, dict):
        raise WildRuntimeError("accepted selection metadata is missing")
    if (
        selection.get("approved_map_count") != import_wild.EXPECTED_APPROVED_MAPS
        or selection.get("maps_with_encounters") != import_wild.EXPECTED_MAPS_WITH_ENCOUNTERS
        or selection.get("source_header_count") != import_wild.EXPECTED_SOURCE_HEADERS
        or selection.get("emitted_table_count") != import_wild.EXPECTED_EMITTED_TABLES
    ):
        raise WildRuntimeError("accepted selection counts drifted")
    if source_count != import_wild.EXPECTED_SOURCE_HEADERS:
        raise WildRuntimeError("pinned source header count drifted")

    by_map: dict[str, dict[str, dict[str, Any]]] = {}
    for entry in donor_rows:
        map_name = entry["map"]
        time = entry["time_of_day"]
        if map_name in by_map and time in by_map[map_name]:
            raise WildRuntimeError(f"duplicate accepted table for {map_name} {time}")
        linkage = entry.get("map_linkage")
        manifest_entry = approved.get(map_name)
        if not isinstance(linkage, dict) or manifest_entry is None:
            raise WildRuntimeError(f"missing linkage for {map_name}")
        expected_linkage = {
            "ordinal": manifest_entry["ordinal"],
            "source_map": map_name,
            "source_name": manifest_entry["source_name"],
            "source_group": manifest_entry["source_group"],
            "source_index": manifest_entry["source_index"],
            "host": {
                "map_id": manifest_entry["proposed_map"]["map_id"],
                "group": manifest_entry["proposed_host"]["group"],
                "index": manifest_entry["proposed_host"]["index"],
            },
        }
        if linkage != expected_linkage:
            raise WildRuntimeError(f"map linkage drifted for {map_name}")
        host = linkage["host"]
        if host["group"] not in (75, 76) or not 0 <= host["index"] <= 127:
            raise WildRuntimeError(f"Johto host linkage out of range for {map_name}")
        for field_type in FIELD_ORDER:
            if field_type in entry:
                import_wild._validate_mons(entry[field_type], field_type, species)
        by_map.setdefault(map_name, {})[time] = entry

    if len(by_map) != import_wild.EXPECTED_MAPS_WITH_ENCOUNTERS:
        raise WildRuntimeError("accepted map count is not 93")
    if sum(len(times) for times in by_map.values()) != import_wild.EXPECTED_EMITTED_TABLES:
        raise WildRuntimeError("accepted table count is not 147")
    fallback_maps = {name for name, times in by_map.items() if set(times) == {DAY}}
    metadata = accepted.get("absent_night_fallback")
    if not isinstance(metadata, dict) or metadata.get("count") != import_wild.EXPECTED_SINGLE_HEADER_MAPS:
        raise WildRuntimeError("accepted night fallback count drifted")
    listed_fallbacks = {item.get("map") for item in metadata.get("maps", []) if isinstance(item, dict)}
    if fallback_maps != listed_fallbacks:
        raise WildRuntimeError("accepted night fallback maps drifted")
    if any(set(times) not in ({DAY}, {DAY, NIGHT}) for times in by_map.values()):
        raise WildRuntimeError("accepted table time selection is incomplete")
    hosts = [(entry["map_linkage"]["host"]["group"], entry["map_linkage"]["host"]["index"]) for entry in (next(iter(times.values())) for times in by_map.values())]
    if len(hosts) != len(set(hosts)):
        raise WildRuntimeError("Johto runtime host linkages are not unique")
    return accepted, {"fields": fields, "by_map": by_map, "donor_hash": donor_hash, "manifest_hash": manifest_hash}, donor_rows


def _render_info(rows: list[dict[str, Any]]) -> str:
    lines = [
        "/* Generated by tools/johto/register_wild.py; do not edit. */",
        '#include "global.h"',
        '#include "wild_encounter.h"',
        "",
    ]
    seen: set[str] = set()
    for entry in rows:
        label = entry["base_label"]
        for field_type in FIELD_ORDER:
            if field_type not in entry:
                continue
            symbol = _symbol(label, field_type)
            if symbol in seen:
                raise WildRuntimeError(f"duplicate generated symbol {symbol}")
            seen.add(symbol)
            field = entry[field_type]
            lines.append(f"static const struct WildPokemon {symbol}[] =")
            lines.append("{")
            for mon in field["mons"]:
                lines.append(f"    {{ {mon['min_level']}, {mon['max_level']}, {mon['species']} }},")
            lines.extend([
                "};",
                "",
                f"static const struct WildPokemonInfo {_info_symbol(label, field_type)} = {{ {field['encounter_rate']}, {symbol} }};",
                "",
            ])
    return "\n".join(lines)


def _pointer(entry: dict[str, Any], field_type: str) -> str:
    if field_type not in entry:
        return "NULL"
    return f"&{_info_symbol(entry['base_label'], field_type)}"


def _render_headers(by_map: dict[str, dict[str, Any]]) -> str:
    lines = ["/* Generated by tools/johto/register_wild.py; do not edit. */"]
    maps = sorted(by_map, key=lambda name: by_map[name][DAY]["map_linkage"]["ordinal"])
    for map_name in maps:
        times = by_map[map_name]
        day_entry = times[DAY]
        night_entry = times.get(NIGHT, day_entry)
        host = day_entry["map_linkage"]["host"]
        lines.extend([
            "{",
            f"    .mapGroup = {host['group']},",
            f"    .mapNum = {host['index']},",
            "    .encounterTypes =",
            "    {",
        ])
        for enum_name, selected in TIME_MEMBERS:
            entry = day_entry if selected == DAY else night_entry
            lines.extend([
                f"        [{enum_name}] =",
                "        {",
            ])
            for field_type in FIELD_ORDER:
                lines.append(f"            .{FIELD_MEMBERS[field_type]} = {_pointer(entry, field_type)},")
            lines.extend(["        },"])
        lines.extend(["    },", "},", ""])
    return "\n".join(lines)


def _render_runtime(accepted: dict[str, Any], info: dict[str, Any]) -> str:
    by_map: dict[str, dict[str, Any]] = info["by_map"]
    maps = sorted(by_map, key=lambda name: by_map[name][DAY]["map_linkage"]["ordinal"])
    headers = []
    for map_name in maps:
        times = by_map[map_name]
        day = times[DAY]
        night = times.get(NIGHT)
        headers.append({
            "map": map_name,
            "map_linkage": day["map_linkage"],
            "day": day["base_label"],
            "night": night["base_label"] if night is not None else day["base_label"],
            "night_selection": "night" if night is not None else "day_fallback",
        })
    tables = []
    for entry in sorted(info["by_map"].values(), key=lambda times: (times[DAY]["map_linkage"]["ordinal"], next(iter(times.values()))["time_of_day"])):
        for time in (DAY, NIGHT):
            if time not in entry:
                continue
            row = entry[time]
            tables.append({
                "map": row["map"],
                "source_labels": row["source_labels"],
                "canonical_label": row["base_label"],
                "time_of_day": time,
                "symbol": row["base_label"],
                "pointers": {field: _pointer(row, field) for field in FIELD_ORDER},
                "map_linkage": row["map_linkage"],
                "selection": time,
            })
    runtime = {
        "schema_version": 1,
        "schema": "johto-wild-runtime-v1",
        "region": "johto",
        "runtime_ready": True,
        "runtime_status": "host_wild_headers_and_rtc_selector_wired",
        "world_status": "unwired",
        "limitation": "Map assets, navigation, native adapters, and campaign traversal remain outside this unit.",
        "provenance": {
            "accepted_source_path": "data/johto/wild_encounters.json",
            "accepted_source_sha256": _sha256(ACCEPTED_SOURCE),
            "donor_revision": import_wild.DONOR_REVISION,
            "donor_tree": import_wild.DONOR_TREE,
            "donor_source_path": DONOR_SOURCE,
            "donor_source_sha256": info["donor_hash"],
            "region_manifest_path": import_wild.MANIFEST_SOURCE,
            "region_manifest_sha256": info["manifest_hash"],
        },
        "selection": {
            "approved_map_count": import_wild.EXPECTED_APPROVED_MAPS,
            "runtime_header_count": len(headers),
            "source_header_count": import_wild.EXPECTED_SOURCE_HEADERS,
            "source_table_count": len(tables),
            "night_fallback_count": sum(1 for row in headers if row["night_selection"] == "day_fallback"),
            "host_time_config_independent": True,
        },
        "time_windows": {
            "day": {"start": "06:00", "end": "17:59"},
            "night": {"start": "18:00", "end": "05:59"},
            "selector": "JohtoWild_TimeForHour",
        },
        "headers": headers,
        "tables": tables,
    }
    return json.dumps(runtime, indent=2, ensure_ascii=False) + "\n"


def build_artifacts(donor_root: Path) -> dict[Path, str]:
    accepted, info, rows = _validate_accepted(donor_root.resolve())
    return {
        INFO_OUTPUT: _render_info(rows),
        HEADERS_OUTPUT: _render_headers(info["by_map"]),
        RUNTIME_OUTPUT: _render_runtime(accepted, info),
    }


def _write_all(artifacts: dict[Path, str]) -> None:
    temporary: list[tuple[Path, Path]] = []
    try:
        for output, content in artifacts.items():
            output.parent.mkdir(parents=True, exist_ok=True)
            with tempfile.NamedTemporaryFile(
                mode="w", encoding="utf-8", newline="\n", dir=output.parent,
                prefix=f".{output.name}.", suffix=".tmp", delete=False,
            ) as handle:
                handle.write(content)
                temporary.append((Path(handle.name), output))
        for temp, output in temporary:
            os.replace(temp, output)
    finally:
        for temp, _output in temporary:
            try:
                temp.unlink()
            except FileNotFoundError:
                pass


def run(donor_root: Path, *, write: bool, check: bool) -> int:
    if write == check:
        raise WildRuntimeError("choose exactly one of --write or --check")
    artifacts = build_artifacts(donor_root)
    if check:
        for output, expected in artifacts.items():
            if not output.is_file():
                raise WildRuntimeError(f"generated file is missing: {output}")
            try:
                current = output.read_text(encoding="utf-8")
            except OSError as exc:
                raise WildRuntimeError(f"cannot read generated file {output}: {exc}") from exc
            if current != expected:
                raise WildRuntimeError(f"generated file drift: {output}")
        return 0
    _write_all(artifacts)
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--donor-root", type=Path, required=True)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--write", action="store_true")
    mode.add_argument("--check", action="store_true")
    args = parser.parse_args(argv)
    try:
        return run(args.donor_root, write=args.write, check=args.check)
    except (OSError, WildRuntimeError, import_wild.ImportErrorStrict, ValueError) as exc:
        print(f"johto wild runtime: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
