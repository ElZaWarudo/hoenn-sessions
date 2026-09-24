"""Install authenticated Cormoria Game Corner animation sources in isolated paths."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

from tools.cormoria import import_world


ROOT = Path(__file__).resolve().parents[2]
DONOR_PREFIX = "data/tilesets/secondary/mauville_game_corner/anim/lights"
TARGET_PREFIX = "data/tilesets/cormoria/secondary/mauville_game_corner/anim/lights"


class AnimationSourceError(ValueError):
    pass


def authenticated_frames(stage: Path, root: Path = ROOT) -> dict[Path, bytes]:
    region, _, sources = import_world.load_manifests(root)
    stage_info = json.loads((stage / "staging_manifest.json").read_text(encoding="utf-8"))
    if (stage_info.get("manifest_sha256") != import_world.PINNED_SHA256
            or stage_info.get("provenance") != region["provenance"]):
        raise AnimationSourceError("stage manifest provenance drifted")
    staged = {row["path"]: row for row in stage_info["files"]}
    pinned = {row["path"]: row for row in sources["files"]}
    if len(staged) != len(stage_info["files"]) or len(pinned) != len(sources["files"]):
        raise AnimationSourceError("duplicate source manifest record")
    result = {}
    for frame in range(6):
        name = f"light_anim_{frame}.png"
        relative = f"{DONOR_PREFIX}/{name}"
        stage_relative = f"source/{relative}"
        source = stage / import_world.safe_relative(stage_relative)
        if not source.is_file() or not source.resolve().is_relative_to(stage.resolve()):
            raise AnimationSourceError(f"missing staged animation frame: {relative}")
        data = source.read_bytes()
        digest = hashlib.sha256(data).hexdigest()
        for record in (staged.get(stage_relative), pinned.get(relative)):
            if record is None or record["bytes"] != len(data) or record["sha256"] != digest:
                raise AnimationSourceError(f"animation source hash mismatch: {relative}")
        result[Path(TARGET_PREFIX) / name] = data
    return result


def install(stage: Path, root: Path = ROOT) -> None:
    frames = authenticated_frames(stage, root)
    resolved_root = root.resolve()
    for relative, data in frames.items():
        target = root / relative
        if not target.resolve().is_relative_to(resolved_root):
            raise AnimationSourceError(f"animation target escapes repository: {relative}")
        if target.exists() and target.read_bytes() != data:
            raise AnimationSourceError(f"existing animation source differs: {relative}")
    for relative, data in frames.items():
        target = root / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stage", required=True, type=Path)
    args = parser.parse_args()
    install(args.stage)


if __name__ == "__main__":
    main()
