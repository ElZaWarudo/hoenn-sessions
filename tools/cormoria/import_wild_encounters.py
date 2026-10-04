"""Import Cormoria's authenticated wild encounters into the shared table.

Dreamstone uses the same source map names for several worlds.  The region
manifest is therefore the namespace authority: only its Cormoria map sources
are selected, and their map IDs are rewritten to the allocated Cormoria IDs.
Every species, level range, and per-area encounter rate is copied verbatim.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
import sys
from pathlib import Path
from typing import Any

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

from tools.cormoria import import_world


ROOT = Path(__file__).resolve().parents[2]
SOURCE = "src/data/wild_encounters.json"
EXPECTED_CORMORIA_ENTRIES = 41
GROUP_LABEL = "gWildMonHeaders"


class ImportErrorStrict(ValueError):
    """The authenticated source or generated table cannot be trusted."""


def _source_entry(sources: dict[str, Any]) -> dict[str, Any]:
    for entry in sources["files"]:
        if entry.get("path") == SOURCE:
            return entry
    raise ImportErrorStrict(f"source absent from pinned inventory: {SOURCE}")


def _load_donor(source_root: Path, repo_root: Path = ROOT) -> tuple[dict[str, Any], dict[str, str]]:
    """Load and authenticate the staged donor file and map namespace."""

    region, _symbols, sources = import_world.load_manifests(repo_root)
    expected = _source_entry(sources)
    source = source_root / SOURCE
    if not source.is_file() or not source.resolve().is_relative_to(source_root.resolve()):
        raise ImportErrorStrict(f"missing donor source: {source}")
    raw = source.read_bytes()
    if len(raw) != expected["bytes"] or hashlib.sha256(raw).hexdigest() != expected["sha256"]:
        raise ImportErrorStrict("wild encounter source differs from authenticated donor bytes")
    try:
        donor = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise ImportErrorStrict("authenticated donor wild encounters are not valid JSON") from exc

    maps = {}
    for entry in region["maps"]:
        source_id = entry.get("source_id")
        target_id = entry.get("target_id")
        if not isinstance(source_id, str) or not isinstance(target_id, str):
            raise ImportErrorStrict("invalid map namespace entry")
        if target_id.startswith("MAP_CORMORIA_"):
            if source_id in maps and maps[source_id] != target_id:
                raise ImportErrorStrict(f"ambiguous Cormoria map namespace: {source_id}")
            maps[source_id] = target_id
    if not maps:
        raise ImportErrorStrict("Cormoria map namespace is empty")
    return donor, maps


def _cormoria_entries(donor: dict[str, Any], maps: dict[str, str]) -> list[dict[str, Any]]:
    groups = donor.get("wild_encounter_groups")
    if not isinstance(groups, list) or not groups or groups[0].get("label") != GROUP_LABEL:
        raise ImportErrorStrict("donor wild encounter group shape changed")
    encounters = groups[0].get("encounters")
    if not isinstance(encounters, list):
        raise ImportErrorStrict("donor map encounter list is missing")
    selected_indices = [
        index for index, entry in enumerate(encounters)
        if isinstance(entry, dict) and entry.get("map") in maps
    ]
    if len(selected_indices) != EXPECTED_CORMORIA_ENTRIES:
        raise ImportErrorStrict(
            f"expected {EXPECTED_CORMORIA_ENTRIES} Cormoria donor entries, found {len(selected_indices)}"
        )
    if selected_indices != list(range(len(encounters) - EXPECTED_CORMORIA_ENTRIES, len(encounters))):
        raise ImportErrorStrict("Cormoria donor entries are no longer the authenticated tail")

    result = []
    for index in selected_indices:
        original = encounters[index]
        if set(original) - {"map", "base_label", "land_mons", "water_mons", "rock_smash_mons", "fishing_mons", "hidden_mons"}:
            raise ImportErrorStrict(f"unsupported donor encounter fields at index {index}")
        transformed = copy.deepcopy(original)
        transformed["map"] = maps[original["map"]]
        result.append(transformed)
    return result


def merge(host: dict[str, Any], cormoria: list[dict[str, Any]]) -> dict[str, Any]:
    """Replace only our generated map entries while retaining host order/data."""

    output = copy.deepcopy(host)
    groups = output.get("wild_encounter_groups")
    if not isinstance(groups, list) or not groups or groups[0].get("label") != GROUP_LABEL:
        raise ImportErrorStrict("host wild encounter group shape changed")
    encounters = groups[0].get("encounters")
    if not isinstance(encounters, list):
        raise ImportErrorStrict("host map encounter list is missing")
    targets = {entry["map"] for entry in cormoria}
    existing = [entry for entry in encounters if isinstance(entry, dict) and entry.get("map") in targets]
    if existing and existing != cormoria:
        raise ImportErrorStrict("existing Cormoria wild encounters drifted from authenticated source")
    output["wild_encounter_groups"][0]["encounters"] = [
        entry for entry in encounters
        if not (isinstance(entry, dict) and entry.get("map") in targets)
    ] + copy.deepcopy(cormoria)
    return output


def render(host: dict[str, Any], cormoria: list[dict[str, Any]]) -> str:
    return json.dumps(merge(host, cormoria), indent=2, ensure_ascii=False) + "\n"


def run(source_root: Path, repo_root: Path = ROOT, check: bool = False) -> int:
    donor, maps = _load_donor(source_root, repo_root)
    cormoria = _cormoria_entries(donor, maps)
    output = repo_root / SOURCE
    if not output.is_file():
        raise ImportErrorStrict(f"host output is missing: {output}")
    host = json.loads(output.read_text(encoding="utf-8"))
    expected = render(host, cormoria)
    actual = output.read_text(encoding="utf-8")
    if check:
        if actual != expected:
            raise ImportErrorStrict("generated Cormoria wild encounters drifted")
    else:
        newline = "\r\n" if b"\r\n" in output.read_bytes() else "\n"
        with output.open("w", encoding="utf-8", newline=newline) as stream:
            stream.write(expected)
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--source-root",
        type=Path,
        default=Path(os.environ["CORMORIA_WILD_SOURCE"]) if os.environ.get("CORMORIA_WILD_SOURCE") else None,
        help="authenticated staged donor root (or CORMORIA_WILD_SOURCE)",
    )
    parser.add_argument("--check", action="store_true", help="fail if the generated table is stale")
    args = parser.parse_args(argv)
    if args.source_root is None:
        parser.error("--source-root or CORMORIA_WILD_SOURCE is required")
    try:
        return run(args.source_root, check=args.check)
    except (ImportErrorStrict, import_world.ImportError) as exc:
        parser.error(str(exc))
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
