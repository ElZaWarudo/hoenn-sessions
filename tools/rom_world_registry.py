"""Validate ROM build worlds and resolve a selected world's build metadata."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


def _integer(value: object, label: str, maximum: int) -> int:
    if type(value) is not int or not 1 <= value <= maximum:
        raise ValueError(f"{label} must be an integer from 1 through {maximum}")
    return value


def load_registry(path: Path) -> tuple[str, dict[str, dict]]:
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict) or type(data.get("schema_version")) is not int or data["schema_version"] != 1:
        raise ValueError("unsupported ROM world registry schema")
    entries = data.get("worlds")
    if not isinstance(entries, list) or not entries:
        raise ValueError("ROM world registry needs a nonempty worlds list")

    worlds: dict[str, dict] = {}
    ids: set[int] = set()
    bits: set[int] = set()
    build_names: set[str] = set()
    game_codes: set[str] = set()
    for entry in entries:
        if not isinstance(entry, dict):
            raise ValueError("ROM world entry must be an object")
        name = entry.get("name")
        if not isinstance(name, str) or not re.fullmatch(r"[a-z][a-z0-9_]*", name) or name == "shared":
            raise ValueError(f"invalid ROM world name {name!r}")
        world_id = _integer(entry.get("world_id"), f"{name} world_id", 65535)
        bit = _integer(entry.get("build_bit"), f"{name} build_bit", 1 << 30)
        if bit & (bit - 1):
            raise ValueError(f"{name} build_bit must be one bit")
        if name in worlds or world_id in ids or bit in bits:
            raise ValueError(f"duplicate ROM world name, world_id, or build_bit: {name}")
        ids.add(world_id)
        bits.add(bit)

        game_version = entry.get("game_version")
        if game_version is not None and game_version not in ("EMERALD", "FIRERED", "LEAFGREEN"):
            raise ValueError(f"{name} has invalid game_version")
        map_version = entry.get("map_version")
        if map_version is not None and map_version not in ("emerald", "firered"):
            raise ValueError(f"{name} has invalid map_version")
        build_name = entry.get("build_name")
        if build_name is not None and (not isinstance(build_name, str) or not re.fullmatch(r"[a-z0-9][a-z0-9-]*", build_name)):
            raise ValueError(f"{name} has invalid build_name")
        if build_name is not None:
            if build_name in build_names or (name != data.get("default_world") and build_name in ("emerald", "firered", "leafgreen")):
                raise ValueError(f"{name} reuses a ROM build_name")
            build_names.add(build_name)
        title = entry.get("title")
        if title is not None and (not isinstance(title, str) or not re.fullmatch(r"[A-Z0-9 ]{1,12}", title)):
            raise ValueError(f"{name} has invalid title")
        game_code = entry.get("game_code")
        if game_code is not None and (not isinstance(game_code, str) or not re.fullmatch(r"[A-Z0-9]{4}", game_code)):
            raise ValueError(f"{name} has invalid game_code")
        if game_code is not None:
            if game_code in game_codes or (name != data.get("default_world") and game_code in ("BPEE", "BPRE", "BPGE")):
                raise ValueError(f"{name} reuses a ROM game_code")
            game_codes.add(game_code)
        if name != data.get("default_world") and any(entry.get(key) is None for key in ("game_version", "map_version", "build_name", "title", "game_code")):
            raise ValueError(f"{name} needs complete build metadata")
        if name == data.get("default_world") and any(entry.get(key) is not None for key in ("game_version", "map_version", "build_name", "title", "game_code")):
            raise ValueError(f"{name} must inherit its base game metadata")
        worlds[name] = entry

    default = data.get("default_world")
    if not isinstance(default, str) or default not in worlds:
        raise ValueError("default_world must name a registered world")
    if worlds[default]["build_bit"] != 1:
        raise ValueError("default_world must keep build_bit 1 for legacy maps")
    return default, worlds


def resolve(path: Path, selector: str, game_version: str) -> str:
    _, worlds = load_registry(path)
    matches = [entry for name, entry in worlds.items() if selector == name or selector == str(entry["build_bit"])]
    if len(matches) != 1:
        raise ValueError(f"unknown or ambiguous ROM_WORLD {selector!r}")
    entry = matches[0]
    required_version = entry["game_version"]
    if required_version is not None and required_version != game_version:
        raise ValueError(f"{entry['name']} requires GAME_VERSION={required_version}")
    fields = ("build_bit", "build_name", "title", "game_code", "map_version")
    return " ".join(
        str(entry[key]).replace(" ", "~") if entry[key] is not None else "-"
        for key in fields
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("registry", type=Path)
    parser.add_argument("--selectors", action="store_true")
    parser.add_argument("selector", nargs="?")
    parser.add_argument("game_version", nargs="?")
    args = parser.parse_args()
    try:
        if args.selectors:
            _, worlds = load_registry(args.registry)
            print(" ".join([*worlds, *(str(entry["build_bit"]) for entry in worlds.values())]))
        elif args.selector and args.game_version:
            print(resolve(args.registry, args.selector, args.game_version))
        else:
            raise ValueError("selector and game_version are required")
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        parser.exit(1, f"ROM world registry: {exc}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
