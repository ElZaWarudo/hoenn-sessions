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
SCENERY_REGISTRATION = ROOT / "data/johto/scenery_registration.json"
DONOR_REVISION = "751823abaf677020bcd72c45fe3e7cb2b8a576e4"
DONOR_REPOSITORY = "https://github.com/PokemonHnS-Development/pokemonHnS"
EXPECTED_SELECTED = 239
EXPECTED_ORIGINAL_SELECTED = 239
EXPECTED_LATER_SELECTED = 168
EXPECTED_TOTAL_SELECTED = EXPECTED_ORIGINAL_SELECTED + EXPECTED_LATER_SELECTED
EXPECTED_CLASSIFIED = 954
EXPECTED_SECTIONS = 57
EXPECTED_LATER_SECTIONS = 43
EXPECTED_TOTAL_SECTIONS = EXPECTED_SECTIONS + EXPECTED_LATER_SECTIONS - 1
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
}
PENDING_EXTERNAL_EDGE_TARGETS = {
    "MAP_LILYCOVE_CITY_CONTEST_LOBBY": "Viridian donor contest destination requires an explicit later-era policy",
    "MAP_TREES": "Vermilion donor Trees destination is outside the selected Kanto map closure",
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

LATER_KANTO_CITY_GROUPS = {
    "gMapGroup_IndoorPallet", "gMapGroup_IndoorViridian",
    "gMapGroup_IndoorPewter", "gMapGroup_IndoorCerulean",
    "gMapGroup_IndoorVermilion", "gMapGroup_IndoorLavender",
    "gMapGroup_IndoorCeladon", "gMapGroup_IndoorSaffron",
    "gMapGroup_IndoorFuchsia", "gMapGroup_IndoorCinnabar",
    "gMapGroup_IndoorIndigo",
}

# These are geographic host identities, rather than Johto section aliases.
# Keep the mapping explicit because identical source spellings can describe a
# different campaign and must never be inferred from a name alone.
KANTO_GEOGRAPHIC_SECTIONS = {
    "MAPSEC_PALLET_TOWN": ("MAPSEC_PALLET_TOWN", 88),
    "MAPSEC_VIRIDIAN_CITY": ("MAPSEC_VIRIDIAN_CITY", 89),
    "MAPSEC_PEWTER_CITY": ("MAPSEC_PEWTER_CITY", 90),
    "MAPSEC_CERULEAN_CITY": ("MAPSEC_CERULEAN_CITY", 91),
    "MAPSEC_LAVENDER_TOWN": ("MAPSEC_LAVENDER_TOWN", 92),
    "MAPSEC_VERMILION_CITY": ("MAPSEC_VERMILION_CITY", 93),
    "MAPSEC_CELADON_CITY": ("MAPSEC_CELADON_CITY", 94),
    "MAPSEC_FUCHSIA_CITY": ("MAPSEC_FUCHSIA_CITY", 95),
    "MAPSEC_CINNABAR_ISLAND": ("MAPSEC_CINNABAR_ISLAND", 96),
    "MAPSEC_INDIGO_PLATEAU": ("MAPSEC_INDIGO_PLATEAU", 97),
    "MAPSEC_SAFFRON_CITY": ("MAPSEC_SAFFRON_CITY", 98),
    "MAPSEC_ROUTE_1": ("MAPSEC_ROUTE_1", 101),
    "MAPSEC_ROUTE_2": ("MAPSEC_ROUTE_2", 102),
    "MAPSEC_ROUTE_3": ("MAPSEC_ROUTE_3", 103),
    "MAPSEC_ROUTE_4": ("MAPSEC_ROUTE_4", 104),
    "MAPSEC_ROUTE_5": ("MAPSEC_ROUTE_5", 105),
    "MAPSEC_ROUTE_6": ("MAPSEC_ROUTE_6", 106),
    "MAPSEC_ROUTE_7": ("MAPSEC_ROUTE_7", 107),
    "MAPSEC_ROUTE_8": ("MAPSEC_ROUTE_8", 108),
    "MAPSEC_ROUTE_9": ("MAPSEC_ROUTE_9", 109),
    "MAPSEC_ROUTE_10": ("MAPSEC_ROUTE_10", 110),
    "MAPSEC_ROUTE_11": ("MAPSEC_ROUTE_11", 111),
    "MAPSEC_ROUTE_12": ("MAPSEC_ROUTE_12", 112),
    "MAPSEC_ROUTE_13": ("MAPSEC_ROUTE_13", 113),
    "MAPSEC_ROUTE_14": ("MAPSEC_ROUTE_14", 114),
    "MAPSEC_ROUTE_15": ("MAPSEC_ROUTE_15", 115),
    "MAPSEC_ROUTE_16": ("MAPSEC_ROUTE_16", 116),
    "MAPSEC_ROUTE_17": ("MAPSEC_ROUTE_17", 117),
    "MAPSEC_ROUTE_18": ("MAPSEC_ROUTE_18", 118),
    "MAPSEC_ROUTE_19": ("MAPSEC_ROUTE_19", 119),
    "MAPSEC_ROUTE_20": ("MAPSEC_ROUTE_20", 120),
    "MAPSEC_ROUTE_21": ("MAPSEC_ROUTE_21", 121),
    "MAPSEC_ROUTE_22": ("MAPSEC_ROUTE_22", 122),
    "MAPSEC_ROUTE_24": ("MAPSEC_ROUTE_24", 124),
    "MAPSEC_ROUTE_25": ("MAPSEC_ROUTE_25", 125),
    "MAPSEC_VIRIDIAN_FOREST": ("MAPSEC_VIRIDIAN_FOREST", 126),
    "MAPSEC_MT_MOON": ("MAPSEC_MT_MOON", 127),
    "MAPSEC_DIGLETTS_CAVE": ("MAPSEC_DIGLETTS_CAVE", 131),
    "MAPSEC_ROCK_TUNNEL": ("MAPSEC_ROCK_TUNNEL", 138),
    "MAPSEC_SEAFOAM_ISLANDS": ("MAPSEC_SEAFOAM_ISLANDS", 139),
    "MAPSEC_CERULEAN_CAVE": ("MAPSEC_CERULEAN_CAVE", 141),
    "MAPSEC_POWER_PLANT": ("MAPSEC_POWER_PLANT", 142),
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
        elif symbol in {"MAPSEC_KANTO_VICTORY_ROAD", *(target for target, _ in KANTO_GEOGRAPHIC_SECTIONS.values())}:
            expected = 132 if symbol == "MAPSEC_KANTO_VICTORY_ROAD" else next(
                value for target, value in KANTO_GEOGRAPHIC_SECTIONS.values() if target == symbol
            )
            if ident != expected:
                raise ManifestError(f"reviewed Kanto section must preserve ID {expected}: {symbol}={ident}")
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


def _later_classification(group: str, name: str) -> tuple[bool, str]:
    """Classify the explicit Three Years Later Kanto extension.

    This is intentionally disjoint from ``_classification``.  The same donor
    registration can therefore not silently change era merely because a host
    map with a similar name exists.
    """
    if group == "gMapGroup_TownsAndRoutes":
        return (name not in SELECTED_TOWNS, "later Kanto campaign map" if name not in SELECTED_TOWNS else "Johto campaign map")
    if group == "gMapGroup_IndoorKantoRoutes":
        return (name not in ROUTE26_HOUSES, "later Kanto campaign map" if name not in ROUTE26_HOUSES else "Johto campaign map")
    if group == "gMapGroup_Dungeons":
        selected = name == "Route19_Cave" or name.startswith(KANTO_DUNGEON_PREFIXES)
        return (selected, "later Kanto campaign map" if selected else "Johto dungeon")
    if group in LATER_KANTO_CITY_GROUPS:
        return True, "later Kanto campaign map"
    return False, "outside later Kanto scope"


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


def _accepted_target_layouts() -> dict[str, str]:
    """Return the target layout identities from the accepted scenery ledger.

    The donor layout symbol is retained as provenance on each map record.  The
    target identity is a host integration contract, however, so it must come
    from the reviewed scenery registration rather than a spelling convention.
    """
    data = _load(SCENERY_REGISTRATION)
    records = data.get("layouts")
    if not isinstance(records, list) or len(records) != EXPECTED_ORIGINAL_SELECTED:
        raise ManifestError(
            f"accepted scenery registration must contain {EXPECTED_ORIGINAL_SELECTED} layouts"
        )
    result: dict[str, str] = {}
    target_ids: set[str] = set()
    for record in records:
        source_map = record.get("source_map")
        source_layout = record.get("source_layout_id")
        target_layout = record.get("target_layout_id")
        if not source_map or not source_layout or not target_layout:
            raise ManifestError("accepted scenery layout is missing source/target identity")
        if source_map in result:
            raise ManifestError(f"duplicate accepted scenery map: {source_map}")
        if target_layout in target_ids:
            raise ManifestError(f"duplicate accepted target layout: {target_layout}")
        result[source_map] = target_layout
        target_ids.add(target_layout)
    return result


def _target_classification(target: str | None, selected_ids: set[str], donor_ids: set[str],
                           host_ids: set[str], source_map: str, edge_index: int) -> str:
    if not target:
        raise ManifestError(f"missing edge target: {source_map} edge {edge_index}")
    if target in selected_ids:
        return "selected"
    if target in REQUIRED_HOST_ADAPTER_TARGETS:
        return "required_host_adapter"
    if target in PENDING_EXTERNAL_EDGE_TARGETS:
        return "pending_runtime_policy"
    if target in DEBUG_TARGETS:
        return "excluded_debug_edge"
    # Host IDs outside the reviewed set and arbitrary symbols are unsafe to
    # silently classify: they need an explicit adapter decision.
    raise ManifestError(f"unknown edge target: {source_map} edge {edge_index}: {target}")


def _proposed_map_id(source_id: str, era: str = "johto") -> str:
    if era == "kanto_later":
        suffix = source_id.removeprefix("MAP_")
        return f"MAP_KANTO_LATER_{suffix}"
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


def _section_target(source: str, era: str = "johto") -> tuple[str, str, str | None]:
    if source == "MAPSEC_NEW_BARK_TOWN":
        return source, "preserved_host", None
    if source == "MAPSEC_VICTORY_ROAD":
        return "MAPSEC_KANTO_VICTORY_ROAD", "required_host_adapter", "RECEPTION_GATE"
    if era == "kanto_later" and source in KANTO_GEOGRAPHIC_SECTIONS:
        target, _ = KANTO_GEOGRAPHIC_SECTIONS[source]
        return target, "reviewed_kanto_geography", source
    suffix = source.removeprefix("MAPSEC_")
    target_suffix = SECTION_ALIASES.get(suffix, suffix)
    return f"MAPSEC_JOHTO_{target_suffix}", (
        "johto_alias" if suffix in SECTION_ALIASES else "new_johto"), target_suffix if suffix in SECTION_ALIASES else None


def _region_for_section(section: str, source_section: str | None = None) -> str:
    # Routes 26–28 are physically Kanto but remain part of the donor Johto
    # campaign.  Victory Road and every explicit later-era mapping are Kanto
    # geography as well.
    route_26_28 = {
        "MAPSEC_ROUTE_26", "MAPSEC_ROUTE_27", "MAPSEC_ROUTE_28",
        "MAPSEC_JOHTO_ROUTE_26", "MAPSEC_JOHTO_ROUTE_27", "MAPSEC_JOHTO_ROUTE_28",
    }
    if (section == "MAPSEC_KANTO_VICTORY_ROAD"
            or section in {target for target, _ in KANTO_GEOGRAPHIC_SECTIONS.values()}
            or section in route_26_28
            or source_section in route_26_28):
        return "REGION_KANTO"
    return "REGION_JOHTO"


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
    original_selected: list[dict[str, Any]] = []
    later_selected: list[dict[str, Any]] = []
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
            is_later, later_reason = _later_classification(group, name)
            if is_selected and is_later:
                raise ManifestError(f"map was selected for both eras: {name}")
            record = {"source_map": map_id, "name": name, "source_group": group,
                      "source_index": index,
                      "classification": "selected" if (is_selected or is_later) else "excluded",
                      "reason": reason if is_selected else later_reason if is_later else reason,
                      "era": "johto" if is_selected else "kanto_later" if is_later else None}
            all_records.append(record)
            if is_selected:
                original_selected.append({"group": group, "index": index, "name": name,
                                          "id": map_id, "path": map_json_path, "data": map_data,
                                          "era": "johto", "campaign": "johto"})
            elif is_later:
                later_selected.append({"group": group, "index": index, "name": name,
                                       "id": map_id, "path": map_json_path, "data": map_data,
                                       "era": "kanto_later", "campaign": "kanto_later"})
    selected = original_selected + later_selected
    if duplicate_ids:
        raise ManifestError("duplicate donor map ids: " + ", ".join(sorted(duplicate_ids)))
    if (len(all_records) != EXPECTED_CLASSIFIED
            or len(original_selected) != EXPECTED_ORIGINAL_SELECTED
            or len(later_selected) != EXPECTED_LATER_SELECTED
            or len(selected) != EXPECTED_TOTAL_SELECTED):
        raise ManifestError(
            f"selection counts differ: {len(original_selected)} original + "
            f"{len(later_selected)} later / {len(all_records)} classified"
        )

    accepted_target_layouts = _accepted_target_layouts()
    original_ids = {item["id"] for item in original_selected}
    if set(accepted_target_layouts) != original_ids:
        missing = sorted(original_ids - set(accepted_target_layouts))
        extra = sorted(set(accepted_target_layouts) - original_ids)
        raise ManifestError(
            "accepted scenery registration map prefix differs from selected Johto maps: "
            f"missing={missing[:3]} extra={extra[:3]}"
        )

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
        era = item["era"]
        target_section, section_kind, alias_target = _section_target(source_section, era)
        if target_section not in allocation:
            section_order.append(target_section)
            if target_section == "MAPSEC_NEW_BARK_TOWN":
                target_id = 209
            elif target_section == "MAPSEC_KANTO_VICTORY_ROAD":
                target_id = 132
            elif section_kind == "reviewed_kanto_geography":
                target_id = KANTO_GEOGRAPHIC_SECTIONS[source_section][1]
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
        if era == "johto":
            host_group, host_index = (75, ordinal) if ordinal <= 127 else (76, ordinal - 128)
        else:
            later_ordinal = ordinal - EXPECTED_ORIGINAL_SELECTED
            host_group, host_index = (77, later_ordinal) if later_ordinal <= 127 else (78, later_ordinal - 128)
        if host_group > 127 or host_index > 127:
            raise ManifestError(f"signed map component out of range for {item['name']}")
        proposed_map_id = _proposed_map_id(item["id"], era)
        _assert_proposed_map_identity(
            proposed_map_id, host_group, host_index, item["id"], baseline,
            proposed_map_ids, proposed_map_tuples,
        )
        source_map = item["path"]
        edges = _edge_records(data, selected_ids, donor_ids, host_ids, item["id"])
        if era == "kanto_later":
            for edge in edges:
                if edge.get("target") == "MAP_NEW_BARK_TOWN":
                    edge["classification"] = "pending_era_boundary"
                    edge["adapter"] = {"donor_symbol": "MAP_NEW_BARK_TOWN",
                                        "resolution": "pending_original_or_later_destination"}
        warp_targets = []
        for index, warp in enumerate(data.get("warp_events") or []):
            target = warp.get("dest_map")
            enriched = dict(warp)
            enriched["classification"] = _target_classification(
                target, selected_ids, donor_ids, host_ids, item["id"], index)
            enriched["index"] = index
            if era == "kanto_later" and target == "MAP_NEW_BARK_TOWN":
                enriched["classification"] = "pending_era_boundary"
                enriched["adapter"] = {"donor_symbol": "MAP_NEW_BARK_TOWN",
                                        "resolution": "pending_original_or_later_destination"}
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
            "region": _region_for_section(target_section, source_section),
            "geographic_region": _region_for_section(target_section, source_section),
            "era": "JOHTO" if era == "johto" else "KANTO_LATER",
            "campaign": "JOHTO" if era == "johto" else "KANTO_LATER",
            "world_era": "JOHTO" if era == "johto" else "KANTO_LATER",
            "identity_namespace": {
                "map": proposed_map_id,
                "layout": accepted_target_layouts[item["id"]] if era == "johto" else f"LAYOUT_KANTO_LATER_{layout_id.removeprefix('LAYOUT_')}",
                "script": f"Johto_{item['name']}" if era == "johto" else f"KantoLater_{item['name']}",
            },
            "source_section": source_section,
            "resolved_section": {"symbol": target_section, "id": allocation[target_section]["id"],
                                  "classification": section_kind, "alias_target": alias_target},
            "source_sha256": {"map_json": _sha256(source_map), "script": _sha256(script_path)},
            "connections": data.get("connections") or [],
            "warp_targets": warp_targets,
            "edges": edges,
        })
    source_sections = sorted({item["data"]["region_map_section"] for item in selected})
    if len(source_sections) != EXPECTED_TOTAL_SECTIONS:
        raise ManifestError(f"expected {EXPECTED_TOTAL_SECTIONS} source sections, got {len(source_sections)}")
    section_entries = []
    for source in source_sections:
        section_era = "kanto_later" if source in KANTO_GEOGRAPHIC_SECTIONS else "johto"
        target, kind, alias_target = _section_target(source, section_era)
        entry = allocation.get(target)
        if entry is None:
            raise ManifestError(f"section allocation missing for {source}")
        section_entries.append({"source_symbol": source, "target_symbol": target,
                                "target_id": entry["id"], "classification": kind,
                                "alias_target_suffix": alias_target})
    _assert_section_allocation(allocation)
    johto_sections = {entry["target_symbol"] for entry in section_entries
                      if entry["classification"] in {"new_johto", "johto_alias", "preserved_host"}}
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
    map_identity_by_source = {m["source_map"]: m["proposed_map"]["map_id"] for m in maps}
    for map_record in maps:
        for edge in map_record["edges"]:
            if edge.get("target") in map_identity_by_source:
                edge["target_map_id"] = map_identity_by_source[edge["target"]]
        for edge in map_record["warp_targets"]:
            if edge.get("dest_map") in map_identity_by_source:
                edge["target_map_id"] = map_identity_by_source[edge["dest_map"]]
    external_edges = [edge | {"source_map": m["source_map"]}
                      for m in maps for edge in m["edges"]
                      if edge["classification"] != "selected"]
    later_maps = [m for m in maps if m["era"] == "KANTO_LATER"]
    later_names = sorted(m["source_name"] for m in later_maps)
    later_name_text = "\n".join(later_names) + "\n"
    geographic_aliases = [
        {"source_symbol": source, "target_symbol": target, "target_id": target_id,
         "classification": "reviewed_host_geography"}
        for source, (target, target_id) in sorted(KANTO_GEOGRAPHIC_SECTIONS.items())
    ]
    geographic_aliases.append({"source_symbol": "MAPSEC_VICTORY_ROAD",
                               "target_symbol": "MAPSEC_KANTO_VICTORY_ROAD", "target_id": 132,
                               "classification": "reviewed_host_geography"})
    geographic_aliases.sort(key=lambda entry: entry["source_symbol"])
    era_extension = {
        "schema": "johto-kanto-era-identities-v1",
        "design": "docs/plans/johto-region/kanto-era-identity-design.md",
        "amendment": "docs/plans/johto-region/kanto-era-choice-amendment.md",
        "choice": ["KANTO_ORIGINAL", "KANTO_LATER", "CANCEL"],
        "original": {"selected_count": len(original_selected), "ordinal_range": [0, len(original_selected) - 1],
                      "campaign": "JOHTO", "geography": "REGION_JOHTO_OR_EXPLICIT_KANTO"},
        "later": {"selected_count": len(later_maps), "ordinal_range": [len(original_selected), len(maps) - 1],
                   "campaign": "KANTO_LATER", "geography": "REGION_KANTO",
                   "map_namespace": "MAP_KANTO_LATER_", "layout_namespace": "LAYOUT_KANTO_LATER_",
                   "script_namespace": "KantoLater_",
                   "group_allocations": [{"group": 77, "index_range": [0, 127]},
                                          {"group": 78, "index_range": [0, 39]}],
                   "sorted_name_sha256": hashlib.sha256(later_name_text.encode("utf-8")).hexdigest(),
                   "source_section_count": EXPECTED_LATER_SECTIONS},
        "source_section_union_count": len(source_sections),
        "geographic_aliases": geographic_aliases,
        "identities": [
            {"ordinal": m["ordinal"], "source_map": m["source_map"], "source_name": m["source_name"],
             "source_group": m["source_group"], "source_index": m["source_index"],
             "target_map": m["proposed_map"]["map_id"],
             "target_group": m["proposed_map"]["group"], "target_index": m["proposed_map"]["index"],
             "target_layout": m["identity_namespace"]["layout"],
             "target_script_namespace": m["identity_namespace"]["script"],
             "source_layout": m["layout"]["symbol"], "source_section": m["source_section"],
             "target_section": m["resolved_section"]["symbol"], "target_section_id": m["resolved_section"]["id"],
             "geography": m["geographic_region"], "campaign": m["campaign"], "era": m["era"],
             "source_sha256": m["source_sha256"]}
            for m in later_maps
        ],
        "pending_runtime_policy": {
            "era_choice": "runtime boundary must distinguish Original, Later and Cancel",
            "unknown_edges": "remain explicit diagnostics; no automatic Original-Kanto alias",
        },
    }
    return {
        "schema": "johto-region-manifest-v1",
        "provenance": {"repository": DONOR_REPOSITORY,
                       "donor_revision": revision, "donor_tree": tree,
                       "inventory_sha256": _sha256(INVENTORY),
                       "source_revision": "21b8c9f918800a07b74a5ee2a882b1374d9ac4f9",
                       "sealed_baseline_tree": "80ca34d91679a22288cb4e9c6fd7f8168a5d8554"},
        "selection": {"selected_count": len(selected), "original_selected_count": len(original_selected),
                      "later_selected_count": len(later_selected), "classified_count": len(all_records),
                      "excluded_count": len(all_records) - len(selected),
                      "donor_group_order": order, "proposed_map_groups": {"existing": 75,
                      "first_range": "75:0..127", "second_range": "76:0..110",
                      "later_first_range": "77:0..127", "later_second_range": "78:0..39"}},
        "host_identity": {"new_bark": {"group": 75, "index": 0, "map_id": "MAP_NEW_BARK_TOWN"},
                          "reserved_section_ids": [250, 251, 252, 253, 254, 255],
                          "signed_component_max": 127},
        "sections": {"source_count": len(source_sections), "later_source_count": EXPECTED_LATER_SECTIONS,
                     "johto_count": len({e["target_symbol"] for e in section_entries
                                         if e["classification"] in {"new_johto", "johto_alias", "preserved_host"}}),
                     "kanto_count": len({e["target_symbol"] for e in section_entries
                                         if e["classification"] in {"required_host_adapter", "reviewed_kanto_geography"}}),
                     "entries": section_entries, "allocation_order": section_order,
                     "geographic_aliases": geographic_aliases},
        "section_aliases": aliases,
        "registrations": all_records,
        "maps": maps,
        "external_edges": external_edges,
        "era_extension": era_extension,
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
        identity_output = OUTPUT.parent / "kanto_era_identities.json"
        identity_rendered = _canonical_json(generated["era_extension"])
        if args.check:
            if not OUTPUT.exists():
                raise ManifestError(f"manifest is missing: {OUTPUT}")
            actual = OUTPUT.read_text(encoding="utf-8")
            if actual != rendered:
                raise ManifestError(f"stale manifest: regenerate {OUTPUT}")
            if not identity_output.exists():
                raise ManifestError(f"era identity ledger is missing: {identity_output}")
            identity_actual = identity_output.read_text(encoding="utf-8")
            if identity_actual != identity_rendered:
                raise ManifestError(f"stale era identity ledger: regenerate {identity_output}")
        else:
            OUTPUT.parent.mkdir(parents=True, exist_ok=True)
            OUTPUT.write_text(rendered, encoding="utf-8", newline="\n")
            identity_output.write_text(identity_rendered, encoding="utf-8", newline="\n")
        return 0
    except ManifestError as exc:
        print(f"region_manifest: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
