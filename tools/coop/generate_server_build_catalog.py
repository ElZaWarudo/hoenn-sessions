#!/usr/bin/env python3
"""Derive the server's pinned build catalog from a validated ROM release."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
import tempfile
from pathlib import Path

if not __package__:
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from tools.rom_release_catalog import DIGEST, validate_catalog
from tools.coop import player_transfer_manifest as transfer_schema

BUILD_ID = re.compile(r"[A-Za-z0-9._+:-]{1,128}\Z")
MGBA_VERSION = re.compile(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\Z")
EMULATOR_FIELDS = frozenset(("name", "version", "build_id", "source_commit", "platform",
                             "variant", "archive_sha256", "executable_sha256"))


def _manifest_identity(world: dict, release_root: Path) -> dict:
    path = (release_root / world["bridge_path"]).resolve()
    raw = path.read_bytes()
    if hashlib.sha256(raw).hexdigest() != world["bridge_sha256"]:
        raise ValueError(f"{world['name']} bridge changed after release validation")
    try:
        manifest = json.loads(raw)
        build = manifest["game_build"]
        bridge = manifest["net_bridge"]
        emulator = manifest["emulator"]
        build_id = build["id"]
        rom_sha256 = build["rom_sha256"]
        abi = bridge["abi_version"]
        protocol = bridge["game_protocol_version"]
        version = emulator["version"]
    except (KeyError, TypeError, ValueError) as exc:
        raise ValueError(f"{world['name']} has incomplete bridge build identity") from exc
    if (not isinstance(build_id, str) or not BUILD_ID.fullmatch(build_id)
            or not isinstance(rom_sha256, str) or not DIGEST.fullmatch(rom_sha256)
            or rom_sha256 != world["rom_sha256"]
            or type(abi) is not int or abi != 1
            or type(protocol) is not int or protocol != 1):
        raise ValueError(f"{world['name']} has invalid bridge build identity")
    if (not isinstance(emulator, dict) or set(emulator) != EMULATOR_FIELDS
            or emulator["name"] != "mGBA"
            or not isinstance(version, str) or not MGBA_VERSION.fullmatch(version)
            or any(not isinstance(emulator[field], str) or not emulator[field]
                   for field in EMULATOR_FIELDS)):
        raise ValueError(f"{world['name']} has incomplete emulator identity")
    for field in ("archive_sha256", "executable_sha256"):
        if not DIGEST.fullmatch(emulator[field]):
            raise ValueError(f"{world['name']} has invalid emulator {field}")
    return {
        "game_build_id": build_id,
        "rom_sha256": rom_sha256,
        "mgba_version": version,
        "bridge_abi": abi,
        "protocol_version": protocol,
    }


def generate(build_registry: Path, release_catalog: Path, trusted_sha256: str) -> tuple[bytes, str]:
    """Return Rust-compatible canonical JSON bytes and their SHA-256 digest."""
    worlds = validate_catalog(build_registry, release_catalog, trusted_sha256)
    if len(worlds) > 32:
        raise ValueError("server build catalog supports at most 32 worlds")
    identities = []
    build_ids: set[str] = set()
    rom_digests: set[str] = set()
    descriptor_digests: set[str] = set()
    descriptor_payloads: set[bytes] = set()
    for world_id, world in sorted(worlds.items()):
        build = _manifest_identity(world, release_catalog.resolve().parent)
        if build["game_build_id"] in build_ids or build["rom_sha256"] in rom_digests:
            raise ValueError(f"duplicate server build identity for world {world_id}")
        build_ids.add(build["game_build_id"])
        rom_digests.add(build["rom_sha256"])
        transfer_path = release_catalog.resolve().parent / world["player_transfer_path"]
        transfer_bytes = transfer_path.read_bytes()
        if hashlib.sha256(transfer_bytes).hexdigest() != world["player_transfer_sha256"]:
            raise ValueError(f"{world['name']} transfer manifest changed after release validation")
        transfer = json.loads(transfer_bytes)
        descriptor_digests.add(transfer["sha256"])
        descriptor_payloads.add(transfer_schema.read_rom_symbol(
            release_catalog.resolve().parent / world["rom_path"],
            transfer["address"], transfer["size"]))
        identities.append({
            "world_id": world_id,
            "build": build,
            "arrivals": [{
                "id": arrival_id,
                "map_group": arrival["map_group"],
                "map_number": arrival["map_number"],
                "warp_id": arrival["warp_id"],
                "template_sav_path": arrival["template_sav_path"],
                "template_sav_sha256": arrival["template_sav_sha256"],
            } for arrival_id, arrival in sorted(world["arrivals"].items())],
            "portals": sorted(({
                "id": portal["id"],
                "destination_world_id": portal["destination_world_id"],
                "arrival_portal_id": portal["arrival_portal_id"],
                "return_portal_id": portal["return_portal_id"],
            } for portal in world["portals"]), key=lambda portal: portal["id"]),
        })
    if len(descriptor_digests) != 1 or len(descriptor_payloads) != 1:
        raise ValueError("server worlds disagree on the shared player descriptor")
    payload = {
        "schema_version": 3,
        "shared_player_descriptor_sha256": descriptor_digests.pop(),
        "shared_player_descriptor_hex": descriptor_payloads.pop().hex(),
        "worlds": identities,
    }
    data = (json.dumps(payload, sort_keys=True, separators=(",", ":"), ensure_ascii=True)
            + "\n").encode("ascii")
    return data, hashlib.sha256(data).hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("build_registry", type=Path)
    parser.add_argument("release_catalog", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--trusted-sha256", required=True,
                        help="digest pinned by separately trusted release packaging")
    args = parser.parse_args()
    try:
        data, digest = generate(args.build_registry, args.release_catalog, args.trusted_sha256)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.NamedTemporaryFile(mode="wb", dir=args.output.parent, delete=False) as file:
            temporary = Path(file.name)
            file.write(data)
        os.replace(temporary, args.output)
    except (OSError, UnicodeError, ValueError, json.JSONDecodeError) as exc:
        parser.exit(1, f"server build catalog: {exc}\n")
    print(digest)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
