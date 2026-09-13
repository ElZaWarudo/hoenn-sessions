"""Validate and materialize the sealed 407-map Johto/Kanto-Later world plan.

The source ledgers remain donor-authenticated while the generated world plan
contains the reviewed, runtime-safe topology normalization.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import io
import json
import re
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
WORLD_PLAN_PATH = Path("data/johto/world_plan.json")
CAMPAIGN_SCRIPTS_PATH = Path("data/johto/campaign_scripts.inc")
DONOR_REVISION = "751823abaf677020bcd72c45fe3e7cb2b8a576e4"
DONOR_TREE = "33661709e5368edc01c37ed9bb5e0a7a0cb192c8"
EXPECTED_REGION_MANIFEST_SHA256 = "cdbcbca025c9635dea5f5e02da19290817c3ead298ab6dd73d7b16d7aaa70e3e"
EXPECTED_ASSET_MANIFEST_SHA256 = "db91344abd45f1c1cd012d82db356ec35651fa92dfc79d332b72856ce011a72d"
EXPECTED_SCENERY_REGISTRATION_SHA256 = "38fc1957b77ed2230140b31018eeb08b5e081fe202114a5918f6d27128bd1eba"
EXPECTED_CONTENT_SYMBOLS_SHA256 = "06e0290f183f26f5150ea34ae39b2fe2758417c7eb741a756b5cd397bd9a5b70"
# JSON provenance uses the same canonical semantic identity as the live
# document anchors, independent of line endings and object-key order.
ASSET_RECORDED_REGION_MANIFEST_SHA256 = EXPECTED_REGION_MANIFEST_SHA256
CONTENT_RECORDED_REGION_MANIFEST_SHA256 = EXPECTED_REGION_MANIFEST_SHA256
EXPECTED_GROUP_COUNTS = {75: 128, 76: 111, 77: 128, 78: 40}
EXPECTED_EVENT_TOTALS = {
    "object_events": 3357,
    "warp_events": 1183,
    "coord_events": 407,
    "bg_events": 760,
}
EVENT_FIELDS = tuple(EXPECTED_EVENT_TOTALS)
PENDING_GENERAL_LAYOUTS: tuple[str, ...] = ()
PENDING_GENERAL_MAPS: tuple[str, ...] = ()
EXPECTED_SOURCE_EXTERNAL_EDGES = (
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
# The source manifest deliberately remains donor-authenticated.  This is the
# normalized topology consumed by the world plan after removing known debug
# adapters and resolving the reviewed internal transitions.
EXPECTED_EXTERNAL_EDGES = (
    ("MAP_GOLDENROD_CITY_DEPARTMENT_STORE_ELEVATOR", "warp", 0, "MAP_DYNAMIC", "required_host_adapter"),
)
PRESERVED_EXTERNAL_SCRIPT_DESTINATIONS = (
    ("MAP_OLIVINE_CITY_PORT_INSIDE", "Johto_OlivineCity_PortInside_OlivinePort_EventScript_ChoseSouthernIsland", "MAP_SOUTHERN_ISLAND_EXTERIOR", 13, 22),
    ("MAP_OLIVINE_CITY_PORT_INSIDE", "Johto_OlivineCity_PortInside_OlivinePort_EventScript_ChoseBirthIsland", "MAP_BIRTH_ISLAND_EXTERIOR", 13, 23),
    ("MAP_OLIVINE_CITY_PORT_INSIDE", "Johto_OlivineCity_PortInside_OlivinePort_EventScript_ChoseFarawayIsland", "MAP_FARAWAY_ISLAND_ENTRANCE", 13, 38),
    ("MAP_OLIVINE_CITY_PORT_INSIDE", "Johto_OlivineCity_PortInside_OlivinePort_EventScript_ChoseBattleFrontier", "MAP_BATTLE_FRONTIER_OUTSIDE_WEST", 20, 67),
    ("MAP_VERMILION_CITY_PORT_INSIDE", "KantoLater_VermilionCity_PortInside_VermilionPort_EventScript_ChoseSouthernIsland", "MAP_SOUTHERN_ISLAND_EXTERIOR", 13, 22),
    ("MAP_VERMILION_CITY_PORT_INSIDE", "KantoLater_VermilionCity_PortInside_VermilionPort_EventScript_ChoseBirthIsland", "MAP_BIRTH_ISLAND_EXTERIOR", 13, 23),
    ("MAP_VERMILION_CITY_PORT_INSIDE", "KantoLater_VermilionCity_PortInside_VermilionPort_EventScript_ChoseFarawayIsland", "MAP_FARAWAY_ISLAND_ENTRANCE", 13, 38),
    ("MAP_VERMILION_CITY_PORT_INSIDE", "KantoLater_VermilionCity_PortInside_VermilionPort_EventScript_ChoseBattleFrontier", "MAP_BATTLE_FRONTIER_OUTSIDE_WEST", 20, 67),
)

WARP_REMOVALS = {
    "MAP_NEW_BARK_TOWN": frozenset({4, 7}),
    "MAP_ECRUTEAK_CITY": frozenset({14}),
    "MAP_ROUTE40": frozenset({9, 10, 11, 12}),
    "MAP_CINNABAR_ISLAND": frozenset({0}),
}
CONNECTION_REMOVALS = {
    "MAP_ROUTE40": frozenset({2}),
    "MAP_VERMILION_CITY": frozenset({3}),
}
WARP_REWRITES = {
    ("MAP_SSAQUA_1F", 0): {"dest_warp_id": "0"},
    ("MAP_FUCHSIA_ROUTE19GATE", 0): {
        "dest_map": "MAP_FUCHSIA_CITY",
        "dest_warp_id": "2",
        "classification": "selected",
        "target_map_id": "MAP_KANTO_LATER_FUCHSIA_CITY",
    },
    ("MAP_FUCHSIA_ROUTE19GATE", 1): {"dest_warp_id": "0"},
    ("MAP_FUCHSIA_CITY", 2): {
        "dest_map": "MAP_FUCHSIA_ROUTE19GATE",
        "dest_warp_id": "0",
        "classification": "selected",
        "target_map_id": "MAP_KANTO_LATER_FUCHSIA_ROUTE19GATE",
    },
    ("MAP_NEW_BARK_TOWN_LAB", 1): {"dest_warp_id": "5"},
    ("MAP_CINNABAR_ISLAND_POKEMON_CENTER", 0): {"dest_warp_id": "0"},
    ("MAP_VIRIDIAN_CITY", 2): {
        "dest_map": "MAP_VIRIDIAN_CITY_HOUSE2",
        "dest_warp_id": "0",
        "classification": "selected",
        "target_map_id": "MAP_KANTO_LATER_VIRIDIAN_CITY_HOUSE2",
    },
}


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


def _cross_validate_world_representations(
    record: dict[str, Any],
    source: dict[str, Any] | None = None,
    target_map_ids: dict[str, str] | None = None,
) -> None:
    """Require the manifest's three topology views to describe one graph.

    When ``source`` is supplied, the donor map is the fourth view and every
    source-backed field is compared before any reviewed normalization occurs.
    The optional form is used for the generated plan, where the normalized
    topology no longer has to equal the donor byte-for-byte.
    """
    source_map = record.get("source_map", "<unknown>")
    connections = record.get("connections")
    warp_targets = record.get("warp_targets")
    edges = record.get("edges")
    if not isinstance(connections, list) or not isinstance(warp_targets, list) or not isinstance(edges, list):
        raise WorldPlanError(f"{source_map} topology representations must be arrays")
    donor_connections = (source or {}).get("connections") or []
    donor_warps = (source or {}).get("warp_events") or []
    if source is not None and connections != donor_connections:
        raise WorldPlanError(f"{source_map} connections differ from donor topology")
    if source is not None and len(warp_targets) != len(donor_warps):
        raise WorldPlanError(f"{source_map} warp_targets differ from donor warp count")
    if source is not None:
        for index, (target, donor_warp) in enumerate(zip(warp_targets, donor_warps, strict=True)):
            if not isinstance(target, dict) or not isinstance(donor_warp, dict):
                raise WorldPlanError(f"{source_map} warp representation is malformed at index {index}")
            for field in ("x", "y", "elevation", "dest_map", "dest_warp_id"):
                if target.get(field) != donor_warp.get(field):
                    raise WorldPlanError(f"{source_map} warp_targets[{index}] differs from donor {field}")
    for index, target in enumerate(warp_targets):
        if not isinstance(target, dict) or target.get("index") != index:
            raise WorldPlanError(f"{source_map} warp_targets indices are not contiguous")
    for index, connection in enumerate(connections):
        if not isinstance(connection, dict):
            raise WorldPlanError(f"{source_map} connection representation is malformed at index {index}")
    if len(edges) != len(warp_targets) + len(connections):
        raise WorldPlanError(f"{source_map} edges do not cover warp_targets and connections exactly")
    warp_edges: dict[int, dict[str, Any]] = {}
    connection_edges: dict[int, dict[str, Any]] = {}
    for edge in edges:
        if not isinstance(edge, dict) or edge.get("kind") not in {"warp", "connection"}:
            raise WorldPlanError(f"{source_map} has malformed topology edge")
        kind = edge["kind"]
        key = edge.get("index") if kind == "warp" else edge.get("edge_index")
        if type(key) is not int or key < 0:
            raise WorldPlanError(f"{source_map} {kind} edge index is malformed")
        destination = warp_edges if kind == "warp" else connection_edges
        if key in destination:
            raise WorldPlanError(f"{source_map} has duplicate {kind} edge index {key}")
        destination[key] = edge
    if set(warp_edges) != set(range(len(warp_targets))):
        raise WorldPlanError(f"{source_map} edges do not cover warp_targets exactly")
    if set(connection_edges) != set(range(len(connections))):
        raise WorldPlanError(f"{source_map} edges do not cover connections exactly")
    for index, target in enumerate(warp_targets):
        edge = warp_edges[index]
        for edge_field, target_field in (
            ("target", "dest_map"),
            ("warp_id", "dest_warp_id"),
            ("classification", "classification"),
            ("target_map_id", "target_map_id"),
        ):
            if edge.get(edge_field) != target.get(target_field):
                raise WorldPlanError(f"{source_map} warp edge {index} differs from warp_targets")
    for index, connection in enumerate(connections):
        edge = connection_edges[index]
        for edge_field, connection_field in (("target", "map"), ("offset", "offset"), ("direction", "direction")):
            if edge.get(edge_field) != connection.get(connection_field):
                raise WorldPlanError(f"{source_map} connection edge {index} differs from connections")
        if target_map_ids is not None:
            expected_target_map_id = target_map_ids.get(connection.get("map"))
            if expected_target_map_id is None:
                if edge.get("classification") == "selected":
                    raise WorldPlanError(
                        f"{source_map} connection edge {index} classification differs from selected-map identity"
                    )
                if edge.get("target_map_id") is not None:
                    raise WorldPlanError(
                        f"{source_map} connection edge {index} target_map_id differs from selected-map identity"
                    )
            else:
                if edge.get("classification") != "selected":
                    raise WorldPlanError(
                        f"{source_map} connection edge {index} classification differs from selected-map identity"
                    )
                if edge.get("target_map_id") != expected_target_map_id:
                    raise WorldPlanError(
                        f"{source_map} connection edge {index} target_map_id differs from selected-map identity"
                    )
    if source is not None:
        for index, (connection, donor_connection) in enumerate(zip(connections, donor_connections, strict=True)):
            if connection != donor_connection:
                raise WorldPlanError(f"{source_map} connections[{index}] differs from donor topology")


def _apply_topology_corrections(maps: list[dict[str, Any]]) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    corrected: list[dict[str, Any]] = []
    corrections: list[dict[str, Any]] = []
    for record in maps:
        source_map = record["source_map"]
        item = copy.deepcopy(record)
        original_warps = item["warp_targets"]
        removed_warps = WARP_REMOVALS.get(source_map, frozenset())
        kept_warps: list[dict[str, Any]] = []
        old_to_new: dict[int, int] = {}
        for old_index, warp in enumerate(original_warps):
            if old_index in removed_warps:
                corrections.append({
                    "source_map": source_map,
                    "kind": "warp",
                    "index": old_index,
                    "action": "remove",
                    "reason": "reviewed debug or adapter edge",
                })
                continue
            old_to_new[old_index] = len(kept_warps)
            kept_warps.append(warp)
        for old_index, warp in enumerate(kept_warps):
            original_index = next(index for index, new_index in old_to_new.items() if new_index == old_index)
            warp["index"] = old_index
            rewrite = WARP_REWRITES.get((source_map, original_index), {})
            if rewrite:
                before = {field: warp.get(field) for field in rewrite}
                warp.update(copy.deepcopy(rewrite))
                corrections.append({
                    "source_map": source_map,
                    "kind": "warp",
                    "index": original_index,
                    "action": "rewrite",
                    "before": before,
                    "after": {field: warp.get(field) for field in rewrite},
                })
            warp["index"] = old_index
        item["warp_targets"] = kept_warps

        original_connections = item["connections"]
        removed_connections = CONNECTION_REMOVALS.get(source_map, frozenset())
        item["connections"] = []
        for index, connection in enumerate(original_connections):
            if index in removed_connections:
                corrections.append({
                    "source_map": source_map,
                    "kind": "connection",
                    "index": index,
                    "action": "remove",
                    "reason": "reviewed debug edge",
                })
                continue
            item["connections"].append(connection)

        warp_edges = {
            edge["index"]: edge for edge in item["edges"] if edge.get("kind") == "warp"
        }
        connection_edges = {
            edge["edge_index"]: edge for edge in item["edges"] if edge.get("kind") == "connection"
        }
        item["edges"] = []
        for old_index, warp in enumerate(original_warps):
            if old_index not in old_to_new:
                continue
            edge = copy.deepcopy(warp_edges[old_index])
            edge["index"] = old_to_new[old_index]
            for edge_field, warp_field in (
                ("target", "dest_map"),
                ("warp_id", "dest_warp_id"),
                ("classification", "classification"),
                ("target_map_id", "target_map_id"),
            ):
                if warp_field in warp:
                    edge[edge_field] = warp[warp_field]
                elif edge_field in edge:
                    edge.pop(edge_field)
            if warp.get("classification") == "selected":
                edge.pop("adapter", None)
            item["edges"].append(edge)
        for old_index, connection in enumerate(original_connections):
            if old_index in removed_connections:
                continue
            edge = copy.deepcopy(connection_edges[old_index])
            edge["edge_index"] = len([candidate for candidate in item["edges"] if candidate.get("kind") == "connection"])
            for edge_field, connection_field in (("target", "map"), ("offset", "offset"), ("direction", "direction")):
                edge[edge_field] = connection[connection_field]
            item["edges"].append(edge)
        item["edges"] = [
            *[edge for edge in item["edges"] if edge.get("kind") == "connection"],
            *[edge for edge in item["edges"] if edge.get("kind") == "warp"],
        ]
        _cross_validate_world_representations(item)
        corrected.append(item)
    return corrected, corrections


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


def _validate_preserved_external_script_destinations(root: Path) -> list[dict[str, Any]]:
    """Parse the campaign script labels that intentionally retain host exits.

    The script is the independent source of truth for these eight destinations;
    the expected tuple list only defines which labels are reviewed and what
    their materialized plan entries must be called.
    """
    script_path = root / CAMPAIGN_SCRIPTS_PATH
    try:
        script = script_path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as exc:
        raise WorldPlanError(f"cannot read preserved external script oracle {script_path}: {exc}") from exc
    parsed: list[dict[str, Any]] = []
    for source_map, label, target_map, expected_x, expected_y in PRESERVED_EXTERNAL_SCRIPT_DESTINATIONS:
        block_match = re.search(
            rf"(?ms)^{re.escape(label)}::(.*?)(?=^\S+::|\Z)",
            script,
        )
        if block_match is None:
            raise WorldPlanError(f"preserved external script label is missing: {label}")
        destinations = re.findall(
            r"(?m)^\s*warpsilent\s+([^,\s]+),\s*(\d+),\s*(\d+)",
            block_match.group(1),
        )
        if len(destinations) != 1:
            raise WorldPlanError(f"preserved external script label must have one warpsilent destination: {label}")
        actual_target, actual_x, actual_y = destinations[0]
        actual = (actual_target, int(actual_x), int(actual_y))
        expected = (target_map, expected_x, expected_y)
        if actual != expected:
            raise WorldPlanError(
                f"preserved external script destination drift for {label}: {actual!r} != {expected!r}"
            )
        parsed.append(
            {
                "source_map": source_map,
                "script": label,
                "target_map": actual_target,
                "x": int(actual_x),
                "y": int(actual_y),
            }
        )
    return parsed


def _validate_companion_ledgers(
    root: Path, donor: Path, maps: list[dict[str, Any]]
) -> tuple[dict[str, Any], dict[str, Any], dict[str, bytes]]:
    scenery = _load(root / SCENERY_PATH)
    content = _load(root / CONTENT_PATH)
    if _canonical_json_sha256(scenery) != EXPECTED_SCENERY_REGISTRATION_SHA256:
        raise WorldPlanError("scenery ledger semantics differ from the sealed digest")
    if _canonical_json_sha256(content) != EXPECTED_CONTENT_SYMBOLS_SHA256:
        raise WorldPlanError("content-symbol ledger semantics differ from the sealed digest")
    if (
        scenery.get("provenance", {}).get("region_manifest_sha256")
        != ASSET_RECORDED_REGION_MANIFEST_SHA256
    ):
        raise WorldPlanError("scenery ledger recorded region-manifest digest drift")
    if content.get("provenance", {}).get("manifest_sha256") != CONTENT_RECORDED_REGION_MANIFEST_SHA256:
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
        raise WorldPlanError("General Safari runtime readiness drift")
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
    target_map_ids = {
        record["source_map"]: record["identity_namespace"]["map"]
        for record in maps
        if isinstance(record, dict)
        and isinstance(record.get("source_map"), str)
        and isinstance(record.get("identity_namespace"), dict)
        and isinstance(record["identity_namespace"].get("map"), str)
    }
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
        _cross_validate_world_representations(record, source, target_map_ids)
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
    if not isinstance(external, list) or tuple(_edge_identity(edge) for edge in external) != EXPECTED_SOURCE_EXTERNAL_EDGES:
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

    corrected_maps, topology_corrections = _apply_topology_corrections(maps)
    corrected_by_id = {
        item["identity_namespace"]["map"]: item for item in corrected_maps
    }
    for item in corrected_maps:
        for warp in item["warp_targets"]:
            target_map_id = warp.get("target_map_id")
            destination = warp.get("dest_warp_id")
            if target_map_id in corrected_by_id and isinstance(destination, str) and destination.isdigit():
                target = corrected_by_id[target_map_id]
                if int(destination) >= len(target["warp_targets"]):
                    raise WorldPlanError(
                        f"{item['source_map']} warp {warp['index']} has invalid destination warp id {destination}"
                    )
        _cross_validate_world_representations(item, target_map_ids=target_map_ids)
    corrected_selected_warps = sum(
        warp.get("classification") == "selected"
        for item in corrected_maps
        for warp in item["warp_targets"]
    )
    corrected_selected_connections = sum(
        edge.get("kind") == "connection" and edge.get("classification") == "selected"
        for item in corrected_maps
        for edge in item["edges"]
    )
    if (corrected_selected_warps, corrected_selected_connections) != (1174, 191):
        raise WorldPlanError("corrected selected edge totals must be 1174 warps and 191 connections")
    normalized_external = []
    for item in corrected_maps:
        for edge in item["edges"]:
            if edge.get("classification") != "selected":
                external_edge = copy.deepcopy(edge)
                external_edge["source_map"] = item["source_map"]
                normalized_external.append(external_edge)
    if tuple(_edge_identity(edge) for edge in normalized_external) != EXPECTED_EXTERNAL_EDGES:
        raise WorldPlanError("corrected external edge identity or classification drift")
    normalized_categories = Counter(edge["classification"] for edge in normalized_external)
    source_event_totals = {key: event_totals[key] for key in EVENT_FIELDS}
    corrected_event_totals = {
        "object_events": source_event_totals["object_events"],
        "warp_events": sum(len(item["warp_targets"]) for item in corrected_maps),
        "coord_events": source_event_totals["coord_events"],
        "bg_events": source_event_totals["bg_events"],
    }
    if corrected_event_totals["warp_events"] != 1175:
        raise WorldPlanError(
            "corrected warp event total must be 1175, "
            f"got {corrected_event_totals['warp_events']}"
        )
    preserved_external_script_destinations = _validate_preserved_external_script_destinations(root)
    route40_events = donor_sources["MAP_ROUTE40"].get("object_events", [])
    route40_closure_payload = [
        event
        for event in route40_events
        if isinstance(event, dict)
        and any(
            marker in json.dumps(event, sort_keys=True).upper()
            for marker in ("ENGINEER", "TRAINER_HILL")
        )
    ]
    if route40_closure_payload:
        raise WorldPlanError(
            "Route40 Trainer Hill closure payload remains in the donor object events; "
            "its ownership must be reviewed before removing the entrance"
        )
    topology_notes = [
        {
            "subject": "Route40 Trainer Hill closure payload",
            "status": "none",
            "evidence": (
                "Pinned donor Route40 object_events contains no OBJ_EVENT_GFX_ENGINEER "
                "or TrainerHill-specific script, so the removed Trainer Hill entrance "
                "has no selected Route40 closure NPC to remove."
            ),
        }
    ]

    return {
        "schema": "johto-world-plan-v1",
        "mode": "materialized",
        "selected_map_count": 407,
        "original_map_count": 239,
        "later_map_count": 168,
        "group_counts": {str(key): groups[key] for key in sorted(groups)},
        "event_totals": corrected_event_totals,
        "source_event_totals": source_event_totals,
        "selected_warp_count": corrected_selected_warps,
        "selected_connection_count": corrected_selected_connections,
        "external_edge_counts": {
            "excluded_debug": normalized_categories["excluded_debug_edge"],
            "host_adapter": normalized_categories["required_host_adapter"],
            "runtime_policy": normalized_categories["pending_runtime_policy"],
            "era_boundary": normalized_categories["pending_era_boundary"],
        },
        "external_edges": [
            {"source_map": item[0], "kind": item[1], "index": item[2], "target": item[3], "classification": item[4]}
            for item in EXPECTED_EXTERNAL_EDGES
        ],
        "preserved_external_script_destinations": preserved_external_script_destinations,
        "topology_notes": topology_notes,
        "topology_corrections": topology_corrections,
        "maps": corrected_maps,
        "pending_general_layouts": list(PENDING_GENERAL_LAYOUTS),
        "pending_general_maps": pending_maps,
        "general_runtime_ready_map_count": 407,
        "production_write_ready": False,
        "blockers": [
            "complete 407-map campaign script compiler",
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
        output = Path(repo_root).resolve() / WORLD_PLAN_PATH
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_bytes(render(plan).encode("utf-8"))
    else:
        output = Path(repo_root).resolve() / WORLD_PLAN_PATH
        try:
            existing = _parse_json_document(output.read_bytes(), str(output))
        except OSError as exc:
            raise WorldPlanError(f"cannot read {output}: {exc}") from exc
        if existing != plan:
            raise WorldPlanError("generated world plan is stale")
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
