"""Validate and report the sealed 407-map Johto/Kanto-Later world plan.

This is deliberately a read-only planning gate.  Production map registration is
blocked until the script compiler, external-edge adapters, and dynamic General
Safari scenery are complete.
"""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import subprocess
import sys
from collections import Counter
from pathlib import Path, PurePosixPath
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_DONOR = Path("C:/Users/Mayor/Documents/Caribbean/johto-hns")
MANIFEST_PATH = Path("data/johto/region_manifest.json")
SCENERY_PATH = Path("data/johto/scenery_registration.json")
CONTENT_PATH = Path("data/johto/content_symbols.json")
ASSET_MANIFEST_PATH = Path("data/johto/asset_manifest.json")
DONOR_REVISION = "751823abaf677020bcd72c45fe3e7cb2b8a576e4"
DONOR_TREE = "33661709e5368edc01c37ed9bb5e0a7a0cb192c8"
EXPECTED_REGION_MANIFEST_SHA256 = "cdbcbca025c9635dea5f5e02da19290817c3ead298ab6dd73d7b16d7aaa70e3e"
EXPECTED_ASSET_MANIFEST_SHA256 = "c85cb17e2fa47106e89ff71583fca385f3288ac9e5bacd2247e043f217a1c5ec"
# This value is recorded inside the accepted asset ledger.  It identifies the
# original region-manifest artifact; the complete live document is sealed by
# EXPECTED_REGION_MANIFEST_SHA256 after canonical JSON serialization below.
ASSET_RECORDED_REGION_MANIFEST_SHA256 = "b6e86075e617caece5405a9cfbeae0645361ba66ce543b93aa2c161db7c6ddc6"
EXPECTED_GROUP_COUNTS = {75: 128, 76: 111, 77: 128, 78: 40}
EXPECTED_EVENT_TOTALS = {
    "object_events": 3357,
    "warp_events": 1183,
    "coord_events": 407,
    "bg_events": 760,
}
EVENT_FIELDS = tuple(EXPECTED_EVENT_TOTALS)
PENDING_GENERAL_LAYOUTS = (
    "LAYOUT_FUCHSIA_CITY_SAFARI_ZONE_BEACH",
    "LAYOUT_FUCHSIA_CITY_SAFARI_ZONE_BRUSH",
    "LAYOUT_FUCHSIA_CITY_SAFARI_ZONE_MOUNTAIN",
)
PENDING_GENERAL_MAPS = (
    "MAP_KANTO_LATER_FUCHSIA_CITY_SAFARI_ZONE_BEACH",
    "MAP_KANTO_LATER_FUCHSIA_CITY_SAFARI_ZONE_BRUSH",
    "MAP_KANTO_LATER_FUCHSIA_CITY_SAFARI_ZONE_MOUNTAIN",
)
EXPECTED_EXTERNAL_EDGES = (
    ("MAP_NEW_BARK_TOWN", "warp", 4, "MAP_WORLD_HUB", "excluded_debug_edge"),
    ("MAP_NEW_BARK_TOWN", "warp", 7, "MAP_WORLD_HUB", "excluded_debug_edge"),
    ("MAP_ECRUTEAK_CITY", "warp", 14, "MAP_BATTLE_FRONTIER_BATTLE_TOWER_LOBBY", "required_host_adapter"),
    ("MAP_ROUTE40", "connection", 2, "MAP_TRAINER_HILL_COURTYARD", "excluded_debug_edge"),
    ("MAP_ROUTE40", "warp", 9, "MAP_GATE_ROUTE40_TRAINER_HILL_COURTYARD", "excluded_debug_edge"),
    ("MAP_ROUTE40", "warp", 10, "MAP_GATE_ROUTE40_TRAINER_HILL_COURTYARD", "excluded_debug_edge"),
    ("MAP_ROUTE40", "warp", 11, "MAP_GATE_ROUTE40_TRAINER_HILL_COURTYARD", "excluded_debug_edge"),
    ("MAP_ROUTE40", "warp", 12, "MAP_GATE_ROUTE40_TRAINER_HILL_COURTYARD", "excluded_debug_edge"),
    ("MAP_GOLDENROD_CITY_DEPARTMENT_STORE_ELEVATOR", "warp", 0, "MAP_DYNAMIC", "required_host_adapter"),
    ("MAP_VIRIDIAN_CITY", "warp", 2, "MAP_LILYCOVE_CITY_CONTEST_LOBBY", "pending_runtime_policy"),
    ("MAP_VERMILION_CITY", "connection", 3, "MAP_TREES", "pending_runtime_policy"),
    ("MAP_FUCHSIA_CITY", "warp", 2, "MAP_NEW_BARK_TOWN", "pending_era_boundary"),
    ("MAP_CINNABAR_ISLAND", "warp", 0, "MAP_NEW_BARK_TOWN", "pending_era_boundary"),
)


class WorldPlanError(ValueError):
    """The sealed world plan or its source evidence is inconsistent."""


def _reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise WorldPlanError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def _parse_json_document(raw: bytes | str, label: str) -> Any:
    try:
        return json.loads(raw, object_pairs_hook=_reject_duplicate_keys)
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise WorldPlanError(f"{label} is not valid JSON: {exc}") from exc


def _canonical_json_bytes(value: Any) -> bytes:
    return json.dumps(
        value,
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=True,
    ).encode("utf-8")


def _canonical_json_sha256(value: Any) -> str:
    return _sha256(_canonical_json_bytes(value))


def _load(path: Path) -> Any:
    try:
        return _parse_json_document(path.read_bytes(), str(path))
    except OSError as exc:
        raise WorldPlanError(f"cannot read {path}: {exc}") from exc


def _sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def _donor_evidence_sha256(raw: bytes) -> str:
    """Match the sealed hashes made from the donor's CRLF checkout."""
    checkout_bytes = raw.replace(b"\r\n", b"\n").replace(b"\n", b"\r\n")
    return _sha256(checkout_bytes)


def _git(donor: Path, revision: str) -> str:
    try:
        return subprocess.run(
            ["git", "-C", str(donor), "rev-parse", revision],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError) as exc:
        raise WorldPlanError(f"cannot authenticate donor repository: {exc}") from exc


def _verify_donor(donor: Path) -> None:
    if not donor.is_dir():
        raise WorldPlanError(f"donor root does not exist: {donor}")
    actual = (_git(donor, "HEAD"), _git(donor, "HEAD^{tree}"))
    if actual != (DONOR_REVISION, DONOR_TREE):
        raise WorldPlanError(f"donor pin mismatch: {actual[0]}/{actual[1]}")


def _donor_object_name(relative: str) -> str:
    if not isinstance(relative, str):
        raise WorldPlanError("donor object path must be a string")
    posix = PurePosixPath(relative)
    if (
        not relative
        or posix.is_absolute()
        or posix.as_posix() != relative
        or any(part in {"", ".", ".."} for part in posix.parts)
        or any(ord(character) <= 0x1F or ord(character) == 0x7F for character in relative)
        or "\\" in relative
        or ":" in relative
    ):
        raise WorldPlanError(f"unsafe donor object path: {relative!r}")
    return f"{DONOR_REVISION}:{relative}"


def _git_blobs(donor: Path, relatives: list[str]) -> dict[str, bytes]:
    unique = list(dict.fromkeys(relatives))
    object_names = [_donor_object_name(relative) for relative in unique]
    try:
        result = subprocess.run(
            ["git", "-C", str(donor), "cat-file", "--batch"],
            check=True,
            input=("\n".join(object_names) + "\n").encode(),
            capture_output=True,
        )
    except (OSError, subprocess.CalledProcessError) as exc:
        raise WorldPlanError(f"cannot read pinned donor blobs: {exc}") from exc
    stream = io.BytesIO(result.stdout)
    blobs: dict[str, bytes] = {}
    for relative in unique:
        header = stream.readline().decode("utf-8", errors="replace").rstrip("\n")
        fields = header.split()
        if len(fields) == 2 and fields[1] == "missing":
            raise WorldPlanError(f"pinned donor object does not exist: {relative}")
        if len(fields) != 3 or not fields[2].isdigit():
            raise WorldPlanError(f"invalid git object response for donor path: {relative}")
        if fields[1] != "blob":
            raise WorldPlanError(f"donor object is not a blob: {relative}")
        size = int(fields[2])
        raw = stream.read(size)
        if len(raw) != size or stream.read(1) != b"\n":
            raise WorldPlanError(f"truncated git blob response for donor path: {relative}")
        blobs[relative] = raw
    if stream.read():
        raise WorldPlanError("unexpected trailing data from git cat-file")
    return blobs


def _git_blob(donor: Path, relative: str) -> bytes:
    return _git_blobs(donor, [relative])[relative]


def _git_json(raw: bytes, relative: str) -> dict[str, Any]:
    try:
        value = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise WorldPlanError(f"pinned donor blob is not JSON: {relative}: {exc}") from exc
    if not isinstance(value, dict):
        raise WorldPlanError(f"pinned donor JSON must be an object: {relative}")
    return value


def _strict_int(value: Any, label: str, minimum: int = 0, maximum: int = 255) -> int:
    if type(value) is not int or not minimum <= value <= maximum:
        raise WorldPlanError(f"{label} must be an integer in {minimum}..{maximum}")
    return value


def _validate_event_arrays(source: dict[str, Any], source_name: str) -> dict[str, int]:
    counts: dict[str, int] = {}
    for field in EVENT_FIELDS:
        events = source.get(field)
        if not isinstance(events, list):
            raise WorldPlanError(f"{source_name}.{field} must be an array")
        counts[field] = _strict_int(len(events), f"{source_name}.{field} count")
    return counts


def _validate_source_inventory(maps: list[dict[str, Any]]) -> None:
    source_maps: set[str] = set()
    source_names: set[str] = set()
    for ordinal, record in enumerate(maps):
        source_map = record.get("source_map")
        source_name = record.get("source_name")
        if not isinstance(source_map, str) or not isinstance(source_name, str):
            raise WorldPlanError(f"malformed source identity at ordinal {ordinal}")
        if source_map in source_maps:
            raise WorldPlanError(f"duplicate source_map identity at ordinal {ordinal}: {source_map}")
        if source_name in source_names:
            raise WorldPlanError(f"duplicate source_name identity at ordinal {ordinal}: {source_name}")
        source_maps.add(source_map)
        source_names.add(source_name)


def _edge_identity(edge: dict[str, Any]) -> tuple[str, str, int, str, str]:
    kind = edge.get("kind")
    position = edge.get("index") if kind == "warp" else edge.get("edge_index")
    return (
        edge.get("source_map"),
        kind,
        position,
        edge.get("target"),
        edge.get("classification"),
    )


def _validate_external_source(edge: dict[str, Any], source: dict[str, Any]) -> None:
    kind = edge["kind"]
    field = "warp_events" if kind == "warp" else "connections"
    index_key = "index" if kind == "warp" else "edge_index"
    index = _strict_int(edge.get(index_key), f"external edge {index_key}", 0, 65535)
    records = source.get(field)
    if not isinstance(records, list) or index >= len(records):
        raise WorldPlanError(f"external edge has no donor {field}[{index}]")
    record = records[index]
    target_key = "dest_map" if kind == "warp" else "map"
    if not isinstance(record, dict) or record.get(target_key) != edge.get("target"):
        raise WorldPlanError(f"external edge target differs from donor {field}[{index}]")
    if kind == "warp" and str(record.get("dest_warp_id")) != str(edge.get("warp_id")):
        raise WorldPlanError(f"external warp id differs from donor {field}[{index}]")
    if kind == "connection" and (
        record.get("direction") != edge.get("direction")
        or record.get("offset") != edge.get("offset")
    ):
        raise WorldPlanError(f"external connection geometry differs from donor {field}[{index}]")


def _validate_companion_ledgers(
    root: Path, donor: Path, maps: list[dict[str, Any]]
) -> tuple[dict[str, Any], dict[str, Any], dict[str, bytes]]:
    scenery = _load(root / SCENERY_PATH)
    content = _load(root / CONTENT_PATH)
    if (
        scenery.get("provenance", {}).get("region_manifest_sha256")
        != ASSET_RECORDED_REGION_MANIFEST_SHA256
    ):
        raise WorldPlanError("scenery ledger recorded region-manifest digest drift")
    if content.get("provenance", {}).get("manifest_sha256") != ASSET_RECORDED_REGION_MANIFEST_SHA256:
        raise WorldPlanError("content-symbol ledger recorded region-manifest digest drift")
    layouts = scenery.get("layouts")
    if not isinstance(layouts, list) or len(layouts) != 407:
        raise WorldPlanError("scenery ledger must contain exactly 407 layouts")
    for record, scenery_record in zip(maps, layouts, strict=True):
        if not isinstance(scenery_record, dict):
            raise WorldPlanError("malformed scenery layout record")
        identity = record["identity_namespace"]
        expected = (identity["map"], identity["layout"], identity["script"])
        actual_namespace = scenery_record.get("identity_namespace")
        identity_matches = (
            scenery_record.get("source_map") == record["source_map"]
            and scenery_record.get("target_layout_id") == identity["layout"]
            and (
                record["ordinal"] < 239
                or isinstance(actual_namespace, dict)
                and (
                    actual_namespace.get("map"),
                    actual_namespace.get("layout"),
                    actual_namespace.get("script"),
                ) == expected
            )
        )
        if not identity_matches:
            raise WorldPlanError(f"scenery identity mismatch at ordinal {record['ordinal']}")
    completeness = content.get("completeness", {})
    if (
        completeness.get("selected_map_count"),
        completeness.get("original_selected_map_count"),
        completeness.get("later_selected_map_count"),
    ) != (407, 239, 168):
        raise WorldPlanError("content-symbol ledger selection differs from world plan")
    files = content.get("source_files")
    if not isinstance(files, list) or len(files) != 814:
        raise WorldPlanError("content-symbol ledger must bind 814 selected sources")
    if content.get("provenance", {}).get("selected_source_count") != 814:
        raise WorldPlanError("content-symbol source count provenance drifted")
    source_paths: list[str] = []
    for ordinal, record in enumerate(maps):
        pair = files[ordinal * 2 : ordinal * 2 + 2]
        expected_pair = (
            (record["source_map"], record["source_name"], "map_json", record["source_sha256"]["map_json"]),
            (record["source_map"], record["source_name"], "script", record["source_sha256"]["script"]),
        )
        actual_pair = tuple(
            (item.get("map"), item.get("map_name"), item.get("kind"), item.get("sha256"))
            for item in pair
            if isinstance(item, dict)
        )
        if actual_pair != expected_pair:
            raise WorldPlanError(f"content source identity mismatch at ordinal {ordinal}")
        for item in pair:
            relative = item.get("path")
            expected_prefix = f"data/maps/{record['source_name']}/"
            if not isinstance(relative, str) or not relative.startswith(expected_prefix) or ".." in Path(relative).parts:
                raise WorldPlanError(f"unsafe content source path at ordinal {ordinal}")
            source_paths.append(relative)
    donor_blobs = _git_blobs(donor, source_paths)
    for ordinal, record in enumerate(maps):
        for item in files[ordinal * 2 : ordinal * 2 + 2]:
            relative = item["path"]
            raw = donor_blobs[relative]
            if _donor_evidence_sha256(raw) != item["sha256"]:
                raise WorldPlanError(f"content source hash drift at ordinal {ordinal}: {relative}")
    pending = scenery.get("runtime_readiness", {}).get("general_pending_layouts")
    if tuple(pending or ()) != PENDING_GENERAL_LAYOUTS:
        raise WorldPlanError("the three dynamic General Safari layouts must remain pending")
    if scenery.get("runtime_readiness", {}).get("ready") is not False:
        raise WorldPlanError("scenery ledger prematurely claims runtime readiness")
    return scenery, content, donor_blobs


def _validate_asset_manifest_anchor(root: Path, manifest_raw: bytes, maps: list[dict[str, Any]]) -> None:
    asset_path = root / ASSET_MANIFEST_PATH
    try:
        asset_raw = asset_path.read_bytes()
    except OSError as exc:
        raise WorldPlanError(f"cannot read {asset_path}: {exc}") from exc
    asset_manifest = _parse_json_document(asset_raw, "sealed asset manifest")
    if not isinstance(asset_manifest, dict):
        raise WorldPlanError("sealed asset manifest must be an object")
    if _canonical_json_sha256(asset_manifest) != EXPECTED_ASSET_MANIFEST_SHA256:
        raise WorldPlanError("asset manifest semantics differ from the sealed digest")
    provenance = asset_manifest.get("provenance", {})
    manifest = _parse_json_document(manifest_raw, "sealed region manifest")
    if (
        provenance.get("donor_revision") != DONOR_REVISION
        or provenance.get("donor_tree") != DONOR_TREE
        or provenance.get("region_manifest_path") != MANIFEST_PATH.as_posix()
        or provenance.get("region_manifest_sha256") != ASSET_RECORDED_REGION_MANIFEST_SHA256
        or _canonical_json_sha256(manifest) != EXPECTED_REGION_MANIFEST_SHA256
    ):
        raise WorldPlanError("region manifest differs from the sealed asset-manifest anchor")
    layouts = asset_manifest.get("layouts")
    if not isinstance(layouts, list) or len(layouts) != 407:
        raise WorldPlanError("asset manifest must contain exactly 407 ordered layouts")
    for ordinal, (record, layout) in enumerate(zip(maps, layouts, strict=True)):
        if not isinstance(layout, dict):
            raise WorldPlanError(f"malformed asset layout at ordinal {ordinal}")
        asset_ordinal = _strict_int(layout.get("ordinal"), f"asset layout ordinal at index {ordinal}", 0, 406)
        if asset_ordinal != ordinal:
            raise WorldPlanError("asset layout ordinals must be unique, ordered, and contiguous 0..406")
        if (layout.get("source_map"), layout.get("symbol")) != (
            record["source_map"],
            record["layout"]["symbol"],
        ):
            raise WorldPlanError(f"asset layout source identity mismatch at ordinal {ordinal}")


def build_plan(repo_root: Path = ROOT, donor_root: Path = DEFAULT_DONOR) -> dict[str, Any]:
    root = Path(repo_root).resolve()
    donor = Path(donor_root).resolve()
    _verify_donor(donor)
    manifest_file = root / MANIFEST_PATH
    try:
        manifest_raw = manifest_file.read_bytes()
        manifest = _parse_json_document(manifest_raw, "region manifest")
    except OSError as exc:
        raise WorldPlanError(f"cannot read {manifest_file}: {exc}") from exc
    if not isinstance(manifest, dict):
        raise WorldPlanError("region manifest must be an object")
    if manifest.get("schema") != "johto-region-manifest-v1":
        raise WorldPlanError("unsupported region manifest schema")
    provenance = manifest.get("provenance", {})
    if (provenance.get("donor_revision"), provenance.get("donor_tree")) != (
        DONOR_REVISION,
        DONOR_TREE,
    ):
        raise WorldPlanError("region manifest donor pin drift")
    maps = manifest.get("maps")
    if not isinstance(maps, list) or len(maps) != 407:
        raise WorldPlanError("world plan must contain exactly 407 maps")
    for index, record in enumerate(maps):
        if not isinstance(record, dict):
            raise WorldPlanError(f"malformed map record at ordinal {index}")
        ordinal = _strict_int(record.get("ordinal"), f"map ordinal at index {index}", 0, 406)
        if ordinal != index:
            raise WorldPlanError("map ordinals must be unique, ordered, and contiguous 0..406")
    sections = manifest.get("sections")
    if not isinstance(sections, dict) or (
        sections.get("source_count"),
        sections.get("later_source_count"),
        sections.get("johto_count"),
        sections.get("kanto_count"),
    ) != (99, 43, 41, 43):
        raise WorldPlanError("section inventory counts drifted")
    section_entries = sections.get("entries")
    if not isinstance(section_entries, list) or len(section_entries) != 99:
        raise WorldPlanError("section inventory must contain exactly 99 source identities")
    section_by_source: dict[str, dict[str, Any]] = {}
    section_classes: Counter[str] = Counter()
    for entry in section_entries:
        if not isinstance(entry, dict) or not isinstance(entry.get("source_symbol"), str):
            raise WorldPlanError("malformed section inventory entry")
        if entry["source_symbol"] in section_by_source or not isinstance(entry.get("target_symbol"), str):
            raise WorldPlanError("duplicate or malformed section identity")
        _strict_int(entry.get("target_id"), f"section {entry['source_symbol']} id")
        if not isinstance(entry.get("classification"), str):
            raise WorldPlanError("malformed section classification")
        section_by_source[entry["source_symbol"]] = entry
        section_classes[entry["classification"]] += 1
    if section_classes != {
        "new_johto": 40,
        "johto_alias": 15,
        "preserved_host": 1,
        "required_host_adapter": 1,
        "reviewed_kanto_geography": 42,
    }:
        raise WorldPlanError("section classification totals drifted")
    _validate_source_inventory(maps)
    _, _, donor_blobs = _validate_companion_ledgers(root, donor, maps)

    groups: Counter[int] = Counter()
    selected_warps = 0
    selected_connections = 0
    event_totals: Counter[str] = Counter()
    donor_sources: dict[str, dict[str, Any]] = {}
    map_ids: set[str] = set()
    namespaces: set[tuple[str, str, str]] = set()
    pending_maps: list[str] = []
    regions: Counter[str] = Counter()
    for ordinal, record in enumerate(maps):
        if not isinstance(record, dict):
            raise WorldPlanError(f"malformed map record at ordinal {ordinal}")
        source_map = record.get("source_map")
        source_name = record.get("source_name")
        if not isinstance(source_map, str) or not isinstance(source_name, str):
            raise WorldPlanError(f"malformed source identity at ordinal {ordinal}")
        expected_era = "JOHTO" if ordinal < 239 else "KANTO_LATER"
        expected_campaign = "JOHTO" if ordinal < 239 else "KANTO_LATER"
        if (record.get("era"), record.get("campaign"), record.get("world_era")) != (
            expected_era,
            expected_campaign,
            expected_era,
        ):
            raise WorldPlanError(f"wrong era identity at ordinal {ordinal}")
        region = record.get("region")
        geographic = record.get("geographic_region")
        if region not in {"REGION_JOHTO", "REGION_KANTO"} or geographic != region:
            raise WorldPlanError(f"invalid geographic region at ordinal {ordinal}")
        regions[region] += 1
        section = record.get("resolved_section")
        if not isinstance(record.get("source_section"), str) or not isinstance(section, dict):
            raise WorldPlanError(f"invalid section at ordinal {ordinal}")
        if not isinstance(section.get("symbol"), str):
            raise WorldPlanError(f"invalid resolved section symbol at ordinal {ordinal}")
        _strict_int(section.get("id"), f"section id at ordinal {ordinal}")
        section_entry = section_by_source.get(record["source_section"])
        if section_entry is None or (
            section.get("symbol"), section.get("id")
        ) != (section_entry["target_symbol"], section_entry["target_id"]):
            raise WorldPlanError(f"map section resolution drift at ordinal {ordinal}")
        host = record.get("proposed_host")
        if not isinstance(host, dict):
            raise WorldPlanError(f"missing host allocation at ordinal {ordinal}")
        group = _strict_int(host.get("group"), f"group at ordinal {ordinal}")
        index = _strict_int(host.get("index"), f"group index at ordinal {ordinal}")
        era_ordinal = ordinal if ordinal < 239 else ordinal - 239
        expected_group = (75 if ordinal < 239 else 77) + era_ordinal // 128
        expected_index = era_ordinal % 128
        if (group, index) != (expected_group, expected_index):
            raise WorldPlanError(f"non-contiguous host allocation at ordinal {ordinal}")
        groups[group] += 1
        identity = record.get("identity_namespace")
        if not isinstance(identity, dict):
            raise WorldPlanError(f"missing identity namespace at ordinal {ordinal}")
        source_layout = record["layout"]["symbol"]
        expected_map = source_map if ordinal < 239 else "MAP_KANTO_LATER_" + source_map.removeprefix("MAP_")
        expected_layout = source_layout if ordinal < 239 else "LAYOUT_KANTO_LATER_" + source_layout.removeprefix("LAYOUT_")
        expected_script = ("Johto_" if ordinal < 239 else "KantoLater_") + source_name
        namespace = (identity.get("map"), identity.get("layout"), identity.get("script"))
        original_map_names = {source_map, "MAP_JOHTO_" + source_map.removeprefix("MAP_")}
        original_layout_names = {source_layout, "LAYOUT_JOHTO_" + source_layout.removeprefix("LAYOUT_")}
        identity_valid = (
            namespace[2] == expected_script
            and (
                namespace == (expected_map, expected_layout, expected_script)
                if ordinal >= 239
                else namespace[0] in original_map_names and namespace[1] in original_layout_names
            )
        )
        if not identity_valid:
            raise WorldPlanError(f"wrong identity namespace at ordinal {ordinal}")
        if namespace[0] in map_ids or namespace in namespaces:
            raise WorldPlanError(f"duplicate target identity at ordinal {ordinal}")
        map_ids.add(namespace[0])
        namespaces.add(namespace)

        source_path = f"data/maps/{source_name}/map.json"
        source_raw = donor_blobs[source_path]
        source = _git_json(source_raw, source_path)
        if _donor_evidence_sha256(source_raw) != record.get("source_sha256", {}).get("map_json"):
            raise WorldPlanError(f"donor map hash drift at ordinal {ordinal}")
        if (
            source.get("id"), source.get("name"), source.get("layout"), source.get("region_map_section")
        ) != (source_map, source_name, record["layout"]["symbol"], record["source_section"]):
            raise WorldPlanError(f"donor map identity drift at ordinal {ordinal}")
        donor_sources[source_map] = source
        for field, count in _validate_event_arrays(source, source_name).items():
            event_totals[field] += count
        selected_warps += sum(w.get("classification") == "selected" for w in record.get("warp_targets", []))
        selected_connections += sum(
            edge.get("kind") == "connection" and edge.get("classification") == "selected"
            for edge in record.get("edges", [])
        )
        if record["layout"]["symbol"] in PENDING_GENERAL_LAYOUTS:
            pending_maps.append(identity["map"])

    if dict(groups) != EXPECTED_GROUP_COUNTS:
        raise WorldPlanError(f"map group counts drifted: {dict(groups)}")
    if regions != {"REGION_JOHTO": 229, "REGION_KANTO": 178}:
        raise WorldPlanError(f"geographic region totals drifted: {dict(regions)}")
    if dict(event_totals) != EXPECTED_EVENT_TOTALS:
        raise WorldPlanError(f"event totals drifted: {dict(event_totals)}")
    if (selected_warps, selected_connections) != (1172, 191):
        raise WorldPlanError("selected edge totals must be 1172 warps and 191 connections")

    external = manifest.get("external_edges")
    if not isinstance(external, list) or tuple(_edge_identity(edge) for edge in external) != EXPECTED_EXTERNAL_EDGES:
        raise WorldPlanError("external edge identity or classification drift")
    categories = Counter(edge["classification"] for edge in external)
    if categories != {
        "excluded_debug_edge": 7,
        "required_host_adapter": 2,
        "pending_runtime_policy": 2,
        "pending_era_boundary": 2,
    }:
        raise WorldPlanError("external edge category totals drift")
    for edge in external:
        _validate_external_source(edge, donor_sources[edge["source_map"]])
    if tuple(pending_maps) != PENDING_GENERAL_MAPS:
        raise WorldPlanError("pending General Safari map identity drift")
    _validate_asset_manifest_anchor(root, manifest_raw, maps)
    _verify_donor(donor)

    return {
        "schema": "johto-world-plan-v1",
        "mode": "read-only",
        "selected_map_count": 407,
        "original_map_count": 239,
        "later_map_count": 168,
        "group_counts": {str(key): groups[key] for key in sorted(groups)},
        "event_totals": {key: event_totals[key] for key in EVENT_FIELDS},
        "selected_warp_count": selected_warps,
        "selected_connection_count": selected_connections,
        "external_edge_counts": {
            "excluded_debug": categories["excluded_debug_edge"],
            "host_adapter": categories["required_host_adapter"],
            "runtime_policy": categories["pending_runtime_policy"],
            "era_boundary": categories["pending_era_boundary"],
        },
        "external_edges": [
            {"source_map": item[0], "kind": item[1], "index": item[2], "target": item[3], "classification": item[4]}
            for item in EXPECTED_EXTERNAL_EDGES
        ],
        "pending_general_layouts": list(PENDING_GENERAL_LAYOUTS),
        "pending_general_maps": pending_maps,
        "playable_map_count": 404,
        "production_write_ready": False,
        "blockers": [
            "complete 407-map campaign script compiler",
            "resolve 13 external edges",
            "implement three dynamic General Safari layouts",
            "register production map headers, groups, and events",
        ],
    }


def render(plan: dict[str, Any]) -> str:
    return json.dumps(plan, indent=2, sort_keys=True) + "\n"


def run(
    repo_root: Path = ROOT,
    donor_root: Path = DEFAULT_DONOR,
    *,
    check: bool = False,
    write: bool = False,
) -> int:
    if check == write:
        raise WorldPlanError("select exactly one of --check or --write")
    plan = build_plan(repo_root, donor_root)
    if write:
        raise WorldPlanError(
            "read-only planner refuses production world writes: compiler, external-edge, and dynamic-scenery dependencies are incomplete"
        )
    print(render(plan), end="")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=ROOT)
    parser.add_argument("--donor-root", type=Path, default=DEFAULT_DONOR)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true")
    mode.add_argument("--write", action="store_true")
    args = parser.parse_args(argv)
    try:
        return run(args.repo_root, args.donor_root, check=args.check, write=args.write)
    except WorldPlanError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
