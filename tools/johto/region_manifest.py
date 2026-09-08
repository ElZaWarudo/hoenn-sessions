"""Build and validate the pinned Johto map import manifest.

This module deliberately produces a data ledger only.  It never installs map
registrations or changes runtime files.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any, Iterable

ROOT = Path(__file__).resolve().parents[2]
INVENTORY = ROOT / "docs/plans/johto-region/j1/inventory.json"
OUTPUT = ROOT / "data/johto/region_manifest.json"
HOST_IDENTITY_BASELINE = ROOT / "data/johto/host_identity_baseline.json"
DONOR_REVISION = "751823abaf677020bcd72c45fe3e7cb2b8a576e4"
DONOR_REPOSITORY = "https://github.com/PokemonHnS-Development/pokemonHnS"
EXPECTED_SELECTED = 239
EXPECTED_CLASSIFIED = 954
EXPECTED_SECTIONS = 57
EXPECTED_JOHTO_SECTIONS = 41
HOST_BASELINE_REVISION = "21b8c9f918800a07b74a5ee2a882b1374d9ac4f9"
RESERVED_SECTION_IDS = {250, 251, 252, 253, 254, 255}
SECTION_ID_MIN = 210
SECTION_ID_MAX = 249

# These are the reviewed cross-boundary edges in this tooling slice.  A donor
# symbol is retained in the ledger and resolved by a later runtime adapter.
REQUIRED_HOST_ADAPTER_TARGETS = {
    "MAP_ROUTE22",
    "MAP_BATTLE_FRONTIER_BATTLE_TOWER_LOBBY",
    "MAP_DYNAMIC",
}
DEBUG_TARGETS = {
    "MAP_WORLD_HUB",
    "MAP_TRAINER_HILL_COURTYARD",
    "MAP_GATE_ROUTE40_TRAINER_HILL_COURTYARD",
    "MAP_VICTORY_ROAD_KANTO_B2F",
}
PROPOSED_MAP_ID_ALIASES = {
    "MAP_ROCKET_HIDEOUT_B1F": "MAP_JOHTO_ROCKET_HIDEOUT_B1F",
    "MAP_ROCKET_HIDEOUT_B2F": "MAP_JOHTO_ROCKET_HIDEOUT_B2F",
    "MAP_ROCKET_HIDEOUT_B3F": "MAP_JOHTO_ROCKET_HIDEOUT_B3F",
}

SELECTED_TOWNS = {
    "NewBarkTown", "CherrygroveCity", "VioletCity", "AzaleaTown",
    "GoldenrodCity", "EcruteakCity", "OlivineCity", "CianwoodCity",
    "SafariZoneGate", "Mahoganytown", "BlackthornCity", "Route29",
    "Route30", "Route31", "Route32", "Route33", "Route34", "Route35",
    "Route36", "Route37", "Route38", "Route39", "Route40", "Route41",
    "Route42", "Route43", "Route44", "Route45", "Route46", "Route47",
    "Route48", "Route26", "Route26North", "Route27", "Route28",
}
FULL_JOHTO_GROUPS = {
    "gMapGroup_IndoorNewBark", "gMapGroup_IndoorCherrygrove",
    "gMapGroup_IndoorViolet", "gMapGroup_IndoorAzalea",
    "gMapGroup_IndoorGoldenrod", "gMapGroup_IndoorEcruteak",
    "gMapGroup_IndoorOlivine", "gMapGroup_IndoorCianwood",
    "gMapGroup_IndoorMahogany", "gMapGroup_IndoorBlackthorn",
}
TRAINER_HILL = {"Gate_Route40_TrainerHill_Courtyard", "TrainerHill_Courtyard"}
ROUTE26_HOUSES = {"Route26_House1", "Route26_House2"}
KANTO_DUNGEON_PREFIXES = (
    "VictoryRoadKanto_", "ViridianForest", "MtMoon_", "RockTunnel_", "CeruleanCave_",
    "DiglettsCave_", "SeafoamIslands_",
)
SPECIAL_SELECTED = {
    "SafariZone_Top_Left", "SafariZone_Low_Mid", "SafariZone_Enterance",
    "SafariZone_Low_Left", "SafariZone_Low_Right", "SafariZone_Top_Mid",
    "SafariZone_Top_Right",
}
SPECIAL_EXCLUDED = {
    "Trees", "WorldHub", "WorldHub2", "NewMap1", "SafariZone1",
    "SafariZone2", "SafariZone3", "SafariZoneIndoor", "Saffron_Temp",
}

# The suffixes are intentionally kept as data: this is the reviewed semantic
# allocation and is also emitted in the manifest for later runtime work.
SECTION_ALIASES = {
    "OLIVINE_LIGHTHOUSE": "OLIVINE_CITY",
    "SPROUT_TOWER": "VIOLET_CITY",
    "BURNED_TOWER": "ECRUTEAK_CITY",
    "TIN_TOWER": "ECRUTEAK_CITY",
    "DRAGONS_DEN": "BLACKTHORN_CITY",
    "SLOWPOKE_WELL": "AZALEA_TOWN",
    "ROCKET_HIDEOUT": "MAHOGANY_TOWN",
    "SAFARI_ZONE": "SAFARI_ZONE_GATE",
    "CLIFF_CAVE": "ROUTE_47",
    "EMBEDDED_TOWER": "ROUTE_47",
    "TOHJO_FALLS": "ROUTE_27",
    "UNION_CAVE": "ROUTE_32",
    "DARK_CAVE": "ROUTE_31",
    "MT_MORTAR": "ROUTE_42",
    "ICE_PATH": "ROUTE_44",
}

# The source JSON may be at its immutable 210-entry bootstrap state or may
# already contain this exact append-only tail.  Keep the expected tail here so
# host identity checks reject arbitrary additions and reordering.
EXPECTED_SECTION_TAIL_IDS = (
    "MAPSEC_JOHTO_CHERRYGROVE_CITY",
    "MAPSEC_JOHTO_VIOLET_CITY",
    "MAPSEC_JOHTO_AZALEA_TOWN",
    "MAPSEC_JOHTO_GOLDENROD_CITY",
    "MAPSEC_JOHTO_ECRUTEAK_CITY",
    "MAPSEC_JOHTO_OLIVINE_CITY",
    "MAPSEC_JOHTO_CIANWOOD_CITY",
    "MAPSEC_JOHTO_SAFARI_ZONE_GATE",
    "MAPSEC_JOHTO_MAHOGANY_TOWN",
    "MAPSEC_JOHTO_BLACKTHORN_CITY",
    "MAPSEC_JOHTO_ROUTE_29",
    "MAPSEC_JOHTO_ROUTE_30",
    "MAPSEC_JOHTO_ROUTE_31",
    "MAPSEC_JOHTO_ROUTE_32",
    "MAPSEC_JOHTO_ROUTE_33",
    "MAPSEC_JOHTO_ROUTE_34",
    "MAPSEC_JOHTO_ROUTE_35",
    "MAPSEC_JOHTO_ROUTE_36",
    "MAPSEC_JOHTO_ROUTE_37",
    "MAPSEC_JOHTO_ROUTE_38",
    "MAPSEC_JOHTO_ROUTE_39",
    "MAPSEC_JOHTO_ROUTE_40",
    "MAPSEC_JOHTO_ROUTE_41",
    "MAPSEC_JOHTO_ROUTE_42",
    "MAPSEC_JOHTO_ROUTE_43",
    "MAPSEC_JOHTO_ROUTE_44",
    "MAPSEC_JOHTO_ROUTE_45",
    "MAPSEC_JOHTO_ROUTE_46",
    "MAPSEC_JOHTO_ROUTE_47",
    "MAPSEC_JOHTO_ROUTE_48",
    "MAPSEC_JOHTO_ROUTE_26",
    "MAPSEC_JOHTO_ROUTE_27",
    "MAPSEC_JOHTO_ROUTE_28",
    "MAPSEC_JOHTO_LAKE_OF_RAGE",
    "MAPSEC_JOHTO_NATIONAL_PARK",
    "MAPSEC_JOHTO_RUINS_OF_ALPH",
    "MAPSEC_JOHTO_MT_SILVER",
    "MAPSEC_JOHTO_ILEX_FOREST",
    "MAPSEC_JOHTO_WHIRL_ISLANDS",
    "MAPSEC_JOHTO_SS_AQUA",
)


class ManifestError(ValueError):
    """An actionable donor or manifest integrity failure."""


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _load(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise ManifestError(f"missing required file: {path}") from exc
    except json.JSONDecodeError as exc:
        raise ManifestError(f"invalid JSON in {path}: {exc}") from exc


def _git_revision(donor: Path) -> tuple[str, str]:
    try:
        revision = subprocess.check_output(
            ["git", "-C", str(donor), "rev-parse", "HEAD"],
            text=True, stderr=subprocess.STDOUT,
        ).strip()
        tree = subprocess.check_output(
            ["git", "-C", str(donor), "rev-parse", "HEAD^{tree}"],
            text=True, stderr=subprocess.STDOUT,
        ).strip()
    except (OSError, subprocess.CalledProcessError) as exc:
        raise ManifestError(f"donor is not a readable git checkout: {donor}") from exc
    return revision, tree


def _assert_donor_clean(donor: Path) -> None:
    """Reject mutable donor map/layout inputs around a corpus read."""
    if not (donor / ".git").exists():
        # Synthetic tests may use a temporary fixture while mocking the pin.
        return
    try:
        status = subprocess.check_output(
            ["git", "-C", str(donor), "status", "--porcelain=v1",
             "--untracked-files=all", "--", "data/maps", "data/layouts"],
            text=True, stderr=subprocess.STDOUT,
        )
    except (OSError, subprocess.CalledProcessError) as exc:
        raise ManifestError(f"unable to verify donor cleanliness: {donor}") from exc
    if status.strip():
        raise ManifestError("donor has dirty map/layout inputs: " + status.strip())


def _host_identity(path: Path) -> dict[str, Any]:
    """Read the immutable host identity ledger and validate its shape."""
    baseline = _load(path)
    if baseline.get("source_revision") != HOST_BASELINE_REVISION:
        raise ManifestError("host identity baseline source revision mismatch")
    if not isinstance(baseline.get("group_order"), list):
        raise ManifestError("host identity baseline has no group_order")
    if not isinstance(baseline.get("groups"), dict):
        raise ManifestError("host identity baseline has no groups")
    if not isinstance(baseline.get("section_constants"), list):
        raise ManifestError("host identity baseline has no section_constants")
    return baseline


def _assert_host_identity(baseline: dict[str, Any]) -> None:
    """Fail closed if host map order, identity, or section constants drift."""
    live_groups = _load(ROOT / "data/maps/map_groups.json")
    expected_order = baseline["group_order"]
    if live_groups.get("group_order") != expected_order:
        raise ManifestError("host map group order differs from immutable baseline")
    expected_groups = baseline["groups"]
    for group in expected_order:
        expected = expected_groups.get(group)
        actual = live_groups.get(group)
        expected_names = [identity["name"] for identity in expected or []]
        if actual != expected_names:
            raise ManifestError(f"host map group identity/order differs from baseline: {group}")
        for index, identity in enumerate(expected):
            map_data = _load(ROOT / "data/maps" / identity["name"] / "map.json")
            actual_identity = {"name": identity["name"], "id": map_data.get("id"),
                               "group": group, "index": index}
            if actual_identity != identity:
                raise ManifestError(
                    f"host map identity differs from baseline: {group}[{index}]"
                )
    section_path = ROOT / "src/data/region_map/region_map_sections.json"
    live_sections = _load(section_path).get("map_sections")
    expected_sections = baseline["section_constants"]
    actual_constants = [{"id": record.get("id"), "value": index}
                        for index, record in enumerate(live_sections or [])]
    if actual_constants[:len(expected_sections)] != expected_sections:
        raise ManifestError("host region section constants differ from immutable baseline")
    tail = actual_constants[len(expected_sections):]
    if tail not in (
        [],
        [{"id": section_id, "value": 210 + offset}
         for offset, section_id in enumerate(EXPECTED_SECTION_TAIL_IDS)],
    ):
        raise ManifestError("host region section tail is not the exact Johto append-only allocation")


def _assert_section_allocation(allocation: dict[str, dict[str, Any]]) -> None:
    ids = [entry["id"] for entry in allocation.values()]
    if len(ids) != len(set(ids)):
        raise ManifestError("proposed section IDs collide")
    for symbol, entry in allocation.items():
        ident = entry["id"]
        if symbol == "MAPSEC_NEW_BARK_TOWN":
            if ident != 209:
                raise ManifestError("New Bark section must preserve ID 209")
        elif symbol == "MAPSEC_KANTO_VICTORY_ROAD":
            if ident != 132:
                raise ManifestError("Kanto Victory Road section must preserve ID 132")
        elif ident < SECTION_ID_MIN or ident > SECTION_ID_MAX or ident in RESERVED_SECTION_IDS:
            raise ManifestError(f"proposed section ID out of Johto range: {symbol}={ident}")


def _classification(group: str, name: str) -> tuple[bool, str]:
    if group == "gMapGroup_TownsAndRoutes":
        return (name in SELECTED_TOWNS,
                "johto campaign" if name in SELECTED_TOWNS else "canonical Kanto map")
    if group in FULL_JOHTO_GROUPS:
        return True, "johto campaign"
    if group == "gMapGroup_IndoorJohtoRoutes":
        return (name not in TRAINER_HILL,
                "johto campaign" if name not in TRAINER_HILL else "Trainer Hill contamination")
    if group == "gMapGroup_IndoorKantoRoutes":
        return (name in ROUTE26_HOUSES,
                "explicit Route 26 house" if name in ROUTE26_HOUSES else "canonical Kanto map")
    if group == "gMapGroup_Dungeons":
        excluded = name == "Route19_Cave" or name.startswith(KANTO_DUNGEON_PREFIXES)
        return (not excluded, "johto campaign" if not excluded else "Kanto duplicate")
    if group == "gMapGroup_SpecialArea":
        selected = name.startswith("SSAqua_") or name in SPECIAL_SELECTED
        if selected:
            return True, "johto campaign"
        return False, "debug/unused/temporary" if name in SPECIAL_EXCLUDED else "outside Johto scope"
    return False, "outside Johto scope"


def _map_groups(donor: Path) -> tuple[list[str], dict[str, list[str]]]:
    groups = _load(donor / "data/maps/map_groups.json")
    order = groups.get("group_order")
    if not isinstance(order, list) or not order:
        raise ManifestError("donor map_groups.json has no group_order")
    result = {group: groups.get(group) for group in order}
    for group, names in result.items():
        if not isinstance(names, list):
            raise ManifestError(f"map group {group} is missing or not a list")
        if len(names) != len(set(names)):
            raise ManifestError(f"duplicate map name in donor group {group}")
    return order, result


def _host_map_ids() -> set[str]:
    path = ROOT / "data/maps/map_groups.json"
    if not path.exists():
        return set()
    groups = _load(path)
    found: set[str] = set()
    for names in groups.values():
        if not isinstance(names, list):
            continue
        for name in names:
            map_json = ROOT / "data/maps" / name / "map.json"
            if map_json.exists():
                value = _load(map_json).get("id")
                if value:
                    found.add(value)
    return found


def _layout_records(donor: Path) -> dict[str, dict[str, Any]]:
    data = _load(donor / "data/layouts/layouts.json")
    records = data.get("layouts", [])
    result: dict[str, dict[str, Any]] = {}
    for record in records:
        ident = record.get("id")
        if not ident or ident in result:
            raise ManifestError(f"duplicate or missing layout id in donor: {ident!r}")
        result[ident] = record
    return result


def _target_classification(target: str | None, selected_ids: set[str], donor_ids: set[str],
                           host_ids: set[str], source_map: str, edge_index: int) -> str:
    if not target:
        raise ManifestError(f"missing edge target: {source_map} edge {edge_index}")
    if target in selected_ids:
        return "selected"
    if target in REQUIRED_HOST_ADAPTER_TARGETS:
        return "required_host_adapter"
    if target in DEBUG_TARGETS:
        return "excluded_debug_edge"
    # Host IDs outside the reviewed set and arbitrary symbols are unsafe to
    # silently classify: they need an explicit adapter decision.
    raise ManifestError(f"unknown edge target: {source_map} edge {edge_index}: {target}")


def _proposed_map_id(source_id: str) -> str:
    return PROPOSED_MAP_ID_ALIASES.get(source_id, source_id)


def _assert_proposed_map_identity(
    map_id: str,
    group: int,
    index: int,
    source_map: str,
    baseline: dict[str, Any],
    proposed_ids: set[str],
    proposed_tuples: set[tuple[int, int]],
) -> None:
    """Reject proposed map symbols and numeric identities already in use."""
    baseline_ids = {
        identity["id"]
        for group_name in baseline["group_order"]
        for identity in baseline["groups"].get(group_name, [])
    }
    baseline_tuples = {
        (baseline["group_order"].index(identity["group"]), identity["index"])
        for group_name in baseline["group_order"]
        for identity in baseline["groups"].get(group_name, [])
    }
    deliberate_new_bark_reuse = (
        source_map == "MAP_NEW_BARK_TOWN"
        and map_id == "MAP_NEW_BARK_TOWN"
        and (group, index) == (75, 0)
    )
    identity_tuple = (group, index)
    if not deliberate_new_bark_reuse and map_id in baseline_ids:
        raise ManifestError(f"proposed map symbol collides with host baseline: {map_id}")
    if not deliberate_new_bark_reuse and identity_tuple in baseline_tuples:
        raise ManifestError(
            f"proposed map numeric identity collides with host baseline: {group}:{index}"
        )
    if map_id in proposed_ids and not deliberate_new_bark_reuse:
        raise ManifestError(f"proposed map symbol collides with another proposal: {map_id}")
    if identity_tuple in proposed_tuples and not deliberate_new_bark_reuse:
        raise ManifestError(
            f"proposed map numeric identity collides with another proposal: {group}:{index}"
        )
    proposed_ids.add(map_id)
    proposed_tuples.add(identity_tuple)


def _section_target(source: str) -> tuple[str, str, str | None]:
    if source == "MAPSEC_NEW_BARK_TOWN":
        return source, "preserved_host", None
    if source == "MAPSEC_VICTORY_ROAD":
        return "MAPSEC_KANTO_VICTORY_ROAD", "required_host_adapter", "RECEPTION_GATE"
    suffix = source.removeprefix("MAPSEC_")
    target_suffix = SECTION_ALIASES.get(suffix, suffix)
    return f"MAPSEC_JOHTO_{target_suffix}", (
        "johto_alias" if suffix in SECTION_ALIASES else "new_johto"), target_suffix if suffix in SECTION_ALIASES else None


def _region_for_section(section: str) -> str:
    return "REGION_KANTO" if section == "MAPSEC_KANTO_VICTORY_ROAD" else "REGION_JOHTO"


def _edge_records(map_data: dict[str, Any], selected_ids: set[str], donor_ids: set[str],
                  host_ids: set[str], source_map: str) -> list[dict[str, Any]]:
    edges: list[dict[str, Any]] = []
    for index, connection in enumerate(map_data.get("connections") or []):
        target = connection.get("map")
        classification = _target_classification(target, selected_ids, donor_ids, host_ids,
                                                 source_map, index)
        record = {"kind": "connection", "edge_index": index, "target": target,
                  "classification": classification,
                  "offset": connection.get("offset"), "direction": connection.get("direction")}
        if classification == "required_host_adapter":
            record["adapter"] = {"donor_symbol": target, "resolution": "pending_runtime_resolution"}
        edges.append(record)
    for index, warp in enumerate(map_data.get("warp_events") or []):
        target = warp.get("dest_map")
        classification = _target_classification(target, selected_ids, donor_ids, host_ids,
                                                 source_map, index)
        record = {"kind": "warp", "index": index, "target": target,
                  "warp_id": warp.get("dest_warp_id"), "classification": classification}
        if classification == "required_host_adapter":
            record["adapter"] = {"donor_symbol": target, "resolution": "pending_runtime_resolution"}
        edges.append(record)
    return edges


def build_manifest(donor: str | Path) -> dict[str, Any]:
    donor_path = Path(donor).resolve()
    inventory = _load(INVENTORY)
    revision, tree = _git_revision(donor_path)
    _assert_donor_clean(donor_path)
    baseline = _host_identity(HOST_IDENTITY_BASELINE)
    _assert_host_identity(baseline)
    if revision != DONOR_REVISION or revision != inventory.get("donor", {}).get("revision"):
        raise ManifestError(f"donor revision mismatch: expected {DONOR_REVISION}, got {revision}")
    order, groups = _map_groups(donor_path)
    layouts = _layout_records(donor_path)
    all_records: list[dict[str, Any]] = []
    selected: list[dict[str, Any]] = []
    donor_ids: set[str] = set()
    duplicate_ids: set[str] = set()
    for group in order:
        for index, name in enumerate(groups[group]):
            map_json_path = donor_path / "data/maps" / name / "map.json"
            if not map_json_path.exists():
                raise ManifestError(f"missing map registration source: {map_json_path}")
            map_data = _load(map_json_path)
            map_id = map_data.get("id")
            if not map_id or map_data.get("name") != name:
                raise ManifestError(f"map source identity mismatch: {map_json_path}")
            if map_id in donor_ids:
                duplicate_ids.add(map_id)
            donor_ids.add(map_id)
            is_selected, reason = _classification(group, name)
            record = {"source_map": map_id, "name": name, "source_group": group,
                      "source_index": index, "classification": "selected" if is_selected else "excluded",
                      "reason": reason}
            all_records.append(record)
            if is_selected:
                selected.append({"group": group, "index": index, "name": name,
                                 "id": map_id, "path": map_json_path, "data": map_data})
    if duplicate_ids:
        raise ManifestError("duplicate donor map ids: " + ", ".join(sorted(duplicate_ids)))
    if len(all_records) != EXPECTED_CLASSIFIED or len(selected) != EXPECTED_SELECTED:
        raise ManifestError(f"selection counts differ: {len(selected)} selected / {len(all_records)} classified")

    selected_ids = {item["id"] for item in selected}
    host_ids = _host_map_ids()
    allocation: dict[str, dict[str, Any]] = {}
    section_order: list[str] = []
    maps: list[dict[str, Any]] = []
    proposed_map_ids: set[str] = set()
    proposed_map_tuples: set[tuple[int, int]] = set()
    for ordinal, item in enumerate(selected):
        data = item["data"]
        source_section = data.get("region_map_section")
        if not source_section:
            raise ManifestError(f"map has no region_map_section: {item['name']}")
        target_section, section_kind, alias_target = _section_target(source_section)
        if target_section not in allocation:
            section_order.append(target_section)
            if target_section == "MAPSEC_NEW_BARK_TOWN":
                target_id = 209
            elif target_section == "MAPSEC_KANTO_VICTORY_ROAD":
                target_id = 132
            else:
                target_id = 210 + sum(1 for value in allocation.values() if value["kind"] in ("new_johto", "johto_alias"))
            allocation[target_section] = {"target_symbol": target_section, "id": target_id,
                                          "kind": section_kind, "alias_target": alias_target}
        layout_id = data.get("layout")
        layout = layouts.get(layout_id)
        if layout is None:
            raise ManifestError(f"missing layout reference {layout_id!r} for {item['name']}")
        layout_paths = {}
        for key in ("border_filepath", "blockdata_filepath"):
            relative = layout.get(key)
            if not relative:
                raise ManifestError(f"layout {layout_id} missing {key}")
            path = donor_path / relative
            if not path.exists():
                raise ManifestError(f"missing layout asset {relative} for {item['name']}")
            layout_paths[key] = {"path": relative, "sha256": _sha256(path)}
        script_relative = f"data/maps/{item['name']}/scripts.inc"
        script_path = donor_path / script_relative
        if not script_path.exists():
            raise ManifestError(f"missing map script source: {script_path}")
        if ordinal == 0 and item["id"] != "MAP_NEW_BARK_TOWN":
            raise ManifestError("New Bark must be the first selected registration")
        host_group, host_index = (75, ordinal) if ordinal <= 127 else (76, ordinal - 128)
        if host_group > 127 or host_index > 127:
            raise ManifestError(f"signed map component out of range for {item['name']}")
        proposed_map_id = _proposed_map_id(item["id"])
        _assert_proposed_map_identity(
            proposed_map_id, host_group, host_index, item["id"], baseline,
            proposed_map_ids, proposed_map_tuples,
        )
        source_map = item["path"]
        edges = _edge_records(data, selected_ids, donor_ids, host_ids, item["id"])
        warp_targets = []
        for index, warp in enumerate(data.get("warp_events") or []):
            target = warp.get("dest_map")
            enriched = dict(warp)
            enriched["classification"] = _target_classification(
                target, selected_ids, donor_ids, host_ids, item["id"], index)
            enriched["index"] = index
            if enriched["classification"] == "required_host_adapter":
                enriched["adapter"] = {"donor_symbol": target,
                                        "resolution": "pending_runtime_resolution"}
            warp_targets.append(enriched)
        maps.append({
            "ordinal": ordinal,
            "source_map": item["id"],
            "source_name": item["name"],
            "source_group": item["group"],
            "source_index": item["index"],
            "proposed_host": {"group": host_group, "index": host_index},
            "proposed_map": {"map_id": proposed_map_id, "group": host_group, "index": host_index},
            "layout": {"symbol": layout_id, "primary_tileset": layout.get("primary_tileset"),
                       "secondary_tileset": layout.get("secondary_tileset"), "assets": layout_paths},
            "region": _region_for_section(target_section),
            "source_section": source_section,
            "resolved_section": {"symbol": target_section, "id": allocation[target_section]["id"],
                                  "classification": section_kind, "alias_target": alias_target},
            "source_sha256": {"map_json": _sha256(source_map), "script": _sha256(script_path)},
            "connections": data.get("connections") or [],
            "warp_targets": warp_targets,
            "edges": edges,
        })
    source_sections = sorted({item["data"]["region_map_section"] for item in selected})
    if len(source_sections) != EXPECTED_SECTIONS:
        raise ManifestError(f"expected {EXPECTED_SECTIONS} source sections, got {len(source_sections)}")
    section_entries = []
    for source in source_sections:
        target, kind, alias_target = _section_target(source)
        entry = allocation.get(target)
        if entry is None:
            raise ManifestError(f"section allocation missing for {source}")
        section_entries.append({"source_symbol": source, "target_symbol": target,
                                "target_id": entry["id"], "classification": kind,
                                "alias_target_suffix": alias_target})
    _assert_section_allocation(allocation)
    johto_sections = {entry["target_symbol"] for entry in section_entries
                      if entry["target_symbol"] != "MAPSEC_KANTO_VICTORY_ROAD"}
    if len(johto_sections) != EXPECTED_JOHTO_SECTIONS:
        raise ManifestError(f"expected {EXPECTED_JOHTO_SECTIONS} Johto sections, got {len(johto_sections)}")
    aliases = [{"source_suffix": source, "target_suffix": target,
                "source_symbol": f"MAPSEC_{source}",
                "target_symbol": f"MAPSEC_JOHTO_{target}",
                "target_id": allocation[f"MAPSEC_JOHTO_{target}"]["id"]}
               for source, target in SECTION_ALIASES.items()]
    existing = _load(ROOT / "data/maps/NewBarkTown/map.json")
    if existing.get("id") != "MAP_NEW_BARK_TOWN":
        raise ManifestError("host New Bark map identity changed")
    host_groups = _load(ROOT / "data/maps/map_groups.json")
    if host_groups.get("gMapGroup_Johto", [None])[0] != "NewBarkTown":
        raise ManifestError("host Johto group no longer preserves New Bark")
    reception = next((entry for entry in maps if entry["source_map"] == "MAP_RECEPTION_GATE"), None)
    if reception is None or reception["region"] != "REGION_KANTO" or reception["source_section"] != "MAPSEC_VICTORY_ROAD" or reception["resolved_section"] != {"symbol": "MAPSEC_KANTO_VICTORY_ROAD", "id": 132, "classification": "required_host_adapter", "alias_target": "RECEPTION_GATE"}:
        raise ManifestError("ReceptionGate must retain the full Kanto border section record")
    final_revision, final_tree = _git_revision(donor_path)
    if (final_revision, final_tree) != (revision, tree):
        raise ManifestError("donor revision changed while building manifest")
    _assert_donor_clean(donor_path)
    external_edges = [edge | {"source_map": m["source_map"]}
                      for m in maps for edge in m["edges"]
                      if edge["classification"] != "selected"]
    return {
        "schema": "johto-region-manifest-v1",
        "provenance": {"repository": DONOR_REPOSITORY,
                       "donor_revision": revision, "donor_tree": tree,
                       "inventory_sha256": _sha256(INVENTORY),
                       "source_revision": "21b8c9f918800a07b74a5ee2a882b1374d9ac4f9",
                       "sealed_baseline_tree": "80ca34d91679a22288cb4e9c6fd7f8168a5d8554"},
        "selection": {"selected_count": len(selected), "classified_count": len(all_records),
                      "excluded_count": len(all_records) - len(selected),
                      "donor_group_order": order, "proposed_map_groups": {"existing": 75,
                      "first_range": "75:1..127", "second_range": "76:0..110"}},
        "host_identity": {"new_bark": {"group": 75, "index": 0, "map_id": "MAP_NEW_BARK_TOWN"},
                          "reserved_section_ids": [250, 251, 252, 253, 254, 255],
                          "signed_component_max": 127},
        "sections": {"source_count": len(source_sections),
                     "johto_count": len({e["target_symbol"] for e in section_entries
                                         if e["target_symbol"] != "MAPSEC_KANTO_VICTORY_ROAD"}),
                     "kanto_count": 1, "entries": section_entries, "allocation_order": section_order},
        "section_aliases": aliases,
        "registrations": all_records,
        "maps": maps,
        "external_edges": external_edges,
    }




def _canonical_json(value: Any) -> str:
    return json.dumps(value, indent=2, sort_keys=False, ensure_ascii=False) + "\n"


def main(argv: Iterable[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--donor", default=os.environ.get("JOHTO_DONOR"),
                        help="pinned donor checkout (or JOHTO_DONOR)")
    parser.add_argument("--check", action="store_true", help="verify checked-in manifest")
    args = parser.parse_args(argv)
    try:
        if not args.donor:
            raise ManifestError("donor checkout is required (pass --donor or set JOHTO_DONOR)")
        generated = build_manifest(args.donor)
        rendered = _canonical_json(generated)
        if args.check:
            if not OUTPUT.exists():
                raise ManifestError(f"manifest is missing: {OUTPUT}")
            actual = OUTPUT.read_text(encoding="utf-8")
            if actual != rendered:
                raise ManifestError(f"stale manifest: regenerate {OUTPUT}")
        else:
            OUTPUT.parent.mkdir(parents=True, exist_ok=True)
            OUTPUT.write_text(rendered, encoding="utf-8", newline="\n")
        return 0
    except ManifestError as exc:
        print(f"region_manifest: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
