"""TEST ONLY: cache one ROM-written held-mail checkpoint; no server writes.

Standalone scripted buttons execute normal ROM menus. This does not establish
PC mailbox, daycare, a second player's custody, or cross-ROM travel.
"""
from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct

from live_author_fixtures import cache_key, config_check, seed_bytes
from live_fixture_custody import _carrier, _mail
from live_fixture_population import GROWTH_OFFSETS, box_species, check_population
from live_harness_oracles import OracleFailure, logical_field, read_flash_bytes
import live_region_harness as harness
from live_scripted_input import owned_scripted_emulator

WORDS = [2060] + [65535] * 8
BOOT = (("boot", 0, 1, 240), ("skip", 2, 8, 1200), ("title", 8, 8, 600),
        ("menu", 8, 8, 240), ("cold-harbor", 1, 8, 300))
# Prototype menu choices with explicit printer/fade waits: operator idle time
# must not be an implicit part of this reproducible itinerary.
CONTROLS = (
    ("debug-menu", [(264, 8, 60)]),
    ("give-menu", [(128, 8, 20)] * 3 + [(1, 8, 20)]),
    ("item-id", [(1, 8, 20)]),
    ("harbor-mail-id", [(16, 8, 20)] * 2 + [(64, 8, 20)] * 2 + [(32, 8, 20)] * 2 + [(128, 8, 20)]),
    ("mail-given", [(1, 8, 20), (64, 8, 20), (64, 8, 20), (1, 8, 60), (2, 8, 20), (2, 8, 20)]),
    ("start-menu", [(8, 8, 20)]),
    ("bag-mail", [(128, 8, 20), (1, 8, 90)]),
    ("mail-party", [(1, 8, 20), (16, 8, 20), (1, 8, 60)]),
    ("mail-editor", [(1, 8, 60)]),
    ("mail-greeting", [(1, 8, 20), (128, 8, 20), (128, 8, 20), (1, 8, 20)]),
    ("mail-confirm", [(1, 8, 20), (8, 8, 20)]),
    ("mail-delivered", [(1, 8, 20)]),
    # gText_PkmnWasGivenItem ends in PAUSE_UNTIL_PRESS. Finish printing first,
    # then dismiss with B and let the party fade return to Bag before closing it.
    ("gift-message-ready", [(0, 1, 600)]),
    ("bag-after-mail", [(2, 8, 180)]),
    ("post-bag-menu", [(2, 8, 180)]),
    ("save-confirm", [(128, 8, 20)] * 4 + [(1, 8, 60)]),
    ("save-confirm-ready", [(0, 1, 300)]),
    ("overwrite-confirm", [(1, 8, 20)]),
    ("overwrite-ready", [(0, 1, 300)]),
    ("first-mail-saved", [(1, 8, 600)]),
)


def recipe_check(player: dict) -> dict:
    if player.get("authoring_menu_profile") != "hoenn-debug-v1":
        raise OracleFailure("first-mail needs hoenn-debug-v1 menu profile")
    recipe = player.get("population_recipe")
    if (not isinstance(recipe, dict) or recipe.get("abi") != "hoenn-box80-v1"
            or not isinstance(recipe.get("party_species"), list) or len(recipe["party_species"]) != 6
            or not isinstance(recipe.get("pc_species"), list) or len(recipe["pc_species"]) != 1
            or any(type(s) is not int or not 1 <= s <= 32 for s in recipe["party_species"] + recipe["pc_species"])
            or type(recipe.get("party_level")) is not int or not 1 <= recipe["party_level"] <= 20):
        raise OracleFailure("first-mail needs the bounded six-party/one-PC population recipe")
    items = recipe.get("bag_items")
    if (not isinstance(items, list) or not items or any(
            not isinstance(item, list) or len(item) != 2 or type(item[0]) is not int
            or not 1 <= item[0] <= 1023 or 199 <= item[0] <= 210
            or type(item[1]) is not int or not 1 <= item[1] <= 999 for item in items)):
        raise OracleFailure("first-mail requires original non-mail Bag recipe")
    return recipe


def _growth(record: bytes) -> list[int]:
    box_species(record)
    personality, trainer = struct.unpack_from("<II", record)
    return [value ^ personality ^ trainer for (value,) in struct.iter_unpack("<I", record[32:80])]


def _sender(source, descriptor: bytes) -> tuple[bytes, bytes]:
    name = logical_field(source, descriptor, 0x0200)
    trainer = logical_field(source, descriptor, 0x0205)
    if len(name) != 8 or len(trainer) != 4 or 255 not in name or 252 in name[:7]:
        raise OracleFailure("first-mail needs ordinary terminated player name and trainer ABI")
    # GiveMailToMonByItemId copies seven bytes, appends EOS, then PadNameString
    # pads to six characters. The eighth byte stays EOS.
    sender = bytearray(name[:7] + b"\xff")
    end = sender.index(255)
    while end < 6:
        sender[end] = 0
        end += 1
    sender[end] = 255
    return bytes(sender), trainer


def _bag(value: bytes) -> Counter:
    if len(value) % 4:
        raise OracleFailure("first-mail Bag ABI differs")
    totals = Counter()
    for item, quantity in struct.iter_unpack("<HH", value):
        if item:
            totals[item] += quantity
    return totals


def validate_first_mail(data: bytes, source, descriptor: bytes, recipe: dict, path: Path) -> dict:
    saved = read_flash_bytes(data, path)
    if saved.generation != source.generation + 1 or saved.lineage != source.lineage:
        raise OracleFailure("first-mail normal Save generation/trainer lineage differs")
    before = logical_field(source, descriptor, 0x0101)
    after = logical_field(saved, descriptor, 0x0101)
    if len(before) != 604 or len(after) != 604 or before[0] != 6 or after[:4] != before[:4]:
        raise OracleFailure("first-mail party ABI/count differs")
    if after[104:] != before[104:] or logical_field(source, descriptor, 0x0301) != logical_field(saved, descriptor, 0x0301):
        raise OracleFailure("first-mail changed other party or PC records")
    original, current = before[4:104], after[4:104]
    species = box_species(original[:80])
    if species is None or species == 201 or 1024 <= species <= 1050:
        raise OracleFailure("first-mail carrier absent or unsupported Unown")
    personality, trainer_id = struct.unpack_from("<II", original)
    carrier = {"species": species, "personality": personality, "ot_id": trainer_id}
    _carrier(original[:80], carrier, 0)
    _carrier(current[:80], carrier, 200)
    if original[85] != 255 or not 0 <= current[85] <= 5:
        raise OracleFailure("first-mail carrier mail index differs")
    # Compare every carrier byte except checksum, held-item bits and mail index.
    old_words, new_words = _growth(original[:80]), _growth(current[:80])
    growth = GROWTH_OFFSETS[personality % 24] * 3
    old_words[growth] &= ~(1023 << 16)
    new_words[growth] &= ~(1023 << 16)
    if (old_words != new_words or original[:28] != current[:28]
            or original[30:32] != current[30:32] or original[80:85] != current[80:85]
            or original[86:] != current[86:]):
        raise OracleFailure("first-mail changed unrelated carrier data")
    sender, trainer = _sender(source, descriptor)
    if _sender(saved, descriptor) != (sender, trainer):
        raise OracleFailure("first-mail player sender identity changed")
    old_mail = logical_field(source, descriptor, 0x010B)
    mail = logical_field(saved, descriptor, 0x010B)
    if len(old_mail) != 576 or len(mail) != 576 or any(struct.unpack_from("<H", old_mail, i * 36 + 32)[0] for i in range(16)):
        raise OracleFailure("first-mail requires an empty source mailbox")
    index = current[85]
    message = {"species": species, "item_id": 200, "words": WORDS}
    _mail(mail[index * 36:(index + 1) * 36], message, sender, trainer)
    if any(mail[i * 36:(i + 1) * 36] != old_mail[i * 36:(i + 1) * 36] for i in range(16) if i != index):
        raise OracleFailure("first-mail changed unrelated mailbox slots")
    old_bag = _bag(logical_field(source, descriptor, 0x0106))
    if any(old_bag[item] for item in range(199, 211)):
        raise OracleFailure("first-mail source already contains mail")
    expected_bag = old_bag.copy()
    expected_bag[200] = 2
    if _bag(logical_field(saved, descriptor, 0x0106)) != expected_bag:
        raise OracleFailure("first-mail changed original Bag quantities")
    before_location = logical_field(source, descriptor, 0x0100)
    if logical_field(saved, descriptor, 0x0100)[:8] != before_location[:8]:
        raise OracleFailure("first-mail changed source harbor position/map")
    population_recipe = dict(recipe, bag_items=recipe["bag_items"] + [[200, 2]])
    population = check_population(saved, descriptor, population_recipe)
    return {"source_sha256": source.sha256, "save_sha256": saved.sha256,
            "generation": saved.generation, "carrier": carrier, "mail_index": index,
            "sender_name_hex": sender.hex(), "trainer_id_hex": trainer.hex(),
            "message": message, "population": population}


def retain_first_mail(data: bytes, target: Path, source, descriptor: bytes, recipe: dict) -> dict:
    result = validate_first_mail(data, source, descriptor, recipe, target)
    with target.open("xb") as out:
        out.write(data)
    if target.read_bytes() != data:
        raise OracleFailure("first-mail retained bytes changed")
    return result


def cached_receipt(root: Path, inputs: dict, source, descriptor: bytes, recipe: dict) -> dict:
    receipt = json.loads((root / "receipt.json").read_text())
    if receipt.get("inputs") != inputs or receipt.get("cache_key") != cache_key(inputs):
        raise OracleFailure("first-mail cache inputs changed")
    data = (root / "first-mail.sav").read_bytes()
    checked = validate_first_mail(data, source, descriptor, recipe, root / "first-mail.sav")
    if receipt.get("first_mail") != checked or receipt.get("cold_save_sha256") != checked["save_sha256"]:
        raise OracleFailure("first-mail cache lacks validated exact cold bytes")
    if (root / "cold/game.sav").read_bytes() != data:
        raise OracleFailure("first-mail cached cold save bytes changed")
    screenshots = receipt.get("screenshots")
    if not isinstance(screenshots, dict) or not {"cold/cold-party.png", "author/first-mail-saved.png"} <= screenshots.keys():
        raise OracleFailure("first-mail cache lacks required screenshots")
    for name, expected in screenshots.items():
        relative = Path(name)
        if relative.is_absolute() or len(relative.parts) != 2 or relative.parts[0] not in ("author", "cold") or relative.suffix != ".png":
            raise OracleFailure("first-mail screenshot path differs")
        image = (root / relative).read_bytes()
        if not image.startswith(b"\x89PNG\r\n\x1a\n") or hashlib.sha256(image).hexdigest() != expected:
            raise OracleFailure("first-mail cached screenshot changed")
    return receipt


def author_player(plan: dict, player: dict, output: Path, config: Path, world_id: int = 1) -> dict:
    harness.preflight(plan, profiles_ready=False)
    harness.check_c_space()
    if player not in plan["players"]:
        raise OracleFailure("first-mail player must be covered by signed plan preflight")
    if world_id != 1 or output.resolve().drive.casefold() == Path(os.environ.get("SystemDrive", "C:")).drive.casefold():
        raise OracleFailure("first-mail supports Main on spare-volume output only")
    recipe = recipe_check(player)
    config_check(config)
    release = Path(plan["release_dir"])
    catalog = json.loads((release / "release_catalog.json").read_text())
    world = next(w for w in catalog["worlds"] if w["world_id"] == world_id)
    descriptor = bytes.fromhex(json.loads((release / "server-build-catalog.json").read_text())["shared_player_descriptor_hex"])
    data, source = seed_bytes(Path(player["source_save"]), player["source_sha256"])
    check_population(source, descriptor, recipe)
    _sender(source, descriptor)
    party = logical_field(source, descriptor, 0x0101)
    species = box_species(party[4:84])
    personality, trainer_id = struct.unpack_from("<II", party, 4)
    _carrier(party[4:84], {"species": species, "personality": personality, "ot_id": trainer_id}, 0)
    if party[89] != 255 or any(_bag(logical_field(source, descriptor, 0x0106))[item] for item in range(199, 211)):
        raise OracleFailure("first-mail source already has carrier/Bag mail")
    mailbox = logical_field(source, descriptor, 0x010B)
    if len(mailbox) != 576 or any(struct.unpack_from("<H", mailbox, i * 36 + 32)[0] for i in range(16)):
        raise OracleFailure("first-mail requires an empty source mailbox")
    # Clean New Game harbor saves retain entrance warp 0; earlier ROM-written
    # ferry fixtures retain WARP_ID_NONE. Preserve either exact source value.
    if logical_field(source, descriptor, 0x0100)[4:7] not in (bytes((13, 10, 0)), bytes((13, 10, 255))):
        raise OracleFailure("first-mail source must be the coherent Main harbor fixture")
    rom, exe = release / world["rom_path"], release / "runtime/mgba.exe"
    harness.require_hash(rom, world["rom_sha256"], "first-mail ROM")
    dependencies = ("live_author_mail.py", "live_author_fixtures.py", "live_fixture_population.py",
                    "live_fixture_custody.py", "live_scripted_input.py", "live_harness_oracles.py",
                    "live_fixture_validation.py", "player_transfer_manifest.py", "live_region_harness.py")
    inputs = {"rom_sha256": world["rom_sha256"], "emulator_sha256": harness.digest(exe),
              "config_sha256": harness.digest(config), "seed_sha256": source.sha256,
              "descriptor_sha256": hashlib.sha256(descriptor).hexdigest(), "recipe": recipe,
              "menu_profile": player["authoring_menu_profile"],
              "signed_fixture": {key: plan[key] for key in (
                  "release_id", "envelope_sha256", "catalog_sha256", "server_catalog_sha256")},
              "dependencies": {name: harness.digest(Path(__file__).with_name(name)) for name in dependencies}}
    root = output / cache_key(inputs)
    if root.exists():
        return cached_receipt(root, inputs, source, descriptor, recipe)
    root.mkdir(parents=True, exist_ok=False)
    (root / "inputs.json").write_text(json.dumps(inputs, indent=2))
    snapshots = {}

    def run(stage: str, initial: bytes, controls) -> bytes:
        folder = root / stage
        folder.mkdir()
        shutil.copyfile(rom, folder / "game.gba")
        (folder / "game.sav").write_bytes(initial)  # Exact byte copy, never patched.
        local = folder / "appdata/mGBA/config.ini"
        local.parent.mkdir(parents=True)
        shutil.copyfile(config, local)
        env = dict(os.environ, APPDATA=str(folder / "appdata"), LOCALAPPDATA=str(folder / "localappdata"))
        with owned_scripted_emulator(exe, folder, env) as script:
            for label, mask, hold, wait in BOOT:
                image = script.act(label, mask, hold=hold, wait=wait)
                snapshots[f"{stage}/{image.name}"] = harness.digest(image)
            for label, actions in controls:
                harness.check_c_space()
                for i, (mask, hold, wait) in enumerate(actions):
                    name = label if i == len(actions) - 1 else f"{label}-{i}"
                    image = script.act(name, mask, hold=hold, wait=wait)
                    snapshots[f"{stage}/{image.name}"] = harness.digest(image)
            captured = (folder / "game.sav").read_bytes()  # One immutable capture.
        if (folder / "game.sav").read_bytes() != captured:
            raise OracleFailure("first-mail emulator closure changed captured save")
        return captured

    authored = run("author", data, CONTROLS)
    first_mail = retain_first_mail(authored, root / "first-mail.sav", source, descriptor, recipe)
    cold = run("cold", authored, (("cold-start", [(8, 8, 60)]), ("cold-party", [(1, 8, 180)])))
    if cold != authored:
        raise OracleFailure("first-mail cold Continue changed retained bytes")
    # Recheck pinned input after both owned processes have closed.
    seed_bytes(Path(player["source_save"]), player["source_sha256"])
    receipt = {"cache_key": cache_key(inputs), "inputs": inputs, "first_mail": first_mail,
               "cold_save_sha256": hashlib.sha256(cold).hexdigest(), "screenshots": snapshots,
               "ui_review": "pending: inspect author/first-mail-saved.png and cold/cold-party.png",
               "scope": "first party-held mail normal Save/cold Continue only; no server/custody travel proof"}
    with (root / "receipt.json").open("x") as file:
        json.dump(receipt, file, indent=2)
    return cached_receipt(root, inputs, source, descriptor, recipe)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", type=Path, required=True)
    parser.add_argument("--player", required=True)
    parser.add_argument("--output-root", type=Path, required=True)
    parser.add_argument("--mgba-config", type=Path, required=True)
    args = parser.parse_args()
    plan = json.loads(args.plan.read_text(encoding="utf-8-sig"))
    players = [p for p in plan["players"] if p["name"] == args.player]
    if len(players) != 1:
        parser.error("select exactly one named player")
    print(json.dumps(author_player(plan, players[0], args.output_root, args.mgba_config), indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
