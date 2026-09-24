"""Materialize authenticated tileset source inputs in an isolated build tree.

The output contains source PNG/PAL/BIN files. The ROM build's normal graphics
rules convert them to the INCBIN paths listed in the tileset preview.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

from tools.cormoria import import_world, register_tilesets

ROOT = Path(__file__).resolve().parents[2]


class MaterializationError(ValueError):
    """A tileset plan or staged input is missing, changed, or unsafe."""


def source_target(recipe: dict[str, str]) -> str:
    target = recipe["target"]
    conversion = recipe["conversion"]
    suffixes = {
        "identity": ("", ""),
        "pal->gbapal": (".gbapal", ".pal"),
        "png->4bpp": (".4bpp", ".png"),
        "png->4bpp->lz": (".4bpp.lz", ".png"),
    }
    if conversion not in suffixes:
        raise MaterializationError(f"unsupported asset conversion: {conversion}")
    output_suffix, input_suffix = suffixes[conversion]
    if output_suffix and not target.endswith(output_suffix):
        raise MaterializationError(f"conversion target suffix drifted: {target}")
    return target.removesuffix(output_suffix) + input_suffix


def verify_donor_outputs(output: Path, plan: dict, donor_build: Path) -> int:
    donor_build = donor_build.resolve(strict=True)
    mismatches: list[str] = []
    for recipe in plan["recipes"]:
        target = (output / import_world.safe_relative(recipe["target"])).resolve()
        expected = (donor_build / import_world.safe_relative(recipe["requested"])).resolve()
        if (not target.is_relative_to(output) or not expected.is_relative_to(donor_build)
                or not target.is_file() or not expected.is_file()
                or target.read_bytes() != expected.read_bytes()):
            mismatches.append(recipe["requested"])
    if mismatches:
        raise MaterializationError(f"{len(mismatches)} donor output mismatches: {', '.join(mismatches[:5])}")
    return len(plan["recipes"])


def materialize(stage: Path, general_foundation: Path, donor_rules: Path, donor_anims: Path,
                preview: Path, output: Path, gfx_tool: Path | None = None,
                source_only: bool = False, donor_build: Path | None = None,
                root: Path = ROOT) -> int:
    stage = stage.resolve(strict=True)
    general_foundation = general_foundation.resolve(strict=True)
    donor_rules = donor_rules.resolve(strict=True)
    donor_anims = donor_anims.resolve(strict=True)
    preview = preview.resolve(strict=True)
    output = output.resolve()
    root = root.resolve()
    if (output.exists() or output == root or output.is_relative_to(root)
            or output.is_relative_to(stage) or output.is_relative_to(general_foundation)
            or output.is_relative_to(preview)):
        raise MaterializationError("output must be fresh and outside source and inputs")
    if not source_only:
        if gfx_tool is None:
            raise MaterializationError("explicit gfx tool is required for donor-equivalent conversions")
        gfx_tool = gfx_tool.resolve(strict=True)
    expected = register_tilesets.render(stage, general_foundation, donor_rules, donor_anims, root)
    for name, content in expected.items():
        if (preview / name).read_bytes() != content:
            raise MaterializationError(f"tileset preview differs from authenticated stage: {name}")
    plan = json.loads(expected["asset_plan.json"])
    seen: set[str] = set()
    pending: list[tuple[Path, bytes]] = []
    for recipe in plan["recipes"]:
        relative = source_target(recipe)
        safe = import_world.safe_relative(relative)
        if not relative.startswith("data/tilesets/cormoria/") or relative in seen:
            raise MaterializationError(f"asset source target is unsafe or duplicated: {relative}")
        seen.add(relative)
        stage_relative = import_world.safe_relative(recipe["source"])
        origin = {"stage": stage, "general-foundation": general_foundation}.get(recipe.get("origin"))
        if origin is None:
            raise MaterializationError(f"unknown recipe origin: {recipe.get('origin')}")
        source = (origin / stage_relative).resolve()
        if not source.is_relative_to(origin) or not source.is_file():
            raise MaterializationError(f"staged asset is missing: {recipe['source']}")
        content = source.read_bytes()
        if hashlib.sha256(content).hexdigest() != recipe["source_sha256"]:
            raise MaterializationError(f"staged asset hash changed: {recipe['source']}")
        pending.append((safe, content))
    output.mkdir(parents=True)
    for relative, content in pending:
        destination = output / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(content)
    if not source_only:
        for recipe in plan["recipes"]:
            conversion = recipe["conversion"]
            if conversion == "identity":
                continue
            source = output / source_target(recipe)
            target = output / import_world.safe_relative(recipe["target"])
            target.parent.mkdir(parents=True, exist_ok=True)
            intermediate = target.with_suffix("") if conversion == "png->4bpp->lz" else target
            arguments = [str(gfx_tool), str(source), str(intermediate)]
            if "num_tiles" in recipe:
                if conversion != "png->4bpp->lz" and conversion != "png->4bpp":
                    raise MaterializationError(f"tile-count option on non-tile asset: {recipe['target']}")
                arguments += ["-num_tiles", str(recipe["num_tiles"]), "-Wnum_tiles"]
            result = subprocess.run(arguments, capture_output=True, check=False)
            if result.returncode:
                raise MaterializationError(f"gfx conversion failed: {recipe['target']}: "
                                           + result.stderr.decode(errors="replace")[-400:])
            if conversion == "png->4bpp->lz":
                result = subprocess.run([str(gfx_tool), str(intermediate), str(target)],
                                        capture_output=True, check=False)
                if result.returncode:
                    raise MaterializationError(f"gfx compression failed: {recipe['target']}: "
                                               + result.stderr.decode(errors="replace")[-400:])
    (output / "tilesets.c").write_bytes(expected["tilesets.c"])
    (output / "asset_plan.json").write_bytes(expected["asset_plan.json"])
    if donor_build is not None and not source_only:
        verify_donor_outputs(output, plan, donor_build)
    return len(pending)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stage", required=True, type=Path)
    parser.add_argument("--general-foundation", required=True, type=Path)
    parser.add_argument("--donor-rules", required=True, type=Path)
    parser.add_argument("--donor-anims", required=True, type=Path)
    parser.add_argument("--preview", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--gfx-tool", required=True, type=Path)
    parser.add_argument("--donor-build", type=Path)
    args = parser.parse_args(argv)
    try:
        count = materialize(args.stage, args.general_foundation, args.donor_rules, args.donor_anims,
                            args.preview, args.output, args.gfx_tool, donor_build=args.donor_build)
    except (OSError, ValueError, KeyError) as exc:
        print(f"Cormoria tileset materialization: {exc}", file=sys.stderr)
        return 1
    print(f"Materialized {count} authenticated tileset sources at {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
