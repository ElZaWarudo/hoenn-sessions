"""TEST ONLY: cached normal third-carrier Mail checkpoint, generation4 to5.

Uses current verified first-Mail and PC-Mail parents, never reauthors them.
Buttons and normal Save only; no memory/save patches or server operations.
This stage precedes Daycare and proves neither Daycare nor region travel.
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
import live_author_pc_mail as pc_mail
from live_author_fixtures import cache_key, config_check, seed_bytes
from live_fixture_population import check_population
from live_harness_oracles import OracleFailure, logical_field, read_flash_bytes
import live_region_harness as harness
from live_scripted_input import owned_scripted_emulator

PARENT_DEPS = pc_mail.PARENT_DEPS + ("live_author_pc_mail.py",)
_BAG_START = next(i for i, (label, _) in enumerate(held_mail.CONTROLS) if label == "bag-mail")
CONTROLS = (("third-start", [(8, 8, 60)]),) + tuple(
    ("third-" + label, [(128, 8, 20), (128, 8, 20), (1, 8, 60)]
     if label == "mail-editor" else actions)
    for label, actions in held_mail.CONTROLS[_BAG_START:])


def validate_parent(pc_root: Path, held_root: Path, plan: dict, player: dict,
                    descriptor: bytes, rom: Path, exe: Path, config: Path):
    """Reconstruct current PC-Mail inputs rather than trusting receipt claims."""
    recipe = held_mail.recipe_check(player)
    _, held, held_receipt, ancestry = pc_mail.validate_parent(
        held_root, plan, player, descriptor, rom, exe, config)
    expected = {"player": player["name"], "source_sha256": held.sha256, "recipe": recipe,
                "ancestry": ancestry, "parent_cache_key": held_receipt["cache_key"],
                "parent_receipt_sha256": harness.digest(held_root / "receipt.json"),
                "rom_sha256": harness.digest(rom), "emulator_sha256": harness.digest(exe),
                "config_sha256": harness.digest(config),
                "descriptor_sha256": hashlib.sha256(descriptor).hexdigest(),
                "signed_fixture": held_receipt["inputs"]["signed_fixture"],
                "dependencies": {name: harness.digest(Path(__file__).with_name(name)) for name in PARENT_DEPS}}
    if json.loads((pc_root / "inputs.json").read_text()) != expected:
        raise OracleFailure("third-Mail PC parent current signed/source/dependency inputs differ")
    receipt = pc_mail.cached_receipt(pc_root, expected, held, descriptor, recipe, ancestry)
    data, source = seed_bytes(pc_root / "pc-mail.sav", receipt["validated"]["save_sha256"])
    validate_source(source, descriptor)
    expected_ancestry = ancestry + [{"path": str(source.path.resolve()), "sha256": source.sha256}]
    if receipt["seed_lineage"] != expected_ancestry:
        raise OracleFailure("third-Mail PC parent canonical ancestry differs")
    return data, source, receipt, expected_ancestry


def validate_source(source, descriptor: bytes) -> dict:
    if source.generation != 4:
        raise OracleFailure("third-Mail requires a generation-four PC-Mail parent")
    party = logical_field(source, descriptor, 0x0101)
    mail = logical_field(source, descriptor, 0x010B)
    if len(party) != 604 or party[0] != 6 or len(mail) != 576:
        raise OracleFailure("third-Mail source party/Mail ABI differs")
    if logical_field(source, descriptor, 0x0100)[4:7] not in (bytes((13, 10, 0)), bytes((13, 10, 255))):
        raise OracleFailure("third-Mail source must retain the coherent Main harbor")
    identities = []
    for slot, held, index in ((0, 0, 255), (1, 200, 0), (2, 0, 255)):
        record = party[4 + slot * 100:104 + slot * 100]
        species = held_mail.box_species(record[:80])
        if species is None or species == 201 or 1024 <= species <= 1050:
            raise OracleFailure("third-Mail source carrier absent or unsupported Unown")
        personality, ot_id = struct.unpack_from("<II", record)
        identity = {"species": species, "personality": personality, "ot_id": ot_id}
        held_mail._carrier(record[:80], identity, held)
        if record[85] != index:
            raise OracleFailure("third-Mail source carrier attachment differs")
        identities.append(identity)
    if len({mon["species"] for mon in identities}) != 3:
        raise OracleFailure("third-Mail requires three distinct custody carrier species")
    sender, trainer = held_mail._sender(source, descriptor)
    for index, identity in ((0, identities[1]), (6, identities[0])):
        held_mail._mail(mail[index * 36:(index + 1) * 36],
                        {"words": held_mail.WORDS, "species": identity["species"], "item_id": 200}, sender, trainer)
    if any(struct.unpack_from("<H", mail, index * 36 + 32)[0] for index in range(16) if index not in (0, 6)):
        raise OracleFailure("third-Mail source has unrelated occupied mailbox slots")
    if held_mail._bag(logical_field(source, descriptor, 0x0106))[200] != 1:
        raise OracleFailure("third-Mail source must retain exactly one Bag Harbor Mail")
    return identities[2]


def validate_third_mail(data: bytes, source, descriptor: bytes, recipe: dict, path: Path) -> dict:
    carrier = validate_source(source, descriptor)
    saved = read_flash_bytes(data, path)
    if saved.generation != 5 or saved.lineage != source.lineage:
        raise OracleFailure("third-Mail normal Save generation/trainer lineage differs")
    for fid in (0x0301, 0x0102, 0x0200, 0x0205):
        if logical_field(saved, descriptor, fid) != logical_field(source, descriptor, fid):
            raise OracleFailure("third-Mail changed PC/money/player identity")
    if logical_field(saved, descriptor, 0x0100)[:8] != logical_field(source, descriptor, 0x0100)[:8]:
        raise OracleFailure("third-Mail changed exact harbor location")
    before = logical_field(source, descriptor, 0x0101)
    party = logical_field(saved, descriptor, 0x0101)
    if len(party) != 604 or party[:204] != before[:204] or party[304:] != before[304:]:
        raise OracleFailure("third-Mail changed other party records/count")
    original, current = before[204:304], party[204:304]
    held_mail._carrier(current[:80], carrier, 200)
    if current[85] != 1:
        raise OracleFailure("third-Mail target attachment must use first free slot1")
    old_words, new_words = held_mail._growth(original[:80]), held_mail._growth(current[:80])
    growth = held_mail.GROWTH_OFFSETS[carrier["personality"] % 24] * 3
    old_words[growth] &= ~(1023 << 16)
    new_words[growth] &= ~(1023 << 16)
    if (old_words != new_words or original[:28] != current[:28]
            or original[30:32] != current[30:32] or original[80:85] != current[80:85]
            or original[86:] != current[86:]):
        raise OracleFailure("third-Mail changed unrelated target carrier data")
    old_mail = logical_field(source, descriptor, 0x010B)
    mail = logical_field(saved, descriptor, 0x010B)
    if len(mail) != 576 or mail[:36] != old_mail[:36] or mail[72:] != old_mail[72:]:
        raise OracleFailure("third-Mail changed held/PC/unrelated mailbox bytes")
    sender, trainer = held_mail._sender(source, descriptor)
    message = {"words": held_mail.WORDS, "species": carrier["species"], "item_id": 200}
    held_mail._mail(mail[36:72], message, sender, trainer)
    # GiveMailToMonByItemId writes message/name/trainer/species/item, leaving
    # ABI padding untouched. Validate that padding separately, never patch it.
    if mail[70:72] != old_mail[70:72]:
        raise OracleFailure("third-Mail changed target letter padding")
    expected_bag = held_mail._bag(logical_field(source, descriptor, 0x0106))
    expected_bag[200] -= 1
    if held_mail._bag(logical_field(saved, descriptor, 0x0106)) != expected_bag:
        raise OracleFailure("third-Mail failed to consume last Mail/preserve original Bag")
    population = check_population(saved, descriptor, recipe)
    witnesses = [{"field_id": fid, "sha256": hashlib.sha256(value).hexdigest(),
                  "offset": 0, "size": len(value), "min_nonzero_bytes": 1}
                 for fid, value in ((0x0101, party), (0x010B, mail))]
    return {"source_sha256": source.sha256, "save_sha256": hashlib.sha256(data).hexdigest(),
            "generation": saved.generation, "carrier": carrier, "party_index": 2, "mail_index": 1,
            "message": message, "sender_name_hex": sender.hex(), "trainer_id_hex": trainer.hex(),
            "population": population, "shared_witnesses": witnesses}


def cached_receipt(root: Path, inputs: dict, source, descriptor: bytes, recipe: dict, ancestry: list) -> dict:
    receipt = json.loads((root / "receipt.json").read_text())
    if (json.loads((root / "inputs.json").read_text()) != inputs
            or receipt.get("inputs") != inputs or receipt.get("cache_key") != cache_key(inputs)):
        raise OracleFailure("third-Mail cache current inputs differ")
    data = (root / "third-mail.sav").read_bytes()
    checked = validate_third_mail(data, source, descriptor, recipe, root / "third-mail.sav")
    expected = ancestry + [{"path": str((root / "third-mail.sav").resolve()), "sha256": checked["save_sha256"]}]
    if (receipt.get("validated") != checked or receipt.get("seed_lineage") != expected
            or receipt.get("cold_save_sha256") != checked["save_sha256"]):
        raise OracleFailure("third-Mail cache semantic/ancestry/cold witness differs")
    if any((root / stage / "game.sav").read_bytes() != data for stage in ("author", "cold")):
        raise OracleFailure("third-Mail cached author/retained/cold bytes differ")
    images = receipt.get("screenshots")
    required = {"author/third-first-mail-saved.png", "author/third-gift-message-ready.png", "cold/cold-party.png"}
    if not isinstance(images, dict) or not required <= images.keys():
        raise OracleFailure("third-Mail cache lacks boundary screenshots")
    for relative, digest in images.items():
        path = Path(relative)
        if path.is_absolute() or len(path.parts) != 2 or path.parts[0] not in ("author", "cold") or path.suffix != ".png":
            raise OracleFailure("third-Mail screenshot path differs")
        image = (root / path).read_bytes()
        if not image.startswith(b"\x89PNG\r\n\x1a\n") or hashlib.sha256(image).hexdigest() != digest:
            raise OracleFailure("third-Mail cached screenshot differs")
    return receipt


def author_player(plan: dict, name: str, pc_root: Path, held_root: Path, output: Path, config: Path) -> dict:
    harness.preflight(plan, profiles_ready=False)
    harness.check_c_space()
    players = [player for player in plan["players"] if player["name"] == name]
    if name not in ("a", "b") or len(players) != 1:
        raise OracleFailure("third-Mail must select exactly one plan player a/b")
    player = players[0]
    recipe = held_mail.recipe_check(player)
    if output.resolve().drive.casefold() == Path(os.environ.get("SystemDrive", "C:")).drive.casefold():
        raise OracleFailure("third-Mail requires spare-volume output")
    config_check(config)
    release = Path(plan["release_dir"])
    world = next(w for w in json.loads((release / "release_catalog.json").read_text())["worlds"] if w["world_id"] == 1)
    descriptor = bytes.fromhex(json.loads((release / "server-build-catalog.json").read_text())["shared_player_descriptor_hex"])
    rom, exe = release / world["rom_path"], release / "runtime/mgba.exe"
    harness.require_hash(rom, world["rom_sha256"], "third-Mail signed Main ROM")
    initial, source, parent, ancestry = validate_parent(pc_root, held_root, plan, player, descriptor, rom, exe, config)
    inputs = {"player": name, "source_sha256": source.sha256, "recipe": recipe, "ancestry": ancestry,
              "parent_cache_key": parent["cache_key"], "parent_receipt_sha256": harness.digest(pc_root / "receipt.json"),
              "rom_sha256": harness.digest(rom), "emulator_sha256": harness.digest(exe), "config_sha256": harness.digest(config),
              "descriptor_sha256": hashlib.sha256(descriptor).hexdigest(), "signed_fixture": parent["inputs"]["signed_fixture"],
              "dependencies": {dep: harness.digest(Path(__file__).with_name(dep)) for dep in PARENT_DEPS + ("live_author_third_mail.py",)}}
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
        (folder / "game.sav").write_bytes(initial_bytes)  # Exact source copy only.
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
            raise OracleFailure("third-Mail owned closure changed captured bytes")
        return captured

    authored = run("author", initial, CONTROLS)
    checked = validate_third_mail(authored, source, descriptor, recipe, root / "third-mail.sav")
    with (root / "third-mail.sav").open("xb") as file:
        file.write(authored)
    if (root / "third-mail.sav").read_bytes() != authored:
        raise OracleFailure("third-Mail immutable retention differs")
    cold = run("cold", authored, (("cold-start", [(8, 8, 60)]), ("cold-party", [(1, 8, 180)])))
    if cold != authored:
        raise OracleFailure("third-Mail cold Continue changed retained bytes")
    validate_parent(pc_root, held_root, plan, player, descriptor, rom, exe, config)
    if harness.digest(pc_root / "receipt.json") != inputs["parent_receipt_sha256"]:
        raise OracleFailure("third-Mail PC parent receipt changed during authoring")
    receipt = {"cache_key": cache_key(inputs), "inputs": inputs, "validated": checked,
               "seed_lineage": ancestry + [{"path": str((root / "third-mail.sav").resolve()), "sha256": checked["save_sha256"]}],
               "cold_save_sha256": hashlib.sha256(cold).hexdigest(), "screenshots": images,
               "ui_review": "pending: inspect third gift/Save/cold-party screenshots",
               "scope": "normal third-carrier Mail only; auxiliary time/stats/script/key changes allowed; no Daycare/travel proof"}
    with (root / "receipt.json").open("x") as file:
        json.dump(receipt, file, indent=2)
    return cached_receipt(root, inputs, source, descriptor, recipe, ancestry)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", type=Path, required=True)
    parser.add_argument("--player", choices=("a", "b"), required=True)
    parser.add_argument("--pc-mail-root", type=Path, required=True)
    parser.add_argument("--held-mail-root", type=Path, required=True)
    parser.add_argument("--output-root", type=Path, required=True)
    parser.add_argument("--mgba-config", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(author_player(json.loads(args.plan.read_text(encoding="utf-8-sig")), args.player,
                                   args.pc_mail_root, args.held_mail_root, args.output_root, args.mgba_config), indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
