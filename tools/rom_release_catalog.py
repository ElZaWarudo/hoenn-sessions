"""Validate a trusted, world-neutral multi-ROM release catalog.

The catalog is release input, not a path supplied by a save or portal script.
Its digest must come from separately trusted release packaging. Artifact checks
alone cannot authenticate a substituted catalog.
"""

from __future__ import annotations

import argparse
from functools import lru_cache
import hashlib
import json
import os
import re
import subprocess
from pathlib import Path

if __package__:
    from tools.rom_world_registry import load_registry
    from tools.coop import player_transfer_manifest as transfer_schema
else:
    from rom_world_registry import load_registry
    from coop import player_transfer_manifest as transfer_schema

TOKEN = re.compile(r"[a-z][a-z0-9_]*\Z")
DIGEST = re.compile(r"[0-9a-f]{64}\Z")
_VERIFIED_ARRIVALS: set[tuple[str, int, int, int, int]] = set()


@lru_cache(maxsize=1)
def _arrival_verifier() -> Path:
    """Build the same V2 parser used by the server before approving templates."""
    root = Path(__file__).resolve().parents[1]
    executable = root / "target" / "debug" / ("verify_arrival_save.exe" if os.name == "nt" else "verify_arrival_save")
    result = subprocess.run(
        ["cargo", "build", "-p", "coop-save", "--bin", "verify_arrival_save",
         "--locked", "--quiet", "--target-dir", str(root / "target")],
        cwd=root / "coop", capture_output=True, text=True, check=False)
    if result.returncode != 0 or not executable.is_file():
        raise ValueError("arrival save verifier could not be built")
    return executable


def _verify_arrival_save(path: Path, digest: str, group: int, number: int, warp: int,
                         layout: int) -> None:
    key = (digest, group, number, warp, layout)
    if key in _VERIFIED_ARRIVALS:
        return
    result = subprocess.run(
        [str(_arrival_verifier()), str(path), digest, str(group), str(number), str(warp),
         str(layout)],
        capture_output=True, text=True, check=False)
    if result.returncode != 0:
        raise ValueError(f"arrival save rejected: {result.stderr.strip()}")
    _VERIFIED_ARRIVALS.add(key)


def _integer(value: object, label: str, maximum: int = 65535) -> int:
    if type(value) is not int or not 0 <= value <= maximum:
        raise ValueError(f"{label} must be an integer from 0 through {maximum}")
    return value


def _token(value: object, label: str) -> str:
    if (not isinstance(value, str) or not TOKEN.fullmatch(value)
            or ("portal" in label and len(value) > 96)):
        raise ValueError(f"invalid {label}")
    return value


def _artifact(root: Path, name: object, digest: object) -> Path:
    if (not isinstance(name, str) or not name or "\\" in name or ":" in name
            or name.startswith("/")
            or any(part in ("", ".", "..") for part in name.split("/"))):
        raise ValueError("artifact path must be release-relative")
    relative = Path(name)
    if relative.is_absolute() or any(part in (".", "..") for part in relative.parts):
        raise ValueError("artifact path must be release-relative")
    if not isinstance(digest, str) or not DIGEST.fullmatch(digest):
        raise ValueError(f"invalid artifact digest for {name}")
    path = (root / relative).resolve()
    if not path.is_relative_to(root) or not path.is_file():
        raise ValueError(f"artifact is missing or escapes release: {name}")
    actual = hashlib.sha256(path.read_bytes()).hexdigest()
    if actual != digest:
        raise ValueError(f"artifact digest mismatch: {name}")
    return path


def validate_catalog(build_registry: Path, catalog_path: Path, trusted_sha256: str) -> dict[int, dict]:
    """Return validated worlds, binding the catalog to a trusted release digest."""
    if not isinstance(trusted_sha256, str) or not DIGEST.fullmatch(trusted_sha256):
        raise ValueError("trusted release catalog digest is invalid")
    catalog_bytes = catalog_path.read_bytes()
    if hashlib.sha256(catalog_bytes).hexdigest() != trusted_sha256:
        raise ValueError("release catalog does not match trusted digest")
    default_name, builds = load_registry(build_registry)
    data = json.loads(catalog_bytes.decode("utf-8"))
    if not isinstance(data, dict) or type(data.get("schema_version")) is not int or data["schema_version"] != 1:
        raise ValueError("unsupported release catalog schema")
    entries = data.get("worlds")
    if not isinstance(entries, list) or len(entries) != len(builds):
        raise ValueError("release catalog must cover every build world")
    root = catalog_path.resolve().parent
    worlds: dict[int, dict] = {}
    names: set[str] = set()
    namespaces: set[str] = set()
    artifacts: set[Path] = set()
    player_contracts: set[tuple[int, str, str, int]] = set()
    location_sections: list[tuple[int, int, str]] = []
    for entry in entries:
        if not isinstance(entry, dict):
            raise ValueError("release world must be an object")
        name = _token(entry.get("name"), "world name")
        world_id = _integer(entry.get("world_id"), "world_id")
        namespace = _token(entry.get("save_namespace"), "save namespace")
        if (name not in builds or builds[name]["world_id"] != world_id
                or name in names or world_id in worlds or namespace in namespaces):
            raise ValueError(f"duplicate or mismatched release identity: {name}")
        names.add(name)
        namespaces.add(namespace)
        schema = _integer(entry.get("shared_player_schema"), "shared_player_schema")
        codec = _integer(entry.get("location_codec"), "location_codec")
        regional_schema = _integer(entry.get("regional_save_schema"), "regional_save_schema")
        if 0 in (schema, codec, regional_schema):
            raise ValueError(f"{name} needs positive schema versions")
        section_range = entry.get("owned_location_sections")
        if not isinstance(section_range, list) or len(section_range) != 2:
            raise ValueError(f"{name} needs an owned location-section range")
        first_section = _integer(section_range[0], f"{name} first location section")
        last_section = _integer(section_range[1], f"{name} last location section")
        if first_section > last_section:
            raise ValueError(f"{name} has a reversed location-section range")
        for other_first, other_last, other_name in location_sections:
            if first_section <= other_last and other_first <= last_section:
                raise ValueError(f"{name} location sections overlap {other_name}")
        location_sections.append((first_section, last_section, name))
        object_digest = entry.get("object_catalog_sha256")
        if not isinstance(object_digest, str) or not DIGEST.fullmatch(object_digest):
            raise ValueError(f"{name} has invalid object catalog digest")
        checked_artifacts = {}
        for prefix in ("rom", "bridge", "player_transfer"):
            artifact = _artifact(root, entry.get(f"{prefix}_path"), entry.get(f"{prefix}_sha256"))
            if artifact in artifacts:
                raise ValueError(f"artifact reused by multiple worlds: {artifact}")
            artifacts.add(artifact)
            checked_artifacts[prefix] = artifact
        try:
            bridge = json.loads(checked_artifacts["bridge"].read_text(encoding="utf-8"))
            bridge_rom = bridge["game_build"]["rom_sha256"]
            bridge_schema = bridge["save"]["schema_version"]
        except (OSError, UnicodeError, ValueError, TypeError, KeyError) as exc:
            raise ValueError(f"{name} has invalid bridge manifest") from exc
        if bridge_rom != entry["rom_sha256"] or type(bridge_schema) is not int or bridge_schema != schema:
            raise ValueError(f"{name} bridge manifest does not match ROM or shared-player schema")
        try:
            transfer = json.loads(checked_artifacts["player_transfer"].read_text(encoding="utf-8"))
            transfer_rom = transfer["rom_sha256"]
            transfer_version = transfer["schema_version"]
            transfer_digest = transfer["sha256"]
            transfer_address = transfer["address"]
            transfer_size = transfer["size"]
        except (OSError, UnicodeError, ValueError, TypeError, KeyError) as exc:
            raise ValueError(f"{name} has invalid player-transfer manifest") from exc
        if (transfer_rom != entry["rom_sha256"]
                or type(transfer_version) is not int or transfer_version != transfer_schema.SCHEMA_VERSION
                or type(transfer_address) is not int or type(transfer_size) is not int
                or transfer_size != transfer_schema.HEADER_SIZE + len(transfer_schema.EXPECTED_FIELDS) * transfer_schema.FIELD_SIZE
                or not isinstance(transfer_digest, str) or not DIGEST.fullmatch(transfer_digest)):
            raise ValueError(f"{name} player-transfer manifest does not match ROM or schema")
        try:
            payload = transfer_schema.read_rom_symbol(checked_artifacts["rom"], transfer_address, transfer_size)
            decoded = transfer_schema.parse_schema_payload(payload)
        except transfer_schema.ManifestError as exc:
            raise ValueError(f"{name} has invalid player-transfer ROM descriptor: {exc}") from exc
        if (transfer_schema.sha256_bytes(payload) != transfer_digest
                or transfer.get("field_count") != decoded["field_count"]
                or transfer.get("descriptor_size") != decoded["descriptor_size"]
                or transfer.get("spans") != decoded["spans"]
                or transfer.get("fields") != decoded["fields"]):
            raise ValueError(f"{name} player-transfer manifest does not match ROM descriptor")
        player_contracts.add((schema, object_digest, transfer_digest, codec))
        arrivals = entry.get("arrivals")
        portals = entry.get("portals")
        if not isinstance(arrivals, dict) or not isinstance(portals, list):
            raise ValueError(f"{name} needs arrivals and portals")
        for portal_id, arrival in arrivals.items():
            _token(portal_id, "arrival portal ID")
            if not isinstance(arrival, dict):
                raise ValueError(f"{name} has invalid arrival")
            coordinates = tuple(_integer(arrival.get(field), f"{name} {field}", 255)
                                for field in ("map_group", "map_number", "warp_id"))
            layout_id = _integer(arrival.get("map_layout_id"), f"{name} map_layout_id")
            if layout_id == 0:
                raise ValueError(f"{name} map_layout_id must be positive")
            template = _artifact(root, arrival.get("template_sav_path"),
                                 arrival.get("template_sav_sha256"))
            if template.stat().st_size not in (131072, 131088):
                raise ValueError(f"{name} {portal_id} has invalid arrival save size")
            _verify_arrival_save(template, arrival["template_sav_sha256"], *coordinates,
                                 layout_id)
        portals_by_id: dict[str, dict] = {}
        for portal in portals:
            if not isinstance(portal, dict):
                raise ValueError(f"{name} has invalid portal")
            portal_id = _token(portal.get("id"), "portal ID")
            if portal_id in portals_by_id:
                raise ValueError(f"{name} has duplicate portal {portal_id}")
            _integer(portal.get("destination_world_id"), "destination_world_id")
            _token(portal.get("arrival_portal_id"), "arrival_portal_id")
            _token(portal.get("return_portal_id"), "return_portal_id")
            portals_by_id[portal_id] = portal
        worlds[world_id] = {**entry, "portals_by_id": portals_by_id}
    if names != set(builds) or len(player_contracts) != 1:
        raise ValueError("release worlds disagree on build or shared-player contract")
    if sum(len(world["portals_by_id"]) for world in worlds.values()) > 256:
        raise ValueError("release has too many world portals")
    for source_id, source in worlds.items():
        for portal in source["portals_by_id"].values():
            destination_id = portal["destination_world_id"]
            if destination_id == source_id or destination_id not in worlds:
                raise ValueError("portal destination is unknown or self-referential")
            destination = worlds[destination_id]
            if portal["arrival_portal_id"] not in destination["arrivals"]:
                raise ValueError("portal arrival is missing")
            reverse = destination["portals_by_id"].get(portal["return_portal_id"])
            if reverse is None or reverse["destination_world_id"] != source_id:
                raise ValueError("portal has no reciprocal return")
            if reverse["return_portal_id"] != portal["id"]:
                raise ValueError("portal return does not name the source portal")
            if reverse["arrival_portal_id"] not in source["arrivals"]:
                raise ValueError("return arrival is missing")
    reachable = {builds[default_name]["world_id"]}
    frontier = list(reachable)
    while frontier:
        source_id = frontier.pop()
        for portal in worlds[source_id]["portals"]:
            destination_id = portal["destination_world_id"]
            if destination_id not in reachable:
                reachable.add(destination_id)
                frontier.append(destination_id)
    if reachable != set(worlds):
        raise ValueError("release has a world unreachable from the default world")
    return worlds


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("build_registry", type=Path)
    parser.add_argument("release_catalog", type=Path)
    parser.add_argument("--trusted-sha256", required=True,
                        help="catalog digest from the separately trusted release manifest")
    args = parser.parse_args()
    try:
        worlds = validate_catalog(args.build_registry, args.release_catalog, args.trusted_sha256)
    except (OSError, UnicodeError, ValueError, json.JSONDecodeError) as exc:
        parser.exit(1, f"ROM release catalog: {exc}\n")
    print(f"Validated {len(worlds)} ROM worlds and their travel portals")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
