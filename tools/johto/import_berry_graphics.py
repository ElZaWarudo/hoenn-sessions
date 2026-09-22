#!/usr/bin/env python3
"""Import the pinned donor berry tree images and generated renderer tables."""
from __future__ import annotations
import argparse, hashlib, json, struct
from pathlib import Path

CONTRACT_HASH = "sha256:60b66f702080efdf4492e32ae8607adf321340e26e521333e80f376bf1bb74f4"
DONOR_REVISION = "751823abaf677020bcd72c45fe3e7cb2b8a576e4"
DATA_PATH = Path("data/johto/berry_graphics.json")
HEADER_PATH = Path("src/data/object_events/johto_berry_graphics.h")
RULES_PATH = Path("spritesheet_rules.mk")
ASSET_NAMES = ("cheri", "chesto", "pecha", "rawst", "aspear", "leppa", "oran", "persim", "lum", "sitrus", "dirt_pile", "sprout")
STAGES = ("BERRY_STAGE_PLANTED", "BERRY_STAGE_SPROUTED", "BERRY_STAGE_TALLER", "BERRY_STAGE_TRUNK", "BERRY_STAGE_BUDDING", "BERRY_STAGE_FLOWERING", "BERRY_STAGE_BERRIES")
STAGE_MAPPING = (0, 1, 2, 2, 2, 3, 4)
PALETTE_TAGS = {2: "OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_WHITE", 3: "OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_PINK", 4: "OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE", 5: "OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_GREEN"}
SPECIES = ASSET_NAMES[:10]

def sha256(data: bytes) -> str: return hashlib.sha256(data).hexdigest()
def png_size(data: bytes) -> tuple[int, int]:
    if data[:8] != b"\x89PNG\r\n\x1a\n" or data[12:16] != b"IHDR": raise SystemExit("invalid PNG signature/IHDR")
    return struct.unpack(">II", data[16:24])
def donor_asset(root: Path, name: str) -> Path: return root / "graphics/object_events/pics/berry_trees" / (name + ".png")
def target_asset(root: Path, name: str) -> Path: return root / "graphics/johto/berry_trees" / (name + ".png")
def load_manifest(root: Path) -> dict:
    payload = json.loads((root / DATA_PATH).read_text(encoding="utf-8"))
    if payload.get("contract_hash") != CONTRACT_HASH or payload.get("donor_revision") != DONOR_REVISION: raise SystemExit("manifest contract/source revision mismatch")
    return payload
def render_header() -> bytes:
    out = ["#ifndef GUARD_DATA_OBJECT_EVENTS_JOHTO_BERRY_GRAPHICS_H", "#define GUARD_DATA_OBJECT_EVENTS_JOHTO_BERRY_GRAPHICS_H", "", '#include "constants/johto_berry_plots.h"', "", "/* Generated from donor revision 751823abaf677020bcd72c45fe3e7cb2b8a576e4. */"]
    out += [f'const u32 gJohtoBerryPic_{name.upper()}[] = INCBIN_U32("graphics/johto/berry_trees/{name}.4bpp");' for name in ASSET_NAMES]
    out += [""]
    for name in SPECIES:
        u = name.upper()
        out += [f"static const struct SpriteFrameImage sJohtoBerryPicTable_{u}[] = {{", "    overworld_frame(gJohtoBerryPic_DIRT_PILE, 2, 2, 0),", "    overworld_frame(gJohtoBerryPic_SPROUT, 2, 2, 0),", "    overworld_frame(gJohtoBerryPic_SPROUT, 2, 2, 1),"]
        out += [f"    overworld_frame(gJohtoBerryPic_{u}, 2, 4, {i})," for i in range(6)]
        out += ["};", "", "static const u16 sJohtoBerryPaletteTags_" + u + "[] = {"]
        out += [f"    {PALETTE_TAGS[int(slot)]}," for slot in [3,4,4,2,2][0:3] + []] if False else []
        out += ["};", ""]
    # Replace per-species palette bodies from the manifest's donor slots.
    text = "\n".join(out)
    # Re-render cleanly to avoid any parser dependency in generated output.
    out = ["#ifndef GUARD_DATA_OBJECT_EVENTS_JOHTO_BERRY_GRAPHICS_H", "#define GUARD_DATA_OBJECT_EVENTS_JOHTO_BERRY_GRAPHICS_H", "", '#include "constants/johto_berry_plots.h"', "", "/* Generated from donor revision 751823abaf677020bcd72c45fe3e7cb2b8a576e4. */"]
    out += [f'const u32 gJohtoBerryPic_{name.upper()}[] = INCBIN_U32("graphics/johto/berry_trees/{name}.4bpp");' for name in ASSET_NAMES] + [""]
    fixed_slots = {"cheri":[3,4,4,2,2],"chesto":[3,4,2,4,4],"pecha":[3,4,4,5,5],"rawst":[3,4,4,3,3],"aspear":[3,4,3,4,4],"leppa":[3,4,3,2,2],"oran":[3,4,2,3,3],"persim":[3,4,2,3,3],"lum":[3,4,4,5,5],"sitrus":[3,4,4,5,5]}
    for name in SPECIES:
        u = name.upper(); slots = fixed_slots[name]
        out += [f"static const struct SpriteFrameImage sJohtoBerryPicTable_{u}[] = {{", "    overworld_frame(gJohtoBerryPic_DIRT_PILE, 2, 2, 0),", "    overworld_frame(gJohtoBerryPic_SPROUT, 2, 2, 0),", "    overworld_frame(gJohtoBerryPic_SPROUT, 2, 2, 1),"]
        out += [f"    overworld_frame(gJohtoBerryPic_{u}, 2, 4, {i})," for i in range(6)] + ["};", "", f"static const u16 sJohtoBerryPaletteTags_{u}[] = {{"]
        out += [f"    {PALETTE_TAGS[slots[i]]}," for i in STAGE_MAPPING] + ["};", ""]
    out += ["static const struct SpriteFrameImage *const sJohtoBerryPicTables[] = {"] + [f"    [BERRY_ID_{name.upper()}] = sJohtoBerryPicTable_{name.upper()}," for name in SPECIES] + ["};", "", "static const u16 *const sJohtoBerryPaletteTags[] = {"] + [f"    [BERRY_ID_{name.upper()}] = sJohtoBerryPaletteTags_{name.upper()}," for name in SPECIES] + ["};", "", "static bool8 JohtoBerryGraphics_IsSupported(u8 plotId, u8 berryId, u8 stage)", "{", "    return plotId >= JOHTO_BERRY_PLOTS_FIRST", "        && plotId <= JOHTO_BERRY_PLOTS_LAST", "        && berryId >= BERRY_ID_CHERI", "        && berryId <= BERRY_ID_SITRUS", "        && stage < ARRAY_COUNT(sJohtoBerryPaletteTags_CHERI);", "}", "", "static bool8 JohtoBerryGraphics_Apply(struct ObjectEvent *objectEvent, struct Sprite *sprite, u8 berryId, u8 stage)", "{", "    u8 paletteIndex;", "    const u16 *paletteTags;", "    if (!JohtoBerryGraphics_IsSupported(objectEvent->trainerRange_berryTreeId, berryId, stage))", "        return FALSE;", "    paletteTags = sJohtoBerryPaletteTags[berryId];", "    if (paletteTags == NULL)", "        return FALSE;", "    paletteIndex = FindObjectEventPaletteIndexByTag(paletteTags[stage]);", "    if (paletteIndex == 0xFF)", "        return FALSE;", "    UpdateSpritePalette(&sObjectEventSpritePalettes[paletteIndex], sprite);", "    sprite->images = sJohtoBerryPicTables[berryId];", "    return TRUE;", "}", "", "#endif // GUARD_DATA_OBJECT_EVENTS_JOHTO_BERRY_GRAPHICS_H", ""]
    return ("\n".join(out)).encode()
def render_rules(existing: str) -> bytes:
    import re
    core = re.sub(r"\n+# BEGIN JOHTO BERRY GRAPHICS RULES\n.*?# END JOHTO BERRY GRAPHICS RULES\n", "\n", existing, flags=re.S)
    out = ["# BEGIN JOHTO BERRY GRAPHICS RULES\n"]
    for name in ASSET_NAMES:
        width = 2; height = 4 if name in SPECIES else 2
        out.append(f"graphics/johto/berry_trees/{name}.4bpp: %.4bpp: %.png\n\t$(GFX) $< $@ -mwidth {width} -mheight {height}\n")
    out.append("# END JOHTO BERRY GRAPHICS RULES\n")
    block = "".join(out)
    shared_marker = "# BEGIN JOHTO SHARED OBJECT FRAME RULES"
    if shared_marker in core:
        position = core.index(shared_marker)
        prefix = core[:position].rstrip()
        suffix = core[position:]
        return (prefix + "\n\n" + block + "\n" + suffix).encode()
    return (core.rstrip() + "\n\n" + block).encode()
def expected_outputs(root: Path, donor: Path, payload: dict) -> dict[Path, bytes]:
    outputs = {root / HEADER_PATH: render_header(), root / RULES_PATH: render_rules((root / RULES_PATH).read_text(encoding="utf-8"))}
    for name in ASSET_NAMES:
        source = donor_asset(donor, name)
        if not source.is_file(): raise SystemExit("missing donor asset: " + str(source))
        data = source.read_bytes()
        rec = payload["assets"][name]
        if sha256(data) != rec["sha256"]: raise SystemExit("donor hash mismatch: " + name)
        if list(png_size(data)) != rec["dimensions"]: raise SystemExit("donor dimensions mismatch: " + name)
        outputs[target_asset(root, name)] = data
    return outputs
def main(argv=None):
    p = argparse.ArgumentParser(); p.add_argument("--donor-root", required=True, type=Path); p.add_argument("--root", type=Path); mode=p.add_mutually_exclusive_group(); mode.add_argument("--write", action="store_true"); mode.add_argument("--check", action="store_true"); args=p.parse_args(argv)
    root = (args.root or Path(__file__).resolve().parents[2]).resolve(); donor = args.donor_root.resolve(); payload=load_manifest(root); outputs=expected_outputs(root, donor, payload)
    for path, data in outputs.items():
        if not path.is_file():
            if args.write: continue
            raise SystemExit("missing generated output: " + str(path))
        actual=path.read_bytes(); expected=data
        if path.suffix in (".h", ".mk"): actual=actual.replace(b"\r\n", b"\n"); expected=expected.replace(b"\r\n", b"\n")
        if actual != expected:
            if not args.write: raise SystemExit("stale or drifted output: " + str(path))
    if args.write:
        for path,data in outputs.items(): path.parent.mkdir(parents=True, exist_ok=True); path.write_bytes(data)
    print(f"Checked {len(outputs)} outputs: 12 donor assets, 10 species tables, 7 host stages")
    return 0
if __name__ == "__main__": raise SystemExit(main())
