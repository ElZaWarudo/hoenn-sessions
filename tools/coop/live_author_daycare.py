"""TEST ONLY: normal Daycare deposit and harbor ferry Save, gen5 to6.

Current cached parents are prerequisites, never implicitly reauthored. No
memory/save patches, server operations, ROM builds or legacy migration.
Auxiliary walking/script/time/key state may change; custody is checked fully.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct

import live_author_mail as held_mail
import live_author_third_mail as third_mail
from live_author_fixtures import cache_key, config_check, seed_bytes
from live_fixture_custody import check_custody
from live_fixture_population import check_population
from live_harness_oracles import OracleFailure, logical_field, read_flash_bytes
import live_region_harness as harness
from live_scripted_input import owned_scripted_emulator
from player_transfer_manifest import parse_schema_payload

PARENT_DEPS = third_mail.PARENT_DEPS + ("live_author_third_mail.py",)
MAX_POST_DEPOSIT_STEPS = 10
MAX_TOTAL_STEPS = 20
CONTROLS = (
    ("daycare-group", [(264, 8, 60), (1, 8, 20), (128, 8, 20), (1, 8, 60)]),
    ("daycare-map", [(64, 8, 20)] * 2 + [(16, 8, 20)] + [(64, 8, 20)] * 2 + [(1, 8, 60)]),
    ("daycare-warp", [(1, 8, 60), (1, 8, 300)]),
    ("daycare-woman", [(64, 8, 60)] * 5 + [(1, 8, 300)]),
    ("daycare-question", [(1, 8, 300)] * 2),
    ("daycare-choose-prompt", [(1, 8, 300)]),
    ("daycare-storage", [(1, 8, 300)]),
    ("daycare-party-button", [(128, 8, 60)] * 5),
    ("daycare-party-panel", [(1, 8, 180)]),
    ("daycare-third-selected", [(128, 8, 60)] * 2 + [(1, 8, 120)]),
    ("daycare-raise", [(1, 8, 300)]),
    ("daycare-raise-second-page", [(1, 8, 300)]),
    ("daycare-deposited-question", [(1, 8, 300)]),
    ("daycare-refuse-second", [(128, 8, 60), (1, 8, 300)]),
    ("daycare-field", [(1, 8, 180)]),
    ("return-warp-group", [(264, 8, 60), (1, 8, 20), (128, 8, 20), (1, 8, 60)]),
    ("return-warp-map", [(64, 8, 20)] * 3 + [(16, 8, 20), (64, 8, 20), (1, 8, 60)]),
    ("return-warp-point", [(16, 8, 20), (64, 8, 20), (1, 8, 60)]),
    ("return-harbor", [(1, 8, 300)]),
    # The first tap after a direction change can turn without walking. A
    # 24-frame directional hold takes the first step; two short taps follow.
    ("ferry-position", [(32, 24, 60), (32, 8, 60), (32, 8, 60),
                         (64, 24, 60), (64, 8, 60), (64, 8, 60)]),
    ("daycare-save-menu", [(8, 8, 60)]),
    ("daycare-save-confirm", [(128, 8, 20)] * 5 + [(1, 8, 60)]),
    ("daycare-save-ready", [(0, 1, 300)]),
    ("daycare-overwrite", [(1, 8, 20)]),
    ("daycare-overwrite-ready", [(0, 1, 300)]),
    ("daycare-saved", [(1, 8, 600)]),
)


def _friendship_counter(save, descriptor: bytes) -> int:
    # SaveBlock1 vars[0x18C] immediately precede gameStats. Derive its base
    # from the compiler-written descriptor, not vanilla annotated offsets.
    fields = {field["id"]: field for field in parse_schema_payload(descriptor)["fields"]}
    stats, local = fields[0x0113], fields[0x0108]
    offset = stats["offset"] - 0x18C * 2 + (0x402A - 0x4000) * 2
    if (stats["size"] != 256 or stats["storage"] != 0 or local["storage"] != 0
            or not local["offset"] <= offset <= local["offset"] + local["size"] - 2):
        raise OracleFailure("Daycare friendship-vars compiler ABI differs")
    value = struct.unpack("<H", save.field(0, offset, 2))[0]
    if value >= 128:
        raise OracleFailure("Daycare friendship counter outside ordinary walking state")
    return value


def _identity(record: bytes) -> dict:
    species = held_mail.box_species(record[:80])
    if species is None or not 1 <= species <= 32:
        raise OracleFailure("Daycare fixture requires ordinary bounded carrier species")
    personality, ot_id = struct.unpack_from("<II", record)
    return {"species": species, "personality": personality, "ot_id": ot_id}


def validate_source(source, descriptor: bytes) -> list[dict]:
    if source.generation != 5:
        raise OracleFailure("Daycare requires a generation-five third-Mail parent")
    party, mail, daycare = (logical_field(source, descriptor, fid) for fid in (0x0101, 0x010B, 0x010D))
    if (len(party), len(mail), len(daycare)) != (604, 576, 288) or party[0] != 6:
        raise OracleFailure("Daycare source custody ABI/count differs")
    if daycare[:284] != bytes(284) or struct.unpack_from("<I", daycare, 284)[0] >= 128:
        raise OracleFailure("Daycare source must have clean empty custody and ordinary hatch counter")
    if logical_field(source, descriptor, 0x0100)[4:7] not in (bytes((13, 10, 0)), bytes((13, 10, 255))):
        raise OracleFailure("Daycare source must retain Main harbor")
    sender, trainer = held_mail._sender(source, descriptor)
    identities = []
    for slot, held, index in ((0, 0, 255), (1, 200, 0), (2, 200, 1)):
        record = party[4 + slot * 100:104 + slot * 100]
        identity = _identity(record)
        held_mail._carrier(record[:80], identity, held)
        if record[85] != index:
            raise OracleFailure("Daycare source attachments differ")
        identities.append(identity)
    if len({identity["species"] for identity in identities}) != 3:
        raise OracleFailure("Daycare needs three distinct custody species")
    for index, identity in ((0, identities[1]), (1, identities[2]), (6, identities[0])):
        held_mail._mail(mail[index * 36:(index + 1) * 36],
                        {"words": held_mail.WORDS, "species": identity["species"], "item_id": 200}, sender, trainer)
    if any(struct.unpack_from("<H", mail, index * 36 + 32)[0] for index in range(16) if index not in (0, 1, 6)):
        raise OracleFailure("Daycare source has unrelated occupied Mail")
    if any(held_mail._bag(logical_field(source, descriptor, 0x0106))[item] for item in range(199, 211)):
        raise OracleFailure("Daycare source Bag must have consumed all Mail")
    target = party[204:304]
    if target[18] & 7 != 2 or 252 in target[8:18]:
        raise OracleFailure("Daycare nickname fixture needs ordinary English bytes")
    _friendship_counter(source, descriptor)
    return identities


def validate_parent(third_root: Path, pc_root: Path, held_root: Path, plan: dict, player: dict,
                    descriptor: bytes, rom: Path, exe: Path, config: Path):
    recipe = held_mail.recipe_check(player)
    _, pc, pc_receipt, ancestry = third_mail.validate_parent(pc_root, held_root, plan, player, descriptor, rom, exe, config)
    expected = {"player": player["name"], "source_sha256": pc.sha256, "recipe": recipe, "ancestry": ancestry,
                "parent_cache_key": pc_receipt["cache_key"], "parent_receipt_sha256": harness.digest(pc_root / "receipt.json"),
                "rom_sha256": harness.digest(rom), "emulator_sha256": harness.digest(exe), "config_sha256": harness.digest(config),
                "descriptor_sha256": hashlib.sha256(descriptor).hexdigest(), "signed_fixture": pc_receipt["inputs"]["signed_fixture"],
                "dependencies": {dep: harness.digest(Path(__file__).with_name(dep)) for dep in PARENT_DEPS}}
    if json.loads((third_root / "inputs.json").read_text()) != expected:
        raise OracleFailure("Daycare third-Mail parent current signed/source/dependency inputs differ")
    receipt = third_mail.cached_receipt(third_root, expected, pc, descriptor, recipe, ancestry)
    data, source = seed_bytes(third_root / "third-mail.sav", receipt["validated"]["save_sha256"])
    validate_source(source, descriptor)
    chain = ancestry + [{"path": str(source.path.resolve()), "sha256": source.sha256}]
    if receipt["seed_lineage"] != chain:
        raise OracleFailure("Daycare third-Mail canonical ancestry differs")
    return data, source, receipt, chain


def validate_daycare(data: bytes, source, descriptor: bytes, recipe: dict, path: Path, *,
                     expected_generation: int = 6, expected_position: tuple[int, int] = (8, 11),
                     max_post_deposit_steps: int = MAX_POST_DEPOSIT_STEPS) -> dict:
    identities = validate_source(source, descriptor)
    if (type(expected_generation) is not int or expected_generation not in (6, 7)
            or not isinstance(expected_position, tuple) or len(expected_position) != 2
            or any(type(value) is not int or not 0 <= value <= 32 for value in expected_position)
            or type(max_post_deposit_steps) is not int or not 1 <= max_post_deposit_steps <= 10):
        raise OracleFailure("Daycare oracle expected boundary arguments differ")
    saved = read_flash_bytes(data, path)
    if saved.generation != expected_generation or saved.lineage != source.lineage:
        raise OracleFailure("Daycare normal Save generation/trainer lineage differs")
    for fid in (0x0301, 0x0106, 0x0102, 0x0200, 0x0205):
        if logical_field(saved, descriptor, fid) != logical_field(source, descriptor, fid):
            raise OracleFailure("Daycare changed PC/Bag/money/player identity")
    location = logical_field(saved, descriptor, 0x0100)
    if location[:8] != struct.pack("<hh", *expected_position) + bytes((13, 10, 0, 0)):
        raise OracleFailure("Daycare final harbor ferry position/map differs")
    before, party = (logical_field(save, descriptor, 0x0101) for save in (source, saved))
    if (len(party) != 604 or party[:4] != bytes((5,)) + before[1:4]
            or party[4:204] != before[4:204] or party[204:504] != before[304:604]):
        raise OracleFailure("Daycare party compaction/surviving records differ")
    # ZeroMonData clears HP before maxHP, writing old maxHP into box.hpLost.
    # The empty tail is therefore not all-zero; mail becomes MAIL_NONE.
    empty = bytearray(100)
    struct.pack_into("<H", empty, 30, struct.unpack_from("<H", before, 504 + 88)[0] & 0x3FFF)
    empty[85] = 255
    if party[504:] != empty:
        raise OracleFailure("Daycare compacted empty tail differs from native ZeroMonData")
    old_mail, mail = (logical_field(save, descriptor, 0x010B) for save in (source, saved))
    expected_mail = bytearray(old_mail)
    struct.pack_into("<H", expected_mail, 36 + 32, 0)
    if mail != expected_mail:
        raise OracleFailure("Daycare source Mail cleanup/other letters differ")
    old_daycare, daycare = (logical_field(save, descriptor, 0x010D) for save in (source, saved))
    if len(daycare) != 288:
        raise OracleFailure("Daycare final ABI differs")
    original, current = before[204:284], daycare[:80]
    held_mail._carrier(current, identities[2], 0)
    old_words, new_words = held_mail._growth(original), held_mail._growth(current)
    growth = held_mail.GROWTH_OFFSETS[identities[2]["personality"] % 24] * 3
    old_words[growth] &= ~(1023 << 16)
    new_words[growth] &= ~(1023 << 16)
    if old_words != new_words or original[:28] != current[:28] or original[30:32] != current[30:32]:
        raise OracleFailure("Daycare stored box identity/moves/PP/other data differ")
    steps = struct.unpack_from("<I", daycare, 136)[0]
    if not 1 <= steps <= max_post_deposit_steps:
        raise OracleFailure("Daycare post-deposit walking budget exceeded or unwalked")
    old_stats, stats = (logical_field(save, descriptor, 0x0113) for save in (source, saved))
    if len(old_stats) != 256 or len(stats) != 256:
        raise OracleFailure("Daycare logical game-stat ABI differs")
    previous, actual = list(struct.unpack("<64I", old_stats)), list(struct.unpack("<64I", stats))
    total_steps = actual[5] - previous[5]
    if not steps <= total_steps <= MAX_TOTAL_STEPS:
        raise OracleFailure("Daycare total walking budget/custody steps differs")
    expected_stats = previous.copy()
    expected_stats[0] += expected_generation - source.generation
    expected_stats[5] += total_steps
    expected_stats[47] += 1
    if actual != expected_stats:
        raise OracleFailure("Daycare Save/used-Daycare/other logical game statistics differ")
    friend = _friendship_counter(source, descriptor)
    if friend + total_steps >= 128 or _friendship_counter(saved, descriptor) != friend + total_steps:
        raise OracleFailure("Daycare walking crossed friendship boundary or counter differs")
    counter = (struct.unpack_from("<I", old_daycare, 284)[0] + total_steps) % 128
    expected_daycare = bytearray(old_daycare)
    expected_daycare[:80] = current  # Already checked independently against source decrypted box.
    expected_daycare[80:116] = old_mail[36:72]
    name = logical_field(source, descriptor, 0x0200)
    end = name.index(255)
    expected_daycare[116:117 + end] = name[:end + 1]
    # GetMonNicknameVanilla copies all10 bytes, including bytes after EOS;
    # the eleventh byte stays at the empty parent value. No control codes.
    expected_daycare[124:134] = original[8:18]
    expected_daycare[135] = 0x22
    struct.pack_into("<I", expected_daycare, 136, steps)
    struct.pack_into("<I", expected_daycare, 284, counter)
    if daycare != expected_daycare:
        raise OracleFailure("Daycare nested letter/names/languages/empty slot/offspring/hatch counter differs")
    sender, trainer = held_mail._sender(source, descriptor)
    message = lambda identity: {"words": held_mail.WORDS, "species": identity["species"], "item_id": 200}
    custody_recipe = {"abi": "hoenn-mail-daycare-v1", "sender_name_hex": sender.hex(), "trainer_id_hex": trainer.hex(),
        "party_mail": [dict(identities[1], slot=1, mail_index=0, mail=message(identities[1]))],
        "pc_mail": [{"slot": 6, "mail": message(identities[0])}],
        "daycare": [dict(identities[2], slot=0, mail=message(identities[2]),
                         ot_name_hex=bytes(expected_daycare[116:124]).hex(), mon_name_hex=bytes(expected_daycare[124:135]).hex(),
                         game_language=2, mon_language=2, steps=steps)]}
    custody = check_custody(saved, descriptor, custody_recipe)
    population_recipe = dict(recipe, party_species=recipe["party_species"][:2] + recipe["party_species"][3:])
    population = check_population(saved, descriptor, population_recipe)
    return {"source_sha256": source.sha256, "save_sha256": hashlib.sha256(data).hexdigest(), "generation": saved.generation,
            "position": list(expected_position), "post_deposit_steps": steps, "total_steps": total_steps,
            "custody_recipe": custody_recipe, "custody": custody, "shared_witnesses": custody["shared_witnesses"],
            "population_recipe": population_recipe, "population": population}


def cached_receipt(root: Path, inputs: dict, source, descriptor: bytes, recipe: dict, ancestry: list) -> dict:
    receipt = json.loads((root / "receipt.json").read_text())
    if (json.loads((root / "inputs.json").read_text()) != inputs or receipt.get("inputs") != inputs
            or receipt.get("cache_key") != cache_key(inputs)):
        raise OracleFailure("Daycare cache current inputs differ")
    data = (root / "daycare.sav").read_bytes()
    checked = validate_daycare(data, source, descriptor, recipe, root / "daycare.sav")
    expected = ancestry + [{"path": str((root / "daycare.sav").resolve()), "sha256": checked["save_sha256"]}]
    if receipt.get("validated") != checked or receipt.get("seed_lineage") != expected or receipt.get("cold_save_sha256") != checked["save_sha256"]:
        raise OracleFailure("Daycare cache semantic/canonical ancestry/cold witness differs")
    if any((root / stage / "game.sav").read_bytes() != data for stage in ("author", "cold")):
        raise OracleFailure("Daycare cache author/retained/cold bytes differ")
    images = receipt.get("screenshots")
    required = {"author/daycare-third-selected.png", "author/daycare-deposited-question.png", "author/daycare-saved.png", "cold/cold-party.png"}
    if not isinstance(images, dict) or not required <= images.keys():
        raise OracleFailure("Daycare cache lacks boundary screenshots")
    for relative, digest in images.items():
        path = Path(relative)
        if path.is_absolute() or len(path.parts) != 2 or path.parts[0] not in ("author", "cold") or path.suffix != ".png":
            raise OracleFailure("Daycare screenshot path differs")
        image = (root / path).read_bytes()
        if not image.startswith(b"\x89PNG\r\n\x1a\n") or hashlib.sha256(image).hexdigest() != digest:
            raise OracleFailure("Daycare cached screenshot differs")
    return receipt


def author_player(plan: dict, name: str, third_root: Path, pc_root: Path, held_root: Path, output: Path, config: Path) -> dict:
    harness.preflight(plan, profiles_ready=False)
    harness.check_c_space()
    players = [player for player in plan["players"] if player["name"] == name]
    if name not in ("a", "b") or len(players) != 1:
        raise OracleFailure("Daycare must select exactly one plan player a/b")
    player = players[0]
    recipe = held_mail.recipe_check(player)
    if output.resolve().drive.casefold() == Path(os.environ.get("SystemDrive", "C:")).drive.casefold():
        raise OracleFailure("Daycare requires spare-volume output")
    config_check(config)
    release = Path(plan["release_dir"])
    world = next(w for w in json.loads((release / "release_catalog.json").read_text())["worlds"] if w["world_id"] == 1)
    descriptor = bytes.fromhex(json.loads((release / "server-build-catalog.json").read_text())["shared_player_descriptor_hex"])
    rom, exe = release / world["rom_path"], release / "runtime/mgba.exe"
    harness.require_hash(rom, world["rom_sha256"], "Daycare signed Main ROM")
    initial, source, parent, ancestry = validate_parent(third_root, pc_root, held_root, plan, player, descriptor, rom, exe, config)
    inputs = {"player": name, "source_sha256": source.sha256, "recipe": recipe, "ancestry": ancestry,
              "parent_cache_key": parent["cache_key"], "parent_receipt_sha256": harness.digest(third_root / "receipt.json"),
              "rom_sha256": harness.digest(rom), "emulator_sha256": harness.digest(exe), "config_sha256": harness.digest(config),
              "descriptor_sha256": hashlib.sha256(descriptor).hexdigest(), "signed_fixture": parent["inputs"]["signed_fixture"],
              "max_post_deposit_steps": MAX_POST_DEPOSIT_STEPS, "max_total_steps": MAX_TOTAL_STEPS,
              "dependencies": {dep: harness.digest(Path(__file__).with_name(dep)) for dep in PARENT_DEPS + ("live_author_daycare.py",)}}
    root = output / cache_key(inputs)
    if root.exists():
        return cached_receipt(root, inputs, source, descriptor, recipe, ancestry)
    root.mkdir(parents=True, exist_ok=False)
    (root / "inputs.json").write_text(json.dumps(inputs, indent=2))
    images = {}

    def run(stage, initial_bytes, actions):
        folder = root / stage
        folder.mkdir()
        shutil.copyfile(rom, folder / "game.gba")
        (folder / "game.sav").write_bytes(initial_bytes)  # Exact source copy, never patch.
        local = folder / "appdata/mGBA/config.ini"
        local.parent.mkdir(parents=True)
        shutil.copyfile(config, local)
        env = dict(os.environ, APPDATA=str(folder / "appdata"), LOCALAPPDATA=str(folder / "localappdata"))
        with owned_scripted_emulator(exe, folder, env) as script:
            for label, mask, hold, wait in held_mail.BOOT:
                image = script.act(label, mask, hold=hold, wait=wait)
                images[f"{stage}/{image.name}"] = harness.digest(image)
            for label, group in actions:
                harness.check_c_space()
                for index, (mask, hold, wait) in enumerate(group):
                    image = script.act(label if index == len(group) - 1 else f"{label}-{index}", mask, hold=hold, wait=wait)
                    images[f"{stage}/{image.name}"] = harness.digest(image)
            captured = (folder / "game.sav").read_bytes()
        if (folder / "game.sav").read_bytes() != captured:
            raise OracleFailure("Daycare owned closure changed captured bytes")
        return captured

    authored = run("author", initial, CONTROLS)
    checked = validate_daycare(authored, source, descriptor, recipe, root / "daycare.sav")
    with (root / "daycare.sav").open("xb") as file:
        file.write(authored)
    if (root / "daycare.sav").read_bytes() != authored:
        raise OracleFailure("Daycare immutable retention differs")
    cold = run("cold", authored, (("cold-start", [(8, 8, 60)]), ("cold-party", [(1, 8, 180)])))
    if cold != authored:
        raise OracleFailure("Daycare cold Continue changed retained bytes")
    validate_parent(third_root, pc_root, held_root, plan, player, descriptor, rom, exe, config)
    if harness.digest(third_root / "receipt.json") != inputs["parent_receipt_sha256"]:
        raise OracleFailure("Daycare third-Mail receipt changed during authoring")
    receipt = {"cache_key": cache_key(inputs), "inputs": inputs, "validated": checked,
               "seed_lineage": ancestry + [{"path": str((root / "daycare.sav").resolve()), "sha256": checked["save_sha256"]}],
               "cold_save_sha256": hashlib.sha256(cold).hexdigest(), "screenshots": images,
               "ui_review": "pending: inspect chooser/deposit/ferry/Save/cold-party screenshots",
               "scope": "normal Daycare custody checkpoint only; no signed presence or region travel proof"}
    with (root / "receipt.json").open("x") as file:
        json.dump(receipt, file, indent=2)
    return cached_receipt(root, inputs, source, descriptor, recipe, ancestry)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", type=Path, required=True)
    parser.add_argument("--player", choices=("a", "b"), required=True)
    parser.add_argument("--third-mail-root", type=Path, required=True)
    parser.add_argument("--pc-mail-root", type=Path, required=True)
    parser.add_argument("--held-mail-root", type=Path, required=True)
    parser.add_argument("--output-root", type=Path, required=True)
    parser.add_argument("--mgba-config", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(author_player(json.loads(args.plan.read_text(encoding="utf-8-sig")), args.player,
                                   args.third_mail_root, args.pc_mail_root, args.held_mail_root, args.output_root, args.mgba_config), indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
