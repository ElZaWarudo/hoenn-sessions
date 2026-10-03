"""TEST ONLY: author and cache a generation-one Main harbor save from New Game.

Only standalone ROM button inputs write the save. No server admission, save
patches, legacy migration, population, or custody/travel proof is performed.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
from PIL import Image, __version__ as PILLOW_VERSION

from live_author_fixtures import cache_key, config_check
from live_fixture_population import box_species
from live_harness_oracles import OracleFailure, logical_field, read_flash_bytes
import live_region_harness as harness
from live_scripted_input import owned_scripted_emulator

BOOT = (("boot", 0, 1, 240), ("skip", 2, 8, 1200), ("title", 8, 8, 600))
COLD = BOOT + (("menu", 8, 8, 240), ("cold-harbor", 1, 8, 300))
# Pinned fresh-1790979201404093700 controls0001..0019. Input release and
# bounded waits replace operator idle time; screenshots retain each boundary.
INTRO = (
    ("new-game-intro", [(8, 8, 240), (1, 8, 600)]),
)
INTRO_LIMIT = 60
TEMPLATES = {
    "gender": ((18, 33, 78, 78), "65bfbec1940c423d1fc69ff3dd6f02034dd9c3b844d17f11d9ff2da9a5f6aead"),
    "name-field": ((87, 34, 164, 69), "da63e79b45986daae7b745b57e7b1aae894bf68a2f1d22c36888250fc5eff8e2"),
    "name-keyboard": ((24, 77, 169, 145), "ca116920e83eccace6d446dcd5a602d67fb8834d654585ee5dce15b55fd8fdf7"),
}


def load_templates() -> dict:
    templates = {}
    for name, (roi, digest) in TEMPLATES.items():
        path = Path(__file__).with_name("fixtures") / f"harbor-{name}.png"
        harness.require_hash(path, digest, "fresh harbor pinned intro template")
        with Image.open(path) as image:
            rgb = image.convert("RGB")
            if rgb.size != (roi[2] - roi[0], roi[3] - roi[1]):
                raise OracleFailure("fresh harbor intro template dimensions differ")
            templates[name] = rgb.copy()
    return templates


def matches_intro(path: Path, templates: dict, names) -> bool:
    with Image.open(path) as image:
        rgb = image.convert("RGB")
        if rgb.size != (240, 160):
            raise OracleFailure("fresh harbor needs native-size intro screenshots")
        for name in names:
            actual = rgb.crop(TEMPLATES[name][0])
            expected = templates[name]
            if name == "name-field":
                # naming_screen.c SpriteCB_InputArrow moves the eight-pixel
                # arrow at y56 left by 0/-4/-2/-1. This ROI begins at x87;
                # mask only its intersecting footprint, before input starts x96.
                actual.paste(expected.crop((0, 18, 8, 26)), (0, 18))
            elif name == "name-keyboard":
                # SpriteCB_Cursor pulses one palette color, not its geometry.
                # CreateCursorSprite/SetCursorPos put default A at (38,88).
                # Keep all 80 outline pixels at that position, one common tint,
                # and every other keyboard pixel exact; don't hide the cursor.
                outline = [(x, y) for y in range(3, 19) for x in range(8, 20)
                           if expected.getpixel((x, y)) == (252, 160, 173)]
                colors = {actual.getpixel(point) for point in outline}
                if len(outline) != 80 or len(colors) != 1 or next(iter(colors))[0] != 252:
                    return False
                for point in outline:
                    actual.putpixel(point, expected.getpixel(point))
            if actual.tobytes() != expected.tobytes():
                return False
        return True


def advance_intro(act, templates: dict) -> dict:
    # Advance only until the actual default Boy/Girl menu is visible. Long
    # fixed-count holds can pass this menu while dialogue still depends on idle.
    image = act("intro-gender-probe", 0, 1, 300)
    for attempt in range(INTRO_LIMIT + 1):
        if matches_intro(image, templates, ("gender",)):
            break
        if attempt == INTRO_LIMIT:
            raise OracleFailure("fresh harbor intro timed out before Boy/Girl checkpoint")
        image = act(f"intro-advance-{attempt:02d}", 1, 8, 300)
    gender = image.name
    act("intro-boy-selected", 1, 8, 600)
    act("intro-name-opening", 1, 8, 300)
    image = act("intro-name-ready", 0, 1, 300)
    if not matches_intro(image, templates, ("name-field", "name-keyboard")):
        raise OracleFailure("fresh harbor blank name/A-selection checkpoint differs")
    return {"gender": gender, "name": image.name}
FIELD = (
    ("new-game-field", [(1, 120, 600)] * 3),
    ("starting-truck", [(1, 120, 600)] * 3),
    ("new-game-truck-arrival", [(1, 120, 1800)]),
    ("new-game-field-ready", [(1, 120, 1200)]),
    ("truck-field", [(1, 120, 1800)]),
    ("harbor-warp-group", [(264, 8, 60), (1, 8, 20), (128, 8, 20), (1, 8, 60)]),
    ("harbor-warp-map", [(64, 8, 20)] * 3 + [(16, 8, 20), (64, 8, 20), (1, 8, 60)]),
    ("harbor-warp-point", [(16, 8, 20), (64, 8, 20), (1, 8, 60)]),
    ("fresh-harbor", [(1, 8, 600)]),
    ("harbor-interaction-point", [(32, 8, 20)] * 3 + [(64, 8, 20)] * 2 + [(64, 8, 120)]),
)
B_IDENTITY = (
    ("b-player-menu", [(264, 8, 60)] + [(128, 8, 20)] * 4 + [(1, 8, 60)]),
    ("b-new-trainer", [(128, 8, 20)] * 2 + [(1, 8, 180)]),
    ("b-give-menu", [(264, 8, 60)] + [(128, 8, 20)] * 3 + [(1, 8, 60)]),
    ("b-max-money", [(128, 8, 20)] * 5 + [(1, 8, 60), (2, 8, 60), (2, 8, 180)]),
)
SAVE = (
    ("fresh-save-menu", [(8, 8, 60)]),
    ("fresh-save-confirm", [(2, 8, 60), (32, 8, 20), (64, 4, 60), (8, 8, 60)]
     + [(128, 8, 20)] * 4 + [(1, 8, 60)]),
    ("fresh-save-ready", [(0, 1, 300)]),
    ("fresh-first-save", [(1, 8, 600)]),
)


def controls(name: str):
    if name not in ("A", "B"):
        raise OracleFailure("fresh harbor supports only A/B normal-intro names")
    naming = ([(16, 8, 20)] if name == "B" else []) + [(1, 8, 20), (8, 8, 60), (1, 8, 600)]
    return INTRO + (("name-confirm", naming),) + FIELD + (B_IDENTITY if name == "B" else ()) + SAVE


def validate_harbor(data: bytes, descriptor: bytes, name: str, path: Path) -> dict:
    controls(name)
    save = read_flash_bytes(data, path)
    if save.generation != 1:
        raise OracleFailure("fresh harbor must be ROM generation one")
    player_name = logical_field(save, descriptor, 0x0200)
    trainer = logical_field(save, descriptor, 0x0205)
    money = logical_field(save, descriptor, 0x0102)
    location = logical_field(save, descriptor, 0x0100)
    party = logical_field(save, descriptor, 0x0101)
    pc = logical_field(save, descriptor, 0x0301)
    if len(player_name) != 8 or player_name[:2] != bytes((0xBB if name == "A" else 0xBC, 255)):
        raise OracleFailure("fresh harbor normal-intro player name differs")
    if len(trainer) != 4 or len(money) != 4 or struct.unpack("<I", money)[0] != (3000 if name == "A" else 999999):
        raise OracleFailure("fresh harbor trainer/money ABI or expected distinction differs")
    if len(location) != 564 or location[4:7] != bytes((13, 10, 0)):
        raise OracleFailure("fresh harbor needs Main harbor with ROM-written warp zero")
    # ZeroMonData leaves MON_DATA_MAIL at MAIL_NONE (255), including empty
    # party slots. Requiring every byte zero would reject real New Game saves.
    empty_mon = bytes(85) + b"\xff" + bytes(14)
    if (len(party) != 604 or party[:4] != bytes(4)
            or any(party[4 + i * 100:104 + i * 100] != empty_mon for i in range(6))
            or len(pc) != 33600 or any(
            box_species(pc[i:i + 80]) is not None for i in range(0, len(pc), 80))):
        raise OracleFailure("fresh harbor must have empty party and PC")
    return {"save_sha256": hashlib.sha256(data).hexdigest(), "generation": save.generation,
            "lineage_hex": save.lineage.hex(), "player_name_hex": player_name.hex(),
            "trainer_id_hex": trainer.hex(), "money": struct.unpack("<I", money)[0],
            "map_group": 13, "map_num": 10, "warp_id": 0,
            "x": struct.unpack_from("<h", location)[0], "y": struct.unpack_from("<h", location, 2)[0],
            "party_count": 0, "pc_count": 0}


def retain_harbor(data: bytes, target: Path, descriptor: bytes, name: str) -> dict:
    checked = validate_harbor(data, descriptor, name, target)
    with target.open("xb") as file:
        file.write(data)
    if target.read_bytes() != data:
        raise OracleFailure("fresh harbor immutable retention differs")
    return checked


def cached_receipt(root: Path, inputs: dict, descriptor: bytes, name: str) -> dict:
    receipt = json.loads((root / "receipt.json").read_text())
    if receipt.get("inputs") != inputs or receipt.get("cache_key") != cache_key(inputs):
        raise OracleFailure("fresh harbor cache inputs differ")
    data = (root / "harbor.sav").read_bytes()
    checked = validate_harbor(data, descriptor, name, root / "harbor.sav")
    if receipt.get("harbor") != checked or receipt.get("cold_save_sha256") != checked["save_sha256"]:
        raise OracleFailure("fresh harbor cache semantic/cold witness differs")
    if any((root / stage / "game.sav").read_bytes() != data for stage in ("author", "cold")):
        raise OracleFailure("fresh harbor cached runtime bytes differ")
    images = receipt.get("screenshots")
    if not isinstance(images, dict) or not {"author/intro-name-ready.png", "author/fresh-first-save.png", "cold/cold-harbor.png"} <= images.keys():
        raise OracleFailure("fresh harbor cache lacks required screenshots")
    for relative, digest in images.items():
        path = Path(relative)
        if path.is_absolute() or len(path.parts) != 2 or path.parts[0] not in ("author", "cold") or path.suffix != ".png":
            raise OracleFailure("fresh harbor screenshot path differs")
        image = (root / path).read_bytes()
        if not image.startswith(b"\x89PNG\r\n\x1a\n") or hashlib.sha256(image).hexdigest() != digest:
            raise OracleFailure("fresh harbor cached screenshot differs")
    if not matches_intro(root / "author/intro-name-ready.png", load_templates(), ("name-field", "name-keyboard")):
        raise OracleFailure("fresh harbor cached blank name checkpoint differs")
    gates = receipt.get("intro_checkpoints")
    if (not isinstance(gates, dict) or gates.get("name") != "author/intro-name-ready.png"
            or gates.get("gender") not in images
            or not gates["gender"].startswith("author/")
            or not matches_intro(root / gates["gender"], load_templates(), ("gender",))):
        raise OracleFailure("fresh harbor cached reached gender checkpoint differs")
    return receipt


def author_player(plan: dict, name: str, output: Path, config: Path) -> dict:
    harness.preflight(plan, profiles_ready=False)
    harness.check_c_space()
    itinerary = controls(name)
    if output.resolve().drive.casefold() == Path(os.environ.get("SystemDrive", "C:")).drive.casefold():
        raise OracleFailure("fresh harbor requires spare-volume output")
    config_check(config)
    templates = load_templates()
    release = Path(plan["release_dir"])
    world = next(w for w in json.loads((release / "release_catalog.json").read_text())["worlds"] if w["world_id"] == 1)
    descriptor = bytes.fromhex(json.loads((release / "server-build-catalog.json").read_text())["shared_player_descriptor_hex"])
    rom, exe = release / world["rom_path"], release / "runtime/mgba.exe"
    harness.require_hash(rom, world["rom_sha256"], "fresh harbor ROM")
    dependencies = ("live_author_harbor.py", "live_author_fixtures.py", "live_fixture_population.py",
                    "live_scripted_input.py", "live_harness_oracles.py", "player_transfer_manifest.py",
                    "live_region_harness.py", "live_fixture_validation.py", "live_fixture_custody.py")
    inputs = {"initial_save": "absent", "name": name, "menu_profile": "hoenn-debug-v1",
              "pillow_version": PILLOW_VERSION,
              "rom_sha256": world["rom_sha256"], "emulator_sha256": harness.digest(exe),
              "config_sha256": harness.digest(config), "descriptor_sha256": hashlib.sha256(descriptor).hexdigest(),
              "intro_templates": {key: {"roi": list(value[0]), "sha256": value[1]} for key, value in TEMPLATES.items()},
              "signed_fixture": {k: plan[k] for k in ("release_id", "envelope_sha256", "catalog_sha256", "server_catalog_sha256")},
              "dependencies": {dep: harness.digest(Path(__file__).with_name(dep)) for dep in dependencies}}
    root = output / cache_key(inputs)
    if root.exists():
        return cached_receipt(root, inputs, descriptor, name)
    root.mkdir(parents=True, exist_ok=False)
    (root / "inputs.json").write_text(json.dumps(inputs, indent=2))
    images = {}
    intro_checkpoints = {}

    def run(stage, initial, actions):
        folder = root / stage
        folder.mkdir()
        shutil.copyfile(rom, folder / "game.gba")
        if (folder / "game.sav").exists():
            raise OracleFailure("fresh harbor initial save must be absent")
        if initial is not None:
            (folder / "game.sav").write_bytes(initial)  # Exact retained copy for cold Continue only.
        local = folder / "appdata/mGBA/config.ini"
        local.parent.mkdir(parents=True)
        shutil.copyfile(config, local)
        env = dict(os.environ, APPDATA=str(folder / "appdata"), LOCALAPPDATA=str(folder / "localappdata"))
        with owned_scripted_emulator(exe, folder, env) as script:
            def act(label, mask, hold, wait):
                harness.check_c_space()
                image = script.act(label, mask, hold=hold, wait=wait)
                images[f"{stage}/{image.name}"] = harness.digest(image)
                return image

            for label, mask, hold, wait in (BOOT if initial is None else COLD):
                act(label, mask, hold, wait)
            for label, group in actions:
                if label == "name-confirm":
                    intro_checkpoints.update({key: f"{stage}/{value}" for key, value in advance_intro(act, templates).items()})
                harness.check_c_space()
                for i, (mask, hold, wait) in enumerate(group):
                    act(label if i == len(group) - 1 else f"{label}-{i}", mask, hold, wait)
            captured = (folder / "game.sav").read_bytes()
        if (folder / "game.sav").read_bytes() != captured:
            raise OracleFailure("fresh harbor closure changed captured bytes")
        return captured

    authored = run("author", None, itinerary)
    checked = retain_harbor(authored, root / "harbor.sav", descriptor, name)
    cold = run("cold", authored, ())
    if cold != authored:
        raise OracleFailure("fresh harbor cold Continue changed exact retained bytes")
    for path, expected in ((rom, inputs["rom_sha256"]), (exe, inputs["emulator_sha256"]), (config, inputs["config_sha256"])):
        harness.require_hash(path, expected, "fresh harbor input after owned sessions")
    receipt = {"cache_key": cache_key(inputs), "inputs": inputs, "harbor": checked,
               "intro_checkpoints": intro_checkpoints,
               "cold_save_sha256": hashlib.sha256(cold).hexdigest(), "screenshots": images,
               "ui_review": "pending: inspect author/fresh-first-save.png and cold/cold-harbor.png",
               "scope": "clean New Game generation-one harbor only; no population/custody/server/travel proof"}
    with (root / "receipt.json").open("x") as file:
        json.dump(receipt, file, indent=2)
    return cached_receipt(root, inputs, descriptor, name)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", type=Path, required=True)
    parser.add_argument("--name", choices=("A", "B"), required=True)
    parser.add_argument("--output-root", type=Path, required=True)
    parser.add_argument("--mgba-config", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(author_player(json.loads(args.plan.read_text(encoding="utf-8-sig")), args.name,
                                   args.output_root, args.mgba_config), indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
