"""Stage the pinned Cormoria campaign for a later, reviewed ROM registration.

This copies content inputs, never donor engine code or a save ABI. The stage is
not a playable world: native bindings and external destinations remain explicit
adapter obligations. No live map, layout, or script registry is edited here.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import tempfile
from pathlib import Path, PurePosixPath
from typing import Any

from tools.cormoria import region_manifest

ROOT = Path(__file__).resolve().parents[2]
MANIFESTS = Path("data/cormoria")
PINNED_SHA256 = {
    "region_manifest.json": "bd2b9d092c3900327878dbdb72bd037956ffc178c156fa7bd187237d243b2091",
    "symbol_ledger.json": "01a377cb93a2e1f5b536b67d255a890eee626f378cf4c6d4e077050a2a7059a1",
    "source_manifest.json": "bd1729a7430b1ade867e5434b8e7440a0ecd729c320f41a5a8a46434ca482c70",
}
EXPECTED_COUNTS = (165, 165, 51, 59)
EXPECTED_GROUP_COUNTS = (28, 32, 40, 31, 30, 4)
EXTERNAL_EDGES = {"MAP_DYNAMIC", "MAP_ROUTE112", "MAP_ROUTE117_POKEMON_DAY_CARE"}
CAMPAIGN_DATA_FILES = {
    "src/data/heal_locations.h", "src/data/heal_locations_pkm_center.h",
    "src/data/mining_minigame.h", "src/data/partner_parties.h",
    "src/data/battle_partners.h", "src/data/trainers.h",
    "src/data/trainers.party", "src/data/wild_encounters.json",
    "src/data/object_events/base_oam.h",
    "src/data/object_events/berry_tree_graphics_tables.h",
    "src/data/object_events/object_event_anims.h",
    "src/data/object_events/object_event_graphics.h",
    "src/data/object_events/object_event_graphics_info.h",
    "src/data/object_events/object_event_graphics_info_followers.h",
    "src/data/object_events/object_event_graphics_info_pointers.h",
    "src/data/object_events/object_event_pic_tables.h",
    "src/data/object_events/object_event_pic_tables_followers.h",
    "src/data/object_events/object_event_subsprites.h",
}
CAMPAIGN_DATA_PREFIXES = (
    "src/data/region_map/", "src/data/tilesets/",
)
TOKEN = re.compile(r"\b[A-Za-z_][A-Za-z_0-9]*\b")
QUOTED = re.compile(r'("(?:\\.|[^"\\])*")')


class ImportError(ValueError):
    """The stage cannot be shown to match the authenticated campaign."""


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def manifest_bytes(root: Path, name: str) -> bytes:
    data = (root / MANIFESTS / name).read_bytes()
    # Git checkouts on Windows may materialize the same committed JSON with
    # CRLF. Bind to its canonical LF content, not the checkout's line endings.
    if b"\r" in data.replace(b"\r\n", b""):
        raise ImportError(f"invalid manifest line endings: {name}")
    return data.replace(b"\r\n", b"\n")


def canonical(value: Any) -> bytes:
    return (json.dumps(value, indent=2, ensure_ascii=False) + "\n").encode("utf-8")


def safe_relative(value: str) -> Path:
    if not isinstance(value, str) or not value or "\\" in value:
        raise ImportError(f"invalid source path: {value!r}")
    parts = PurePosixPath(value).parts
    if value.startswith("/") or any(part in {"", ".", ".."} for part in parts) or ":" in parts[0]:
        raise ImportError(f"source path escape: {value}")
    return Path(*parts)


def source_bytes(donor: Path, relative: str, indexed: dict[str, dict[str, Any]]) -> bytes:
    if relative not in indexed:
        raise ImportError(f"source omitted from hash manifest: {relative}")
    path = donor / safe_relative(relative)
    if not path.is_file() or not path.resolve().is_relative_to(donor.resolve()):
        raise ImportError(f"missing or escaping source: {relative}")
    data = path.read_bytes()
    record = indexed[relative]
    if len(data) != record["bytes"] or digest(data) != record["sha256"]:
        raise ImportError(f"source hash mismatch: {relative}")
    return data


def load_manifests(root: Path = ROOT) -> tuple[dict[str, Any], dict[str, Any], dict[str, Any]]:
    documents = []
    for name, expected in PINNED_SHA256.items():
        data = manifest_bytes(root, name)
        if digest(data) != expected:
            raise ImportError(f"pinned manifest changed: {name}")
        documents.append(json.loads(data))
    region, symbols, sources = documents
    provenance = region["provenance"]
    if (provenance["revision"] != region_manifest.DONOR_REVISION
            or any(item["provenance"] != provenance for item in (symbols, sources))):
        raise ImportError("manifest provenance mismatch")
    if tuple(len(region[k]) for k in ("maps", "layouts", "sections", "tilesets")) != EXPECTED_COUNTS:
        raise ImportError("campaign content count drifted")
    if tuple(len(group["maps"]) for group in region["groups"]) != EXPECTED_GROUP_COUNTS:
        raise ImportError("campaign group count drifted")
    if region["runtime_ready"] or {edge["symbol"] for edge in region["external_edges"]} != EXTERNAL_EDGES:
        raise ImportError("runtime or external-edge contract drifted")
    return region, symbols, sources


def identities(region: dict[str, Any], symbols: dict[str, Any]) -> dict[str, str]:
    result: dict[str, str] = {}

    def add(source: str, target: str) -> None:
        if not source or not target or (source in result and result[source] != target):
            raise ImportError(f"colliding identity: {source}")
        result[source] = target

    for kind, left, right in (("maps", "source_id", "target_id"),
                              ("layouts", "id", "target_id"),
                              ("sections", "source_symbol", "target_symbol"),
                              ("tilesets", "source_symbol", "target_symbol")):
        for item in region[kind]:
            add(item[left], item[right])
    for kind in ("flags", "vars", "trainers", "heals", "labels"):
        for item in symbols[kind]:
            if item.get("binding", "campaign_owned") == "campaign_owned":
                add(item["source_symbol"], item["target_symbol"])
    targets = list(result.values())
    if len(targets) != len(set(targets)):
        raise ImportError("target identity collision")
    return result


def selected_paths(region: dict[str, Any], symbols: dict[str, Any],
                   sources: dict[str, Any]) -> tuple[set[str], dict[str, dict[str, Any]]]:
    records = sources["files"]
    indexed = {item["path"]: item for item in records}
    if len(indexed) != len(records):
        raise ImportError("duplicate source path")
    selected: set[str] = set()
    for item in region["maps"]:
        base = f"data/maps/{item['source_name']}"
        selected.update((f"{base}/map.json", f"{base}/scripts.inc"))
    for item in region["layouts"]:
        selected.update((item["border_filepath"], item["blockdata_filepath"]))
    for item in symbols["labels"]:
        selected.add(item["definition"].rsplit(":", 1)[0])
    # These are campaign data and resources in the authenticated closure.
    # src/*.c, donor headers and assembly macros are semantic evidence only.
    selected.update(item["path"] for item in records
                    if (item["path"].startswith(("data/tilesets/", "sound/", "graphics/"))
                        or item["path"] in CAMPAIGN_DATA_FILES
                        or item["path"].startswith(CAMPAIGN_DATA_PREFIXES))
                    and not item["path"].endswith(".pory"))
    asset_index = {item["requested"]: item for item in sources["assets"]}
    if len(asset_index) != len(sources["assets"]):
        raise ImportError("duplicate asset recipe")
    visited_assets: set[str] = set()

    def add_asset(requested: str, pending: set[str]) -> None:
        if requested in pending:
            raise ImportError(f"cyclic asset recipe: {requested}")
        if requested in visited_assets:
            return
        asset = asset_index.get(requested)
        if asset is None:
            raise ImportError(f"unresolved asset recipe: {requested}")
        dependencies = [asset["source"]] if "source" in asset else asset.get("sources", [])
        if not dependencies:
            raise ImportError(f"asset without source: {requested}")
        for dependency in dependencies:
            if dependency in indexed:
                selected.add(dependency)
            else:
                add_asset(dependency, pending | {requested})
        visited_assets.add(requested)

    for requested in asset_index:
        add_asset(requested, set())
    for path in selected:
        safe_relative(path)
        if path not in indexed:
            raise ImportError(f"required content omitted from source manifest: {path}")
        if path.startswith(("src/", "include/", "asm/")) and not path.startswith("src/data/"):
            raise ImportError(f"engine ABI in content selection: {path}")
    return selected, indexed


def rewrite(value: Any, lookup: dict[str, str]) -> Any:
    if isinstance(value, dict):
        return {key: rewrite(item, lookup) for key, item in value.items()}
    if isinstance(value, list):
        return [rewrite(item, lookup) for item in value]
    if isinstance(value, str):
        return lookup.get(value, value)
    return value


def rewrite_script(text: str, lookup: dict[str, str]) -> str:
    """Rename exact script symbols while preserving quoted dialogue and paths."""
    chunks = QUOTED.split(text)
    return "".join(chunk if index % 2 else TOKEN.sub(
        lambda match: lookup.get(match.group(), match.group()), chunk)
        for index, chunk in enumerate(chunks))


def build_stage(donor: Path, output: Path, root: Path = ROOT) -> dict[str, Any]:
    donor = donor.resolve(strict=True)
    output = output.resolve()
    if output == root.resolve() or output.is_relative_to(root.resolve()) or output.is_relative_to(donor):
        raise ImportError("stage output must be outside the host and donor checkouts")
    if output.exists():
        raise ImportError(f"refusing to overwrite stage output: {output}")
    region, symbols, sources = load_manifests(root)
    provenance = region_manifest.verify_donor(donor)
    if provenance != region["provenance"]:
        raise ImportError("donor tree differs from pinned manifests")
    paths, indexed = selected_paths(region, symbols, sources)
    lookup = identities(region, symbols)
    map_ids = {item["source_id"] for item in region["maps"]}
    adapters = {item["symbol"]: item for item in region["external_edges"]}
    # Every map destination must be owned by this world or an explicit adapter.
    for item in region["maps"]:
        name = item["source_name"]
        original = json.loads(source_bytes(donor, f"data/maps/{name}/map.json", indexed))
        if original["id"] != item["source_id"] or original["layout"] != item["layout"]:
            raise ImportError(f"map identity drift: {name}")
        connections = original.get("connections", [])
        if connections == 0:
            connections = []
        destinations = [edge["map"] for edge in connections]
        destinations += [edge["dest_map"] for edge in original.get("warp_events", [])]
        for token in destinations:
            if token.startswith("MAP_") and token not in map_ids and token not in adapters:
                raise ImportError(f"unresolved map edge {token} in {name}")
    script_lines: dict[str, list[str]] = {}
    for item in symbols["labels"]:
        path, line = item["definition"].rsplit(":", 1)
        if path not in script_lines:
            script_lines[path] = source_bytes(donor, path, indexed).decode("utf-8-sig").splitlines()
        lines = script_lines[path]
        if not line.isdecimal() or not 1 <= int(line) <= len(lines) or not re.search(
                rf"\b{re.escape(item['source_symbol'])}\s*:", lines[int(line) - 1]):
            raise ImportError(f"unresolved script label: {item['source_symbol']} at {item['definition']}")
    parent = output.parent
    parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="cormoria-stage-", dir=parent) as temporary:
        stage = Path(temporary)
        written = []

        def put(relative: str, data: bytes) -> None:
            destination = stage / safe_relative(relative)
            if destination.exists():
                raise ImportError(f"stage path collision: {relative}")
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(data)
            written.append({"path": relative, "bytes": len(data), "sha256": digest(data)})

        for name in PINNED_SHA256:
            put(f"provenance/{name}", manifest_bytes(root, name))
        for path in sorted(paths):
            put(f"source/{path}", source_bytes(donor, path, indexed))
        for item in region["maps"]:
            source = f"data/maps/{item['source_name']}/map.json"
            mapped = rewrite(json.loads(source_bytes(donor, source, indexed)), lookup)
            mapped["name"] = item["target_name"]
            mapped["id"] = item["target_id"]
            mapped["rom_world"] = "cormoria"
            put(f"maps/{item['target_name']}/map.json", canonical(mapped))
        for item in region["layouts"]:
            layout = dict(item)
            layout["id"] = item["target_id"]
            layout["name"] = item["target_name"]
            layout["primary_tileset"] = lookup.get(item["primary_tileset"], item["primary_tileset"])
            layout["secondary_tileset"] = lookup.get(item["secondary_tileset"], item["secondary_tileset"])
            layout["rom_world"] = "cormoria"
            put(f"layouts/{item['target_name']}/layout.json", canonical(layout))
        for path in sorted(script_lines):
            original = source_bytes(donor, path, indexed).decode("utf-8-sig")
            put(f"namespaced_scripts/{path}", rewrite_script(original, lookup).encode("utf-8"))
        # A later unit must translate and link these boundaries, then prove the
        # campaign round trip. Their presence is deliberately not runtime-ready.
        obligations = {"external_edges": region["external_edges"],
                       "native_bindings": symbols["native_bindings"],
                       "status": "staged_only_adapters_not_implemented"}
        put("adapter_obligations.json", canonical(obligations))
        index = {"schema_version": 1, "world_id": "cormoria", "provenance": provenance,
                 "manifest_sha256": PINNED_SHA256, "map_count": len(region["maps"]),
                 "layout_count": len(region["layouts"]), "section_count": len(region["sections"]),
                 "tileset_count": len(region["tilesets"]), "asset_recipe_count": len(sources["assets"]),
                 "namespaced_script_source_count": len(script_lines),
                 "compiled_script_format": "donor scripts.inc inputs plus namespaced script text; not linked",
                 "runtime_ready": False, "identities": lookup, "files": written}
        put("staging_manifest.json", canonical(index))
        stage.rename(output)
    return index


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--donor", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args(argv)
    try:
        index = build_stage(args.donor, args.output)
    except (ImportError, region_manifest.ManifestError, OSError, KeyError, ValueError) as error:
        print(f"Cormoria staging error: {error}", file=sys.stderr)
        return 1
    print(f"Staged {index['map_count']} maps and {index['layout_count']} layouts at {args.output}; runtime adapters pending")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
