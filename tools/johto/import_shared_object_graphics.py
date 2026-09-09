#!/usr/bin/env python3
"""Import the pinned ordinary Johto object graphics as one additive unit."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path


CONTRACT_HASH = "sha256:eb587dee56cf5828f754287e8a6c552145ea74055ced228b74a1b1324703c9d0"
DONOR_REVISION = "751823abaf677020bcd72c45fe3e7cb2b8a576e4"
DATA_PATH = Path("data/johto/shared_object_graphics.json")
HOST_CORE_HASHES = {
    "include/constants/event_objects.h": "1dc568513d5830e57924bb52df99668724bdadef8ced756e8a91acf4320dcada",
    "src/event_object_movement.c": "77ffefa225d34220c9e956b248ca2b08206c3f28e1cc8e73a4d49ba65c336fd3",
    "src/data/object_events/object_event_graphics_info_pointers.h": "439b98168e95f369973870ee38f73942462fb6bfd9379c2dd4d680520ade104a",
    "spritesheet_rules.mk": "0f1aac9bb1d80b09c9f22d46ea36c69e357456ea8df6783f0b4dbe16e81199d9",
}
SOURCE_HASHES = {
    "src/data/object_events/object_event_graphics_info.h": "6987761f2f058fcae67a393da7bcd73dd72ff1faeefa60bceae66f52650744c1",
    "src/data/object_events/object_event_pic_tables.h": "4a87b4462ced16c0aeabd9ef5d065b48a0bf5743cbf8244050cdf0bdd57e1a0c",
    "src/data/object_events/object_event_anims.h": "8dcdce9fc34f693714f403b446ac37e3aa1499e612930410d8bbc82ec1ea9b17",
}
EXPECTED_COUNT = 49


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def normalized(path: Path) -> bytes:
    return path.read_bytes().replace(b"\r\n", b"\n")


def suffix(token: str) -> str:
    return token.removeprefix("OBJ_EVENT_GFX_")


def palette_suffix(source_path: str) -> str:
    return Path(source_path).stem.upper()


def info_name(token: str) -> str:
    return "gObjectEventGraphicsInfo_JohtoShared_" + suffix(token)


def picture_name(token: str) -> str:
    return "gObjectEventPic_JohtoShared_" + suffix(token)


def picture_table_name(token: str) -> str:
    return "sJohtoSharedPicTable_" + suffix(token)


def palette_name(source_path: str) -> str:
    return "gObjectEventPal_JohtoShared_" + palette_suffix(source_path)


def palette_tag(source_path: str) -> str:
    return "OBJ_EVENT_PAL_TAG_JOHTO_SHARED_" + palette_suffix(source_path)


def public_id(token: str) -> str:
    return "OBJ_EVENT_GFX_JOHTO_SHARED_" + suffix(token)


def source_relative(source_path: str) -> Path:
    return Path(source_path).relative_to("graphics/object_events")


def destination_asset(root: Path, source_path: str) -> Path:
    return root / "graphics/johto/shared/object_events" / source_relative(source_path)


def build_asset_path(source_path: str, extension: str) -> str:
    return (Path("graphics/johto/shared/object_events") / source_relative(source_path)).with_suffix(extension).as_posix()


def extract_block(text: str, needle: str) -> str:
    start = text.find(needle)
    if start < 0:
        raise ValueError(f"missing donor definition: {needle}")
    brace = text.find("{", start)
    end = text.find("};", brace)
    if brace < 0 or end < 0:
        raise ValueError(f"unterminated donor definition: {needle}")
    line_start = text.rfind("\n", 0, start) + 1
    return text[line_start : end + 2]


def extract_decl(text: str, declaration: str) -> str:
    match = None
    for candidate in re.finditer(r"(?m)^(?:static )?" + re.escape(declaration) + r"\s*=\s*\{", text):
        if text.rfind("/*", 0, candidate.start()) <= text.rfind("*/", 0, candidate.start()):
            match = candidate
            break
    if match is None:
        raise ValueError(f"missing donor declaration: {declaration}")
    brace = text.find("{", match.start())
    end = text.find("};", brace)
    if brace < 0 or end < 0:
        raise ValueError(f"unterminated donor declaration: {declaration}")
    line_start = text.rfind("\n", 0, match.start()) + 1
    return text[line_start : end + 2]


def extract_animation_table(text: str, name: str) -> str:
    match = re.search(
        r"(?m)^(?:static )?const union AnimCmd \*const " + re.escape(name) + r"\[\]\s*=\s*\{.*?^\};",
        text,
        flags=re.S,
    )
    if match is None:
        raise ValueError(f"missing animation table: {name}")
    return match.group(0)


def animation_command_names(table: str) -> list[str]:
    names = []
    for name in re.findall(r"\b(sAnim_[A-Za-z0-9_]+)\b", table):
        if name not in names:
            names.append(name)
    return names


def normalize_animation(text: str) -> str:
    return re.sub(r"\bstatic\s+", "", text.replace("\r\n", "\n")).strip()


def render_animation_table(donor_text: str, host_text: str, source_name: str) -> tuple[str, list[str]]:
    donor_table = extract_animation_table(donor_text, source_name)
    try:
        host_table = extract_animation_table(host_text, source_name)
    except ValueError:
        host_table = ""
    if host_table and normalize_animation(host_table) == normalize_animation(donor_table):
        return source_name, []

    command_names = animation_command_names(donor_table)
    renamed = donor_table.replace(source_name, "sJohtoSharedAnimTable_" + source_name.removeprefix("sAnimTable_"), 1)
    rendered = []
    for old in command_names:
        command = extract_block(donor_text, "static const union AnimCmd " + old + "[]")
        new = "sJohtoShared_" + old.removeprefix("sAnim_")
        command = re.sub(r"\b" + re.escape(old) + r"\b", new, command)
        renamed = re.sub(r"\b" + re.escape(old) + r"\b", new, renamed)
        rendered.append(command)
    return "sJohtoSharedAnimTable_" + source_name.removeprefix("sAnimTable_"), rendered + [renamed]


def load_records(root: Path) -> tuple[list[dict], dict]:
    data_path = root / DATA_PATH
    if data_path.is_file():
        payload = json.loads(data_path.read_text(encoding="utf-8"))
        records = payload.get("source_ledger")
        if not isinstance(records, list):
            raise SystemExit("durable source ledger missing")
        return records, payload
    raise SystemExit(f"missing tracked source ledger: {data_path}")


def preflight(root: Path, donor_root: Path, records: list[dict], payload: dict) -> None:
    revision = subprocess.check_output(["git", "-C", str(donor_root), "rev-parse", "HEAD"], text=True).strip()
    if revision != DONOR_REVISION:
        raise SystemExit("donor revision drift")
    for rel, expected in SOURCE_HASHES.items():
        path = donor_root / rel
        if sha256_bytes(normalized(path)) != expected:
            raise SystemExit("pinned donor definition drift: " + rel)
    if len(records) != EXPECTED_COUNT:
        raise SystemExit(f"expected exactly {EXPECTED_COUNT} source records")
    tokens = [record["token"] for record in records]
    if len(set(tokens)) != EXPECTED_COUNT or any(not token.startswith("OBJ_EVENT_GFX_") for token in tokens):
        raise SystemExit("source token coverage drift")
    for record in records:
        for key in ("donor_pictures", "donor_palette"):
            if key == "donor_pictures":
                items = record[key]
            else:
                items = [record[key]]
            for item in items:
                source = donor_root / item["path"]
                if not source.is_file():
                    raise SystemExit(f"missing pinned donor asset: {source}")
                if sha256(source) != item["sha256"] or source.stat().st_size != item["bytes"]:
                    raise SystemExit(f"pinned donor asset drift: {source}")
    if payload and payload.get("contract_hash") not in (None, CONTRACT_HASH):
        raise SystemExit("metadata contract binding drift")


def render_outputs(root: Path, donor_root: Path, records: list[dict], source_payload: dict) -> dict[Path, bytes]:
    info_text = (donor_root / "src/data/object_events/object_event_graphics_info.h").read_text(encoding="utf-8")
    pics_text = (donor_root / "src/data/object_events/object_event_pic_tables.h").read_text(encoding="utf-8")
    anim_text = (donor_root / "src/data/object_events/object_event_anims.h").read_text(encoding="utf-8")
    host_anim_text = (root / "src/data/object_events/object_event_anims.h").read_text(encoding="utf-8")
    outputs: dict[Path, bytes] = {}

    pictures = []
    infos = []
    frame_counts = {}
    animation_blocks: list[str] = []
    animation_map: dict[str, str] = {}
    for record in records:
        token = record["token"]
        fields = record["donor_info"]["fields"]
        old_table = fields["anims"]
        if old_table not in animation_map:
            animation_map[old_table], blocks = render_animation_table(anim_text, host_anim_text, old_table)
            animation_blocks.extend(blocks)
        old_pic_table = fields["images"]
        pic_block = extract_decl(pics_text, "const struct SpriteFrameImage " + old_pic_table + "[]")
        pic_block = re.sub(
            r"\b" + re.escape(old_pic_table) + r"\b",
            picture_table_name(token),
            pic_block,
            count=1,
        )
        for picture in record["donor_pictures"]:
            pic_block = re.sub(r"\b" + re.escape(picture["symbol"]) + r"\b", picture_name(token), pic_block)
        pictures.append(pic_block)
        frame_counts[token] = count_frames(pic_block)

        info_needle = "const struct ObjectEventGraphicsInfo " + record["donor_info"]["symbol"]
        info_start = -1
        search_from = 0
        while True:
            candidate = info_text.find(info_needle, search_from)
            if candidate < 0:
                break
            line_start = info_text.rfind("\n", 0, candidate) + 1
            line_prefix = info_text[line_start:candidate].strip()
            if (not line_prefix.startswith("//")
                    and info_text.rfind("/*", 0, candidate) <= info_text.rfind("*/", 0, candidate)):
                info_start = candidate
                break
            search_from = candidate + len(info_needle)
        if info_start < 0:
            raise ValueError("missing donor graphics info: " + record["donor_info"]["symbol"])
        info_brace = info_text.find("{", info_start)
        info_end = info_text.find("};", info_brace)
        values = [value.strip() for value in info_text[info_brace + 1 : info_end].split(",")]
        if len(values) != 16:
            raise ValueError("unexpected graphics info field count: " + token)
        values[1] = palette_tag(record["donor_palette"]["path"])
        values[13] = animation_map[old_table]
        values[14] = picture_table_name(token)
        info_block = "const struct ObjectEventGraphicsInfo " + info_name(token) + " = {" + ", ".join(values) + "};"
        infos.append(info_block)

    unique_palettes = sorted({record["donor_palette"]["path"] for record in records})
    assets_lines = ["// Generated from the pinned donor source ledger; do not hand edit.\n"]
    for record in records:
        source_path = record["donor_pictures"][0]["path"]
        assets_lines.append(
            f'const u32 {picture_name(record["token"])}[] = INCBIN_U32("{build_asset_path(source_path, ".4bpp")}");\n'
        )
    for source_path in unique_palettes:
        assets_lines.append(
            f'const u16 {palette_name(source_path)}[16] = INCBIN_U16("{build_asset_path(source_path, ".gbapal")}");\n'
        )
    outputs[root / "src/data/object_events/johto_shared_assets.h"] = "".join(assets_lines).encode()

    declarations = ["// Generated declarations for the bounded Johto shared object graphics set.\n"]
    for record in records:
        declarations.append(f"extern const u32 {picture_name(record['token'])}[];\n")
    for source_path in unique_palettes:
        declarations.append(f"extern const u16 {palette_name(source_path)}[];\n")
    for record in records:
        declarations.append(f"extern const struct ObjectEventGraphicsInfo {info_name(record['token'])};\n")
    outputs[root / "src/data/object_events/johto_shared_declarations.h"] = "".join(declarations).encode()

    info_lines = ["// Generated from the pinned donor tables; each shared Johto symbol is namespaced.\n\n"]
    info_lines.extend(block + "\n\n" for block in animation_blocks)
    info_lines.extend(block + "\n\n" for block in pictures)
    info_lines.extend(block + "\n\n" for block in infos)
    outputs[root / "src/data/object_events/johto_shared_info.h"] = "".join(info_lines).rstrip().encode() + b"\n"

    outputs[root / "src/data/object_events/johto_shared_pointers.inc"] = "".join(
        f"    [{public_id(record['token'])}] = &{info_name(record['token'])},\n" for record in records
    ).encode()
    outputs[root / "src/data/object_events/johto_shared_palettes.inc"] = "".join(
        f"    {{{palette_name(source_path)}, {palette_tag(source_path)}}},\n" for source_path in unique_palettes
    ).encode()

    metadata = {
        "schema_version": 1,
        "contract_hash": CONTRACT_HASH,
        "donor": {"repository": "johto-hns", "revision": DONOR_REVISION},
        "scope": {"objects": EXPECTED_COUNT, "pictures": EXPECTED_COUNT, "palettes": len(unique_palettes)},
        "objects": [
            {
                "token": record["token"],
                "public_id": public_id(record["token"]),
                "info_symbol": info_name(record["token"]),
                "picture_symbol": picture_name(record["token"]),
                "picture_table": picture_table_name(record["token"]),
                "frame_count": frame_counts[record["token"]],
                "logical_dimensions": f"{record['donor_info']['fields']['width']}x{record['donor_info']['fields']['height']}",
                "palette_slot": int(record["donor_info"]["fields"]["paletteSlot"]),
                "animation_table": record["donor_info"]["fields"]["anims"],
                "palette_tag": palette_tag(record["donor_palette"]["path"]),
                "picture": {"source_path": record["donor_pictures"][0]["path"], "sha256": record["donor_pictures"][0]["sha256"], "bytes": record["donor_pictures"][0]["bytes"]},
                "palette": {"source_path": record["donor_palette"]["path"], "sha256": record["donor_palette"]["sha256"], "bytes": record["donor_palette"]["bytes"]},
            }
            for record in records
        ],
        "excluded_special_objects": source_payload.get("excluded_special_objects", []),
        "host_compatible": source_payload.get("host_compatible", []),
        "source_ledger": records,
    }
    outputs[root / DATA_PATH] = (json.dumps(metadata, indent=2, sort_keys=True) + "\n").encode()

    for record in records:
        source_path = record["donor_pictures"][0]["path"]
        outputs[destination_asset(root, source_path)] = (donor_root / source_path).read_bytes()
    for source_path in unique_palettes:
        outputs[destination_asset(root, source_path)] = (donor_root / source_path).read_bytes()

    outputs.update(render_integrations(root, records, unique_palettes))
    return outputs


def count_frames(raw: str) -> int:
    return len(re.findall(r"\b(?:overworld_frame|obj_frame_tiles)\s*\(", raw))


def strip_integration(rel: str, text: str) -> str:
    if rel == "include/constants/event_objects.h":
        text = re.sub(r"\n    /\* JOHTO_SHARED_OBJECT_GRAPHICS_IDS \*/\n.*?(?=    NUM_OBJ_EVENT_GFX,)", "", text, flags=re.S)
        text = re.sub(r"\n/\* JOHTO_SHARED_OBJECT_GRAPHICS_PALETTE_TAGS \*/\n.*?(?=#define OBJ_EVENT_PAL_TAG_NONE)", "", text, flags=re.S)
    elif rel == "src/event_object_movement.c":
        text = text.replace('#include "data/object_events/johto_shared_assets.h"\n', "")
        text = text.replace('#include "data/object_events/johto_shared_info.h"\n', "")
        text = text.replace('#include "data/object_events/johto_shared_palettes.inc"\n', "")
    elif rel == "src/data/object_events/object_event_graphics_info_pointers.h":
        text = text.replace('#include "johto_shared_declarations.h"\n', "")
        text = text.replace('#include "johto_shared_pointers.inc"\n', "")
    elif rel == "spritesheet_rules.mk":
        text = re.sub(r"\n+# BEGIN JOHTO SHARED OBJECT FRAME RULES\n.*?# END JOHTO SHARED OBJECT FRAME RULES\n", "\n", text, flags=re.S)
    return text


def render_integrations(root: Path, records: list[dict], unique_palettes: list[str]) -> dict[Path, bytes]:
    outputs = {}
    ids_rel = "include/constants/event_objects.h"
    ids_path = root / ids_rel
    ids_core = strip_integration(ids_rel, ids_path.read_text(encoding="utf-8"))
    if sha256_bytes(ids_core.replace("\r\n", "\n").encode()) != HOST_CORE_HASHES[ids_rel]:
        raise SystemExit("host core identity drift: " + ids_rel)
    ids = "\n    /* JOHTO_SHARED_OBJECT_GRAPHICS_IDS */\n" + "".join(
        f"    {public_id(record['token'])},\n" for record in records
    )
    ids_core = ids_core.replace("    NUM_OBJ_EVENT_GFX,", ids + "    NUM_OBJ_EVENT_GFX,", 1)
    tags = "\n/* JOHTO_SHARED_OBJECT_GRAPHICS_PALETTE_TAGS */\n" + "".join(
        f"#define {palette_tag(source_path):<52} 0x{0x1240 + i:04X}\n" for i, source_path in enumerate(unique_palettes)
    )
    ids_core = ids_core.replace("#define OBJ_EVENT_PAL_TAG_NONE", tags + "#define OBJ_EVENT_PAL_TAG_NONE", 1)
    outputs[ids_path] = ids_core.encode()

    movement_rel = "src/event_object_movement.c"
    movement_path = root / movement_rel
    movement_core = strip_integration(movement_rel, movement_path.read_text(encoding="utf-8"))
    movement_core = movement_core.replace(
        '#include "data/object_events/johto_assets.h"',
        '#include "data/object_events/johto_assets.h"\n#include "data/object_events/johto_shared_assets.h"',
        1,
    )
    movement_core = movement_core.replace(
        '#include "data/object_events/johto_info.h"',
        '#include "data/object_events/johto_info.h"\n#include "data/object_events/johto_shared_info.h"',
        1,
    )
    movement_core = movement_core.replace(
        '#include "data/object_events/johto_palettes.inc"',
        '#include "data/object_events/johto_palettes.inc"\n#include "data/object_events/johto_shared_palettes.inc"',
        1,
    )
    if sha256_bytes(strip_integration(movement_rel, movement_path.read_text(encoding="utf-8")).replace("\r\n", "\n").encode()) != HOST_CORE_HASHES[movement_rel]:
        raise SystemExit("host core identity drift: " + movement_rel)
    outputs[movement_path] = movement_core.encode()

    pointer_rel = "src/data/object_events/object_event_graphics_info_pointers.h"
    pointer_path = root / pointer_rel
    pointer_core = strip_integration(pointer_rel, pointer_path.read_text(encoding="utf-8"))
    pointer_core = pointer_core.replace('#include "johto_declarations.h"', '#include "johto_declarations.h"\n#include "johto_shared_declarations.h"', 1)
    pointer_core = pointer_core.replace('#include "johto_pointers.inc"', '#include "johto_pointers.inc"\n#include "johto_shared_pointers.inc"', 1)
    if sha256_bytes(strip_integration(pointer_rel, pointer_path.read_text(encoding="utf-8")).replace("\r\n", "\n").encode()) != HOST_CORE_HASHES[pointer_rel]:
        raise SystemExit("host core identity drift: " + pointer_rel)
    outputs[pointer_path] = pointer_core.encode()

    rules_rel = "spritesheet_rules.mk"
    rules_path = root / rules_rel
    rules_core = strip_integration(rules_rel, rules_path.read_text(encoding="utf-8"))
    if sha256_bytes(rules_core.replace("\r\n", "\n").encode()) != HOST_CORE_HASHES[rules_rel]:
        raise SystemExit("host core identity drift: " + rules_rel)
    rules = ["# BEGIN JOHTO SHARED OBJECT FRAME RULES\n"]
    for record in records:
        width = int(record["donor_info"]["fields"]["width"]) // 8
        height = int(record["donor_info"]["fields"]["height"]) // 8
        source_path = record["donor_pictures"][0]["path"]
        target = build_asset_path(source_path, ".4bpp")
        rules.append(f"{target}: %.4bpp: %.png\n\t$(GFX) $< $@ -mwidth {width} -mheight {height}\n")
    rules.append("# END JOHTO SHARED OBJECT FRAME RULES\n")
    outputs[rules_path] = (rules_core + "".join(rules)).encode()
    return outputs


def check_outputs(root: Path, outputs: dict[Path, bytes], write: bool) -> None:
    for path, expected in outputs.items():
        if not path.is_file():
            if not write:
                raise SystemExit("missing generated output: " + str(path))
            continue
        actual = path.read_bytes()
        if path.suffix in (".h", ".c", ".json", ".inc", ".mk"):
            actual = actual.replace(b"\r\n", b"\n")
            expected = expected.replace(b"\r\n", b"\n")
        if actual != expected:
            if not write:
                raise SystemExit("stale or drifted output: " + str(path))
            rel = path.relative_to(root).as_posix()
            if rel in HOST_CORE_HASHES:
                continue
    if write:
        for path, expected in outputs.items():
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(expected)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--donor-root", required=True, type=Path)
    parser.add_argument("--root", type=Path)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--write", action="store_true")
    mode.add_argument("--check", action="store_true")
    args = parser.parse_args(argv)
    root = args.root.resolve() if args.root else Path(__file__).resolve().parents[2]
    donor_root = args.donor_root.resolve()
    records, payload = load_records(root)
    preflight(root, donor_root, records, payload)
    outputs = render_outputs(root, donor_root, records, payload)
    check_outputs(root, outputs, write=args.write)
    print(f"Checked {len(outputs)} outputs: 49 objects, 9 palettes, 58 assets")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
