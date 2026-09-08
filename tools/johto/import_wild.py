"""Import the pinned Johto wild encounter tables as a reproducible data fragment.

The generated JSON deliberately stops at the data boundary.  Encounter
selection and engine registration remain runtime work owned by a later lane.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
DONOR_REVISION = "751823abaf677020bcd72c45fe3e7cb2b8a576e4"
DONOR_TREE = "33661709e5368edc01c37ed9bb5e0a7a0cb192c8"
DONOR_REPOSITORY = "https://github.com/PokemonHnS-Development/pokemonHnS"
DONOR_SOURCE = "src/data/wild_encounters.json"
MANIFEST_SOURCE = "data/johto/region_manifest.json"
SPECIES_SOURCE = "include/constants/species.h"
OUTPUT_SOURCE = "data/johto/wild_encounters.json"
EXPECTED_APPROVED_MAPS = 239
EXPECTED_MAPS_WITH_ENCOUNTERS = 93
EXPECTED_SOURCE_HEADERS = 149
EXPECTED_EMITTED_TABLES = 147
EXPECTED_SINGLE_HEADER_MAPS = 39
JOHTO_WILD_GROUP = "gJohtoWildMonHeaders"

FIELD_ORDER = ("land_mons", "water_mons", "rock_smash_mons", "fishing_mons")
FIELD_SLOT_LENGTHS = {
    "land_mons": {12},
    "water_mons": {5, 12},
    "rock_smash_mons": {5},
    "fishing_mons": {10, 12},
}
FIELD_KEYS = set(FIELD_ORDER)
SOURCE_LABEL_RE = re.compile(r"^g[A-Za-z0-9_]+$")
SPECIES_RE = re.compile(r"^SPECIES_[A-Z0-9_]+$")


class ImportErrorStrict(ValueError):
    """Raised when the pinned source cannot be converted without guesswork."""


def _read_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise ImportErrorStrict(f"required JSON file is missing: {path}") from exc
    except json.JSONDecodeError as exc:
        raise ImportErrorStrict(f"invalid JSON in {path}: {exc}") from exc


def _sha256(path: Path) -> str:
    try:
        digest = hashlib.sha256()
        with path.open("rb") as source:
            for block in iter(lambda: source.read(1024 * 1024), b""):
                digest.update(block)
    except OSError as exc:
        raise ImportErrorStrict(f"cannot hash required file {path}: {exc}") from exc
    return digest.hexdigest()


def _git(donor: Path, *args: str) -> str:
    try:
        return subprocess.check_output(
            ["git", "-C", str(donor), *args],
            text=True,
            stderr=subprocess.STDOUT,
        ).strip()
    except (OSError, subprocess.CalledProcessError) as exc:
        detail = getattr(exc, "output", "") or str(exc)
        raise ImportErrorStrict(f"donor git verification failed: {detail.strip()}") from exc


def _verify_pinned_donor(donor: Path) -> None:
    if not donor.is_dir():
        raise ImportErrorStrict(f"donor root does not exist: {donor}")
    revision = _git(donor, "rev-parse", "HEAD")
    tree = _git(donor, "rev-parse", "HEAD^{tree}")
    if revision != DONOR_REVISION or tree != DONOR_TREE:
        raise ImportErrorStrict(
            f"donor pin mismatch: revision={revision}, tree={tree}; "
            f"expected {DONOR_REVISION}/{DONOR_TREE}"
        )
    if _git(donor, "status", "--porcelain"):
        raise ImportErrorStrict("donor working tree must be clean")


def _load_manifest(repo_root: Path) -> tuple[dict[str, dict[str, Any]], str]:
    path = repo_root / MANIFEST_SOURCE
    manifest = _read_json(path)
    if not isinstance(manifest, dict):
        raise ImportErrorStrict("region manifest must be an object")
    if manifest.get("schema") != "johto-region-manifest-v1":
        raise ImportErrorStrict("unsupported region manifest schema")
    provenance = manifest.get("provenance")
    if not isinstance(provenance, dict):
        raise ImportErrorStrict("region manifest provenance is missing")
    if (
        provenance.get("repository") != DONOR_REPOSITORY
        or provenance.get("donor_revision") != DONOR_REVISION
        or provenance.get("donor_tree") != DONOR_TREE
    ):
        raise ImportErrorStrict("region manifest donor pin drift")
    maps = manifest.get("maps")
    if not isinstance(maps, list) or len(maps) != EXPECTED_APPROVED_MAPS:
        raise ImportErrorStrict(
            f"approved map manifest has {len(maps) if isinstance(maps, list) else 'invalid'} "
            f"entries, expected {EXPECTED_APPROVED_MAPS}"
        )
    by_map: dict[str, dict[str, Any]] = {}
    for ordinal, entry in enumerate(maps):
        if not isinstance(entry, dict):
            raise ImportErrorStrict(f"manifest map {ordinal} is not an object")
        if entry.get("ordinal") != ordinal:
            raise ImportErrorStrict("manifest map ordinals are not contiguous")
        source_map = entry.get("source_map")
        source_name = entry.get("source_name")
        source_group = entry.get("source_group")
        source_index = entry.get("source_index")
        host = entry.get("proposed_host")
        proposed = entry.get("proposed_map")
        if (
            not isinstance(source_map, str)
            or not re.fullmatch(r"MAP_[A-Z0-9_]+", source_map)
            or not isinstance(source_name, str)
            or not source_name
            or not isinstance(source_group, str)
            or not source_group
            or not isinstance(source_index, int)
            or isinstance(source_index, bool)
            or not isinstance(host, dict)
            or not isinstance(host.get("group"), int)
            or not isinstance(host.get("index"), int)
            or not isinstance(proposed, dict)
            or not isinstance(proposed.get("map_id"), str)
            or not re.fullmatch(r"MAP_[A-Z0-9_]+", proposed.get("map_id"))
            or proposed.get("group") != host["group"]
            or proposed.get("index") != host["index"]
        ):
            raise ImportErrorStrict(f"malformed manifest linkage for {source_map!r}")
        if source_map in by_map:
            raise ImportErrorStrict(f"duplicate approved map {source_map}")
        by_map[source_map] = entry
    return by_map, _sha256(path)


def _species_symbols(repo_root: Path) -> set[str]:
    text = (repo_root / SPECIES_SOURCE).read_text(encoding="utf-8")
    # Match declarations, rather than generated includes or arbitrary comments.
    symbols = set(re.findall(r"^\s*(SPECIES_[A-Z0-9_]+)\s*(?:=|,)", text, re.MULTILINE))
    if not symbols:
        raise ImportErrorStrict("host species declarations are missing")
    return symbols


def _validate_group_fields(fields: Any) -> list[dict[str, Any]]:
    if not isinstance(fields, list):
        raise ImportErrorStrict("gWildMonHeaders fields must be a list")
    if [field.get("type") for field in fields if isinstance(field, dict)] != list(FIELD_ORDER):
        raise ImportErrorStrict("gWildMonHeaders field order or types drifted")
    if len(fields) != len(FIELD_ORDER):
        raise ImportErrorStrict("gWildMonHeaders has an unexpected field count")
    result: list[dict[str, Any]] = []
    expected_rates = {
        "land_mons": [20, 20, 10, 10, 10, 10, 5, 5, 4, 4, 1, 1],
        "water_mons": [60, 30, 5, 4, 1],
        "rock_smash_mons": [60, 30, 5, 4, 1],
        "fishing_mons": [70, 30, 60, 20, 20, 40, 40, 15, 4, 1],
    }
    for field in fields:
        if not isinstance(field, dict):
            raise ImportErrorStrict("wild encounter field is not an object")
        field_type = field.get("type")
        expected_keys = {"type", "encounter_rates"}
        if field_type == "fishing_mons":
            expected_keys.add("groups")
        if set(field) != expected_keys:
            raise ImportErrorStrict(f"unsupported {field_type!r} field shape")
        rates = field.get("encounter_rates")
        if rates != expected_rates[field_type]:
            raise ImportErrorStrict(f"unexpected encounter rates for {field_type}")
        if not all(isinstance(rate, int) and not isinstance(rate, bool) and 0 <= rate <= 100 for rate in rates):
            raise ImportErrorStrict(f"invalid encounter rate table for {field_type}")
        if field_type == "fishing_mons":
            groups = field.get("groups")
            expected_groups = {
                "old_rod": [0, 1],
                "good_rod": [2, 3, 4],
                "super_rod": [5, 6, 7, 8, 9],
            }
            if groups != expected_groups:
                raise ImportErrorStrict("fishing rod group shape drifted")
        result.append(copy.deepcopy(field))
    return result


def _normalize_label(label: str) -> tuple[str, str]:
    if not isinstance(label, str) or not SOURCE_LABEL_RE.fullmatch(label):
        raise ImportErrorStrict(f"invalid wild encounter label {label!r}")
    if label.endswith("_Night"):
        return label, "night"
    if label.endswith("Night"):
        return label[:-5] + "_Night", "night"
    return label, "day"


def _validate_mons(value: Any, field_type: str, species: set[str]) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != {"encounter_rate", "mons"}:
        raise ImportErrorStrict(f"malformed {field_type} encounter field")
    rate = value["encounter_rate"]
    mons = value["mons"]
    if not isinstance(rate, int) or isinstance(rate, bool) or not 0 <= rate <= 100:
        raise ImportErrorStrict(f"invalid {field_type} encounter rate")
    if not isinstance(mons, list) or len(mons) not in FIELD_SLOT_LENGTHS[field_type]:
        raise ImportErrorStrict(f"invalid {field_type} slot length")
    converted: list[dict[str, Any]] = []
    for slot, mon in enumerate(mons):
        if not isinstance(mon, dict) or set(mon) != {"min_level", "max_level", "species"}:
            raise ImportErrorStrict(f"malformed {field_type} slot {slot}")
        minimum = mon["min_level"]
        maximum = mon["max_level"]
        symbol = mon["species"]
        if (
            not isinstance(minimum, int)
            or isinstance(minimum, bool)
            or not isinstance(maximum, int)
            or isinstance(maximum, bool)
            or not 1 <= minimum <= maximum <= 100
            or not isinstance(symbol, str)
            or not SPECIES_RE.fullmatch(symbol)
            or symbol not in species
        ):
            raise ImportErrorStrict(f"invalid {field_type} slot {slot}")
        converted.append({
            "min_level": minimum,
            "max_level": maximum,
            "species": symbol,
        })
    return {"encounter_rate": rate, "mons": converted}


def _payload(entry: dict[str, Any]) -> dict[str, Any]:
    return {key: copy.deepcopy(value) for key, value in entry.items() if key != "base_label"}


def _duplicate_is_allowed(map_name: str, labels: list[str], time: str) -> bool:
    if map_name != "MAP_MT_SILVER_SNOW":
        return False
    expected = (
        ["gMtSilver_Snow", "gMtSilver_SnowUnused"]
        if time == "day"
        else ["gMtSilver_SnowNight", "gMtSilver_SnowUnused_Night"]
    )
    return sorted(labels) == sorted(expected)


def _parse_selected(
    donor_root: Path,
    repo_root: Path,
    approved_maps: dict[str, dict[str, Any]],
    species: set[str],
) -> tuple[list[dict[str, Any]], list[dict[str, Any]], int]:
    source_path = donor_root / DONOR_SOURCE
    source = _read_json(source_path)
    if not isinstance(source, dict) or set(source) != {"wild_encounter_groups"}:
        raise ImportErrorStrict("donor wild encounter JSON has an unsupported top-level shape")
    groups = source["wild_encounter_groups"]
    if not isinstance(groups, list):
        raise ImportErrorStrict("donor wild encounter groups must be a list")
    map_groups = [group for group in groups if isinstance(group, dict) and group.get("label") == "gWildMonHeaders"]
    if len(map_groups) != 1:
        raise ImportErrorStrict("donor must contain exactly one gWildMonHeaders group")
    group = map_groups[0]
    if set(group) != {"label", "for_maps", "fields", "encounters"} or group.get("for_maps") is not True:
        raise ImportErrorStrict("malformed gWildMonHeaders group")
    fields = _validate_group_fields(group["fields"])
    encounters = group["encounters"]
    if not isinstance(encounters, list):
        raise ImportErrorStrict("donor wild encounters must be a list")

    selected: list[dict[str, Any]] = []
    source_labels_seen: set[str] = set()
    by_key: dict[tuple[str, str], list[dict[str, Any]]] = {}
    for entry in encounters:
        if not isinstance(entry, dict) or entry.get("map") not in approved_maps:
            continue
        if set(entry) - ({"map", "base_label"} | FIELD_KEYS):
            raise ImportErrorStrict(f"unsupported fields in {entry.get('base_label', '<unknown>')}")
        if set(entry) & {"map", "base_label"} != {"map", "base_label"}:
            raise ImportErrorStrict("wild encounter entry must have map and base_label")
        map_name = entry["map"]
        label = entry["base_label"]
        canonical, time = _normalize_label(label)
        if map_name == "MAP_MT_SILVER_SNOW":
            canonical = {
                "gMtSilver_SnowUnused": "gMtSilver_Snow",
                "gMtSilver_SnowUnused_Night": "gMtSilver_Snow_Night",
                "gMtSilver_SnowNight": "gMtSilver_Snow_Night",
            }.get(canonical, canonical)
        if label in source_labels_seen:
            raise ImportErrorStrict(f"duplicate source label {label}")
        source_labels_seen.add(label)
        converted: dict[str, Any] = {"map": map_name, "base_label": label}
        for field_type in FIELD_ORDER:
            if field_type in entry:
                converted[field_type] = _validate_mons(entry[field_type], field_type, species)
        key = (map_name, time)
        by_key.setdefault(key, []).append({
            "source_label": label,
            "canonical": canonical,
            "time": time,
            "payload": converted,
        })

    if len(source_labels_seen) != EXPECTED_SOURCE_HEADERS:
        raise ImportErrorStrict(
            f"selected donor headers total {len(source_labels_seen)}, expected {EXPECTED_SOURCE_HEADERS}"
        )
    map_count = len({map_name for map_name, _time in by_key})
    if map_count != EXPECTED_MAPS_WITH_ENCOUNTERS:
        raise ImportErrorStrict(
            f"selected encounter maps total {map_count}, expected {EXPECTED_MAPS_WITH_ENCOUNTERS}"
        )

    collapsed: list[dict[str, Any]] = []
    duplicate_records: list[dict[str, Any]] = []
    for (map_name, time), entries_for_key in by_key.items():
        canonical = entries_for_key[0]["canonical"]
        labels = [item["source_label"] for item in entries_for_key]
        if len(entries_for_key) > 1:
            if not _duplicate_is_allowed(map_name, labels, time):
                raise ImportErrorStrict(
                    f"duplicate source-time table for {map_name} {canonical}: "
                    f"{[item['source_label'] for item in entries_for_key]}"
                )
            payloads = [_payload(item["payload"]) for item in entries_for_key]
            if any(payload != payloads[0] for payload in payloads[1:]):
                raise ImportErrorStrict(f"conflicting verified Snow duplicate for {map_name} {canonical}")
            duplicate_records.append({
                "map": map_name,
                "time_of_day": entries_for_key[0]["time"],
                "canonical_label": canonical,
                "source_labels": sorted(item["source_label"] for item in entries_for_key),
            })
        first = entries_for_key[0]
        source_labels = sorted(item["source_label"] for item in entries_for_key)
        payload = copy.deepcopy(first["payload"])
        payload["base_label"] = canonical
        payload["time_of_day"] = first["time"]
        payload["source_labels"] = source_labels
        manifest_entry = approved_maps[map_name]
        payload["map_linkage"] = {
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
        collapsed.append(payload)

    if len(collapsed) != EXPECTED_EMITTED_TABLES:
        raise ImportErrorStrict(
            f"selected source-time tables total {len(collapsed)}, expected {EXPECTED_EMITTED_TABLES}"
        )
    order = {name: entry["ordinal"] for name, entry in approved_maps.items()}
    collapsed.sort(key=lambda entry: (order[entry["map"]], entry["time_of_day"], entry["base_label"]))
    return collapsed, fields, len(source_labels_seen)


def _fallback_metadata(
    encounters: list[dict[str, Any]], approved_maps: dict[str, dict[str, Any]]
) -> dict[str, Any]:
    counts = {map_name: 0 for map_name in approved_maps}
    labels: dict[str, list[str]] = {map_name: [] for map_name in approved_maps}
    for entry in encounters:
        counts[entry["map"]] += 1
        labels[entry["map"]].extend(entry["source_labels"])
    single = [
        {
            "map": map_name,
            "source_labels": sorted(labels[map_name]),
            "canonical_day_label": next(
                encounter["base_label"]
                for encounter in encounters
                if encounter["map"] == map_name and encounter["time_of_day"] == "day"
            ),
        }
        for map_name in approved_maps
        if counts[map_name] == 1
    ]
    if len(single) != EXPECTED_SINGLE_HEADER_MAPS:
        raise ImportErrorStrict(
            f"single-header map count {len(single)}, expected {EXPECTED_SINGLE_HEADER_MAPS}"
        )
    if any(item["source_labels"] and item["source_labels"][0].endswith("_Night") for item in single):
        raise ImportErrorStrict("single-header night data cannot be used as the day fallback")
    return {
        "count": len(single),
        "maps": single,
        "policy": "use the canonical day table until Johto runtime time selection is wired",
    }


def time_of_day_for_hour(hour: int) -> str:
    """Return the closed Johto source-time label for a local clock hour."""

    if not isinstance(hour, int) or isinstance(hour, bool) or not 0 <= hour <= 23:
        raise ValueError("hour must be an integer from 0 through 23")
    return "day" if 6 <= hour < 18 else "night"


def build_fragment(donor_root: Path, repo_root: Path = ROOT) -> dict[str, Any]:
    donor_root = donor_root.resolve()
    repo_root = repo_root.resolve()
    _verify_pinned_donor(donor_root)
    approved_maps, manifest_hash = _load_manifest(repo_root)
    species = _species_symbols(repo_root)
    encounters, fields, source_header_count = _parse_selected(
        donor_root, repo_root, approved_maps, species
    )
    source_hash = _sha256(donor_root / DONOR_SOURCE)
    by_map = {map_name: entry for map_name, entry in approved_maps.items()}
    duplicate_collapses = [
        {
            "map": entry["map"],
            "time_of_day": entry["time_of_day"],
            "canonical_label": entry["canonical_label"],
            "source_labels": entry["source_labels"],
            "rule": "verified_identical_payload_ignoring_source_label",
        }
        for entry in _duplicate_records(encounters)
    ]
    return {
        "schema_version": 1,
        "schema": "johto-wild-encounters-v1",
        "region": "johto",
        "runtime_ready": False,
        "provenance": {
            "repository": DONOR_REPOSITORY,
            "donor_revision": DONOR_REVISION,
            "donor_tree": DONOR_TREE,
            "donor_source_path": DONOR_SOURCE,
            "donor_source_sha256": source_hash,
            "region_manifest_path": MANIFEST_SOURCE,
            "region_manifest_sha256": manifest_hash,
        },
        "selection": {
            "approved_map_count": len(approved_maps),
            "maps_with_encounters": len({entry["map"] for entry in encounters}),
            "source_header_count": source_header_count,
            "emitted_table_count": len(encounters),
            "duplicate_collapses": duplicate_collapses,
        },
        "time_windows": {
            "day": {"start": "06:00", "end": "17:59"},
            "night": {"start": "18:00", "end": "05:59"},
            "selection_helper": "time_of_day_for_hour",
            "existing_region_default_selection": "unchanged",
        },
        "absent_night_fallback": _fallback_metadata(encounters, by_map),
        "wild_encounter_groups": [
            {
                "label": JOHTO_WILD_GROUP,
                "for_maps": True,
                "fields": fields,
                "encounters": encounters,
            }
        ],
    }


def _duplicate_records(encounters: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Recover the sole allowed duplicate collapse from source-label metadata."""

    records: list[dict[str, Any]] = []
    for entry in encounters:
        if len(entry["source_labels"]) > 1:
            records.append({
                "map": entry["map"],
                "time_of_day": entry["time_of_day"],
                "canonical_label": entry["base_label"],
                "source_labels": entry["source_labels"],
            })
    return records


def _render(fragment: dict[str, Any]) -> str:
    return json.dumps(fragment, indent=2, ensure_ascii=False) + "\n"


def run(
    donor_root: Path,
    repo_root: Path = ROOT,
    output: Path | None = None,
    check: bool = False,
) -> int:
    fragment = build_fragment(donor_root, repo_root)
    output_path = (output or (repo_root / OUTPUT_SOURCE)).resolve()
    rendered = _render(fragment)
    if check:
        if not output_path.is_file():
            raise ImportErrorStrict(f"generated file is missing: {output_path}")
        try:
            current = output_path.read_text(encoding="utf-8")
        except OSError as exc:
            raise ImportErrorStrict(f"cannot read generated file {output_path}: {exc}") from exc
        if current != rendered:
            raise ImportErrorStrict(f"generated file drift: {output_path}")
        return 0
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(rendered, encoding="utf-8", newline="\n")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--donor-root", type=Path, required=True)
    parser.add_argument("--repo-root", type=Path, default=ROOT)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args(argv)
    try:
        return run(
            args.donor_root,
            args.repo_root,
            args.output,
            args.check,
        )
    except (OSError, ImportErrorStrict, json.JSONDecodeError, ValueError) as exc:
        print(f"johto wild import: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
