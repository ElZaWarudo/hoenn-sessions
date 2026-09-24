"""Import Cormoria trainer presentation from the pinned Dreamstone Git commit.

The donor working tree is deliberately ignored: every source is read from an
immutable Git object.  Existing-world class and portrait entries stay intact.
"""

from __future__ import annotations

import argparse
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DONOR_COMMIT = "f7997186345885bfa23a170e5f573851fc034b9b"
CLASSES = (
    "ACE_ROOKIE", "ARTIST", "BACKPACKER", "BUG_CATCHER_F", "BUILDER",
    "BURGLAR", "CADET", "CONTENDER", "COOL_GIRL", "ELECTRICIAN",
    "EMPLOYEE", "FIREFIGHTER", "KOHLA_FINAL", "MODEL", "RUE", "SKIIER",
    "SOMBER_ADMIN", "TEAM_SOMBER", "WAITRESS",
)
PICS = (
    "BACKPACKER", "BUG_CATCHER_F", "BUILDER", "BURGLAR", "CADET_F",
    "CADET_M", "CHAMPION_LYNCH", "COOL_GIRL", "COOL_GUY", "ELECTRICIAN",
    "EMPLOYEE", "FIREFIGHTER", "GUBUKING", "JANIA_ARTIST", "LEADER_ARIANA",
    "LEADER_CARONA", "LEADER_GLORIA", "LEADER_INGER", "LEADER_JANIA",
    "LEADER_RAAZI", "LEADER_VINIEL", "MODEL", "QWILSQUAD_BOSS", "RUE",
    "SHUBUBU", "SKIIER_F", "SOMBER_ADMIN_MELEA", "SOMBER_ADMIN_MOXIE",
    "SOMBER_GRUNT_DUO", "SOMBER_GRUNT_F", "SOMBER_GRUNT_M", "WAITRESS",
)
BEGIN = "/* BEGIN PINNED CORMORIA TRAINER PRESENTATION"
END = "/* END PINNED CORMORIA TRAINER PRESENTATION"


def donor_object(donor: Path, path: str) -> bytes:
    return subprocess.check_output(
        ["git", "-C", str(donor), "show", f"{DONOR_COMMIT}:{path}"],
        stderr=subprocess.PIPE,
    )


def replace_block(path: Path, label: str, content: str, anchor: str, check: bool, last: bool = False) -> None:
    original = path.read_text(encoding="utf-8")
    start_marker = f"{BEGIN} {label} */"
    end_marker = f"{END} {label} */"
    if original.count(start_marker) != original.count(end_marker) or original.count(start_marker) > 1:
        raise ValueError(f"invalid generated block: {path}")
    if start_marker in original:
        start = original.index(start_marker)
        stop = original.index(end_marker, start) + len(end_marker)
        expected = original[:start] + content + original[stop:]
    else:
        if (not last and original.count(anchor) != 1) or (last and anchor not in original):
            raise ValueError(f"missing/ambiguous anchor in {path}: {anchor}")
        position = original.rfind(anchor) if last else original.index(anchor)
        prefix = "" if original[:position].endswith("\n") else "\n"
        expected = original[:position] + prefix + content + "\n" + original[position:]
    if check:
        if original != expected:
            raise ValueError(f"generated trainer presentation drift: {path}")
    elif original != expected:
        path.write_text(expected, encoding="utf-8", newline="\n")


def block(label: str, lines: list[str]) -> str:
    return "\n".join((f"{BEGIN} {label} */", *lines, f"{END} {label} */"))


def source_entry(source: str, symbol: str) -> str:
    match = re.search(
        rf"^\s*\[TRAINER_CLASS_{symbol}\]\s*=\s*(\{{[^\n]+\}}),?",
        source,
        re.MULTILINE,
    )
    if not match:
        raise ValueError(f"missing donor trainer class: {symbol}")
    return match.group(1).rstrip(",")


def picture_mapping(source: str, symbol: str) -> tuple[str, str]:
    match = re.search(
        rf"TRAINER_SPRITE\(\s*TRAINER_PIC_{symbol}\s*,\s*(\w+)\s*,\s*(\w+)",
        source,
    )
    if not match:
        raise ValueError(f"missing donor trainer portrait mapping: {symbol}")
    front, palette = match.groups()
    paths = []
    for name in (front, palette):
        definition = re.search(
            rf'const u32 {name}\[\] = INCBIN_U32\("([^"]+)"\)', source
        )
        if not definition:
            raise ValueError(f"missing donor graphic definition: {name}")
        paths.append(definition.group(1))
    image_path, palette_path = paths
    if not image_path.endswith(".4bpp.lz") or not palette_path.endswith(".gbapal.lz"):
        raise ValueError(f"unsupported donor palette semantics: {symbol}")
    return (
        image_path.removesuffix(".4bpp.lz") + ".png",
        palette_path.removesuffix(".gbapal.lz") + ".png",
    )


def run(donor: Path, root: Path = ROOT, check: bool = False) -> None:
    resolved = subprocess.check_output(
        ["git", "-C", str(donor), "rev-parse", "HEAD"], stderr=subprocess.PIPE
    ).decode().strip()
    if resolved != DONOR_COMMIT:
        raise ValueError(f"donor HEAD is not pinned commit: {resolved}")
    roster = (root / "src/data/cormoria/trainers.h").read_text(encoding="utf-8")
    constants = root / "include/constants/trainers.h"
    battle = root / "src/battle_main.c"
    graphics = root / "src/data/graphics/trainers.h"
    donor_battle = donor_object(donor, "src/battle_main.c").decode("utf-8")
    donor_graphics = donor_object(donor, "src/data/graphics/trainers.h").decode("utf-8")
    donor_files = set(subprocess.check_output(
        ["git", "-C", str(donor), "ls-tree", "-r", "--name-only", DONOR_COMMIT, "graphics/trainers/my_trainers"],
        stderr=subprocess.PIPE,
    ).decode().splitlines())
    for prefix, expected in (("CLASS", CLASSES), ("PIC", PICS)):
        used = set(re.findall(rf"TRAINER_{prefix}_[A-Z0-9_]+", roster))
        host = constants.read_text(encoding="utf-8")
        host = re.sub(r"/\* BEGIN PINNED CORMORIA TRAINER PRESENTATION [A-Z_]+ \*/.*?/\* END PINNED CORMORIA TRAINER PRESENTATION [A-Z_]+ \*/", "", host, flags=re.DOTALL)
        declared = set(re.findall(rf"TRAINER_{prefix}_[A-Z0-9_]+", host))
        missing = {name.removeprefix(f"TRAINER_{prefix}_") for name in used - declared}
        if missing != set(expected):
            raise ValueError(f"Cormoria {prefix.lower()} roster changed: {sorted(missing ^ set(expected))}")

    replace_block(
        constants,
        "PIC_IDS",
        block("PIC_IDS", [f"    TRAINER_PIC_{name}," for name in PICS]),
        "    TRAINER_PIC_COUNT,",
        check,
    )
    replace_block(
        constants,
        "CLASS_IDS",
        block("CLASS_IDS", [f"    TRAINER_CLASS_{name}," for name in CLASSES]),
        "    TRAINER_CLASS_COUNT,",
        check,
    )
    replace_block(
        battle,
        "CLASS_DATA",
        block("CLASS_DATA", [f"    [TRAINER_CLASS_{name}] = {source_entry(donor_battle, name)}," for name in CLASSES]),
        "\n};\n\nstatic void (*const sTurnActionsFuncsTable",
        check,
    )

    definitions = []
    mappings = []
    for name in PICS:
        source_png, palette_png = picture_mapping(donor_graphics, name)
        palette_source = palette_png
        if palette_source not in donor_files:
            palette_source = palette_source.removesuffix(".png") + ".pal"
        if source_png not in donor_files or palette_source not in donor_files:
            raise ValueError(f"missing donor portrait/palette source: {name}")
        for source_path in {source_png, palette_source}:
            destination = root / "graphics/trainers/cormoria" / Path(source_path).name
            expected = donor_object(donor, source_path)
            if source_path.endswith(".png") and expected[:8] != b"\x89PNG\r\n\x1a\n":
                raise ValueError(f"donor portrait is not PNG: {source_path}")
            if check:
                if not destination.is_file() or destination.read_bytes() != expected:
                    raise ValueError(f"Cormoria portrait drift: {destination}")
            else:
                destination.parent.mkdir(parents=True, exist_ok=True)
                if not destination.exists() or destination.read_bytes() != expected:
                    destination.write_bytes(expected)
        stem = (Path("graphics/trainers/cormoria") / Path(source_png).stem).as_posix()
        palette_stem = (Path("graphics/trainers/cormoria") / Path(palette_png).stem).as_posix()
        front = f"gCormoriaTrainerFrontPic_{name}"
        palette = f"gCormoriaTrainerPalette_{name}"
        definitions.extend((
            f'const u32 {front}[] = INCBIN_U32("{stem}.4bpp.smol");',
            f'const u16 {palette}[] = INCBIN_U16("{palette_stem}.gbapal");',
        ))
        mappings.extend((
            f"    [TRAINER_PIC_{name}] =",
            "    {",
            f"        .frontPic = TRAINER_FRONT_PIC({front}, {palette}),",
            "    },",
        ))
    replace_block(graphics, "PIC_DATA", block("PIC_DATA", definitions), "const struct TrainerPicInfo gTrainerPicInfo[TRAINER_PIC_COUNT] =", check)
    replace_block(graphics, "PIC_MAPPINGS", block("PIC_MAPPINGS", mappings), "\n};", check, last=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--donor", type=Path, required=True)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    try:
        run(args.donor, args.root, args.check)
    except (OSError, ValueError, subprocess.CalledProcessError) as exc:
        parser.exit(2, f"Cormoria trainer presentation rejected: {exc}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
