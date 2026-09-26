#!/usr/bin/env python3
"""Build and verify every registered world for the non-deploying CI workflow."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from rom_world_registry import load_registry  # noqa: E402

try:
    from .experience_table_manifest import require_same_experience_tables
    from .map_binding_manifest import verify_arrival
    from .object_contract_manifest import require_same_scalar_tables
    from .player_transfer_manifest import require_same_transfer_schema
except ImportError:
    from experience_table_manifest import require_same_experience_tables
    from map_binding_manifest import verify_arrival
    from object_contract_manifest import require_same_scalar_tables
    from player_transfer_manifest import require_same_transfer_schema


BASE_BUILD = ("emerald", "BPEE")
MANIFESTS = {
    "bridge": ("tools/generate_bridge_manifest.py", "bridge_manifest.json"),
    "transfer": ("tools/coop/player_transfer_manifest.py", "player_transfer_manifest.json"),
    "experience": ("tools/coop/experience_table_manifest.py", "experience_table_manifest.json"),
    "map_binding": ("tools/coop/map_binding_manifest.py", "map_binding_manifest.json"),
    "scalar": ("tools/coop/object_contract_manifest.py", "object_scalar_manifest.json"),
}

def load_arrivals(path: Path, world_names: set[str]) -> dict[str, list[tuple[int, int, int]]]:
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict) or type(data.get("schema_version")) is not int or data["schema_version"] != 1:
        raise ValueError("unsupported ROM world arrivals schema")
    entries = data.get("worlds")
    if not isinstance(entries, dict) or set(entries) != world_names:
        raise ValueError("ROM world arrivals must declare exactly the registered worlds")
    arrivals = {}
    for name, locations in entries.items():
        if not isinstance(locations, list) or not locations:
            raise ValueError(f"{name} needs at least one smoke arrival")
        checks = []
        for location in locations:
            if not isinstance(location, dict) or set(location) != {"map_group", "map_number", "map_layout_id"}:
                raise ValueError(f"{name} has invalid smoke arrival metadata")
            group, number, layout_id = (location[key] for key in
                                        ("map_group", "map_number", "map_layout_id"))
            if (type(group) is not int or not 0 <= group <= 127
                    or type(number) is not int or not 0 <= number <= 127
                    or type(layout_id) is not int or not 1 <= layout_id <= 65535):
                raise ValueError(f"{name} has invalid smoke arrival coordinates")
            checks.append((group, number, layout_id))
        arrivals[name] = checks
    return arrivals


def world_plan(registry: Path, base_version: str = "EMERALD") -> list[tuple[str, str, str, str]]:
    default, worlds = load_registry(registry)
    if not 2 <= len(worlds) <= 16:
        raise ValueError("multiworld CI requires 2 through 16 registered worlds")
    if base_version != "EMERALD":
        raise ValueError("multiworld CI requires GAME_VERSION=EMERALD for the default world")
    base_build, base_code = BASE_BUILD
    return [
        (name,
         base_version if name == default else entry["game_version"],
         base_build if name == default else entry["build_name"],
         base_code if name == default else entry["game_code"])
        for name, entry in worlds.items()
    ]


def build_worlds(plan: list[tuple[str, str, str, str]], dist: Path) -> None:
    for name, version, build_name, _ in plan:
        print(f"Building {name} ({version}, {build_name})", flush=True)
        subprocess.run(["make", f"-j{os.cpu_count() or 2}", "-O",
                        f"ROM_WORLD={name}", f"GAME_VERSION={version}", "all"], check=True)
        root = dist / name
        root.mkdir(parents=True, exist_ok=True)
        rom = Path(f"poke{build_name}.gba")
        elf = Path(f"poke{build_name}.elf")
        shutil.copyfile(rom, root / "game.gba")
        shutil.copyfile(elf, root / "game.elf")
        for kind, (script, output) in MANIFESTS.items():
            command = [sys.executable, script, "--elf", str(elf), "--rom", str(rom),
                       "--manifest", str(root / output)]
            if kind == "bridge":
                command += ["--lua", str(root / "generated_addresses.lua")]
            subprocess.run(command, check=True)


def verify_worlds(plan: list[tuple[str, str, str, str]], dist: Path, arrivals_path: Path) -> None:
    arrivals = load_arrivals(arrivals_path, {name for name, _, _, _ in plan})
    transfers: dict[str, dict] = {}
    experiences: dict[str, dict] = {}
    scalars: dict[str, dict] = {}
    digests: set[str] = set()
    for name, _, _, game_code in plan:
        root = dist / name
        rom = (root / "game.gba").read_bytes()
        if not 0 < len(rom) <= 32 * 1024 * 1024:
            raise ValueError(f"{name} exceeds the GBA ROM window")
        if rom[0xAC:0xB0] != game_code.encode("ascii"):
            raise ValueError(f"{name} has the wrong GBA game code")
        digest = hashlib.sha256(rom).hexdigest()
        manifests = {kind: json.loads((root / output).read_text(encoding="utf-8"))
                     for kind, (_, output) in MANIFESTS.items()}
        for kind, manifest in manifests.items():
            actual = manifest["game_build"]["rom_sha256"] if kind == "bridge" else manifest["rom_sha256"]
            if actual != digest:
                raise ValueError(f"{name} {kind}/ROM mismatch")
        for coordinates in arrivals[name]:
            verify_arrival(rom, manifests["map_binding"], *coordinates)
        transfers[name] = manifests["transfer"]
        experiences[name] = manifests["experience"]
        scalars[name] = manifests["scalar"]
        digests.add(digest)
        print(f"{name}: {len(rom)} bytes, sha256={digest}")
    if len(digests) != len(plan):
        raise ValueError("world ROMs are identical")
    require_same_transfer_schema(transfers)
    require_same_experience_tables(experiences)
    require_same_scalar_tables(scalars)
    print(f"{len(plan)} world ROMs share transfer, experience, and object scalar/pointer/text contracts")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("build", "verify"))
    parser.add_argument("--registry", type=Path, default=Path("data/rom_worlds.json"))
    parser.add_argument("--dist", type=Path, default=Path("dist/multiworld"))
    parser.add_argument("--arrivals", type=Path, default=Path("data/rom_world_arrivals.json"))
    parser.add_argument("--base-version", default=os.environ.get("GAME_VERSION", "EMERALD"))
    args = parser.parse_args()
    try:
        plan = world_plan(args.registry, args.base_version)
        if args.action == "build":
            build_worlds(plan, args.dist)
        else:
            verify_worlds(plan, args.dist, args.arrivals)
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as exc:
        parser.exit(1, f"multiworld CI: {exc}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
