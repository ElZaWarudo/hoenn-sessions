"""TEST ONLY: normal PC-Mail deposit and replacement held Mail, gen3 to gen4.

Uses verified first-Mail receipts without reauthoring. Button inputs only;
no memory/save patches, server calls, builds, or legacy-save migration.
Both letters use APOLOGIZE; different carrier species distinguish custody slots.
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
from live_author_fixtures import cache_key, config_check, seed_bytes
from live_fixture_population import check_population
from live_harness_oracles import OracleFailure, logical_field, read_flash_bytes
import live_region_harness as harness
from live_scripted_input import owned_scripted_emulator

PARENT_DEPS = ("live_author_mail.py", "live_author_fixtures.py", "live_fixture_population.py",
               "live_fixture_custody.py", "live_scripted_input.py", "live_harness_oracles.py",
               "live_fixture_validation.py", "player_transfer_manifest.py", "live_region_harness.py")
DEPOSIT = (
    ("pc-mail-party", [(8, 8, 60), (1, 8, 180)]),
    # Bounded fixture first carriers have no field-move actions. Summary0,
    # Switch1, Mail2; the Mail submenu is Read0, Take1 (party_menu.c).
    ("pc-mail-actions", [(1, 8, 60), (128, 8, 20), (128, 8, 20), (1, 8, 60)]),
    ("pc-mail-take", [(128, 8, 20), (1, 8, 20)]),
    ("pc-mail-question-ready", [(0, 1, 600)]),
    ("pc-mail-send", [(1, 8, 20)]),
    # gText_MailSentToPC ends PAUSE_UNTIL_PRESS. Dismiss after printing,
    # then separately close Party to the remembered Pokemon Start-menu row.
    ("pc-mail-sent-ready", [(0, 1, 600)]),
    ("pc-mail-party-return", [(2, 8, 180)]),
    ("pc-mail-start-return", [(2, 8, 180)]),
)
# Existing normal Bag Give/EasyChat path, excluding its debug item creation.
_BAG_START = next(i for i, (label, _) in enumerate(held_mail.CONTROLS) if label == "bag-mail")
CONTROLS = DEPOSIT + tuple(("pc-mail-saved" if label == "first-mail-saved" else label,
                           [(128, 8, 20), (1, 8, 60)] if label == "mail-editor" else actions)
                          for label, actions in held_mail.CONTROLS[_BAG_START:])


def validate_parent(root: Path, plan: dict, player: dict, descriptor: bytes, rom: Path, exe: Path, config: Path):
    recipe = held_mail.recipe_check(player)
    _, base = seed_bytes(Path(player["source_save"]), player["source_sha256"])
    if base.generation != 2:
        raise OracleFailure("PC-Mail parent needs the pinned generation-two population")
    check_population(base, descriptor, recipe)
    if logical_field(base, descriptor, 0x0100)[4:7] not in (bytes((13, 10, 0)), bytes((13, 10, 255))):
        raise OracleFailure("PC-Mail parent must retain the coherent Main harbor")
    inputs = json.loads((root / "inputs.json").read_text())
    bindings = {k: plan[k] for k in ("release_id", "envelope_sha256", "catalog_sha256", "server_catalog_sha256")}
    if (not isinstance(inputs, dict) or inputs.get("seed_sha256") != base.sha256
            or inputs.get("recipe") != recipe or inputs.get("menu_profile") != "hoenn-debug-v1"
            or inputs.get("signed_fixture") != bindings
            or inputs.get("descriptor_sha256") != hashlib.sha256(descriptor).hexdigest()
            or inputs.get("dependencies") != {dep: harness.digest(Path(__file__).with_name(dep)) for dep in PARENT_DEPS}):
        raise OracleFailure("PC-Mail held-parent signed/source/current dependencies differ")
    for key, path in (("rom_sha256", rom), ("emulator_sha256", exe), ("config_sha256", config)):
        if inputs.get(key) != harness.digest(path):
            raise OracleFailure("PC-Mail held-parent ROM/emulator/config input differs")
    receipt = held_mail.cached_receipt(root, inputs, base, descriptor, recipe)
    data, source = seed_bytes(root / "first-mail.sav", receipt["first_mail"]["save_sha256"])
    if source.generation != 3 or (root / "author/game.sav").read_bytes() != data:
        raise OracleFailure("PC-Mail held-parent generation/author runtime bytes differ")
    prefix = player.get("seed_lineage")
    if not isinstance(prefix, list) or len(prefix) != 2:
        raise OracleFailure("PC-Mail requires exact generation-one/two ancestry")
    ancestry = []
    for generation, item in enumerate(prefix, start=1):
        if not isinstance(item, dict) or set(item) != {"path", "sha256"}:
            raise OracleFailure("PC-Mail ancestry entry differs")
        _, parsed = seed_bytes(Path(item["path"]), item["sha256"])
        if parsed.generation != generation or parsed.lineage != base.lineage:
            raise OracleFailure("PC-Mail ancestry generation/trainer differs")
        ancestry.append({"path": str(parsed.path.resolve()), "sha256": parsed.sha256})
    if ancestry[-1] != {"path": str(base.path.resolve()), "sha256": base.sha256}:
        raise OracleFailure("PC-Mail ancestry does not end at the pinned source")
    ancestry.append({"path": str(source.path.resolve()), "sha256": source.sha256})
    return data, source, receipt, ancestry


def validate_pc_mail(data: bytes, source, descriptor: bytes, recipe: dict, path: Path) -> dict:
    save = read_flash_bytes(data, path)
    if source.generation != 3 or save.generation != 4 or save.lineage != source.lineage:
        raise OracleFailure("PC-Mail generation/trainer lineage differs")
    for field in (0x0301, 0x0102, 0x0200, 0x0205):
        if logical_field(save, descriptor, field) != logical_field(source, descriptor, field):
            raise OracleFailure("PC-Mail changed carrier/other party/PC/player data")
    if logical_field(save, descriptor, 0x0100)[:8] != logical_field(source, descriptor, 0x0100)[:8]:
        raise OracleFailure("PC-Mail changed exact harbor location")
    party = logical_field(source, descriptor, 0x0101)
    current_party = logical_field(save, descriptor, 0x0101)
    before = logical_field(source, descriptor, 0x010B)
    after = logical_field(save, descriptor, 0x010B)
    if (len(party) != 604 or len(current_party) != 604 or party[0] != 6
            or current_party[:4] != party[:4] or current_party[204:] != party[204:]
            or len(before) != 576 or len(after) != 576):
        raise OracleFailure("PC-Mail needs first carrier mail slot zero and normal ABI")
    # First-Mail validator established the carrier/letter semantics. Recheck
    # attachment and written message independently at this transition boundary.
    carriers = []
    for slot, old_held, new_held, old_index, new_index in ((0, 200, 0, 0, 255), (1, 0, 200, 255, 0)):
        offset = 4 + slot * 100
        original, current = party[offset:offset + 100], current_party[offset:offset + 100]
        species = held_mail.box_species(original[:80])
        if species is None or species == 201 or 1024 <= species <= 1050:
            raise OracleFailure("PC-Mail carrier absent or unsupported Unown")
        personality, ot_id = struct.unpack_from("<II", original)
        carrier = {"species": species, "personality": personality, "ot_id": ot_id}
        held_mail._carrier(original[:80], carrier, old_held)
        held_mail._carrier(current[:80], carrier, new_held)
        if original[85] != old_index or current[85] != new_index:
            raise OracleFailure("PC-Mail carrier attachment index differs")
        old_words, new_words = held_mail._growth(original[:80]), held_mail._growth(current[:80])
        growth = held_mail.GROWTH_OFFSETS[personality % 24] * 3
        old_words[growth] &= ~(1023 << 16)
        new_words[growth] &= ~(1023 << 16)
        if (old_words != new_words or original[:28] != current[:28]
                or original[30:32] != current[30:32] or original[80:85] != current[80:85]
                or original[86:] != current[86:]):
            raise OracleFailure("PC-Mail changed unrelated carrier data")
        carriers.append(carrier)
    if carriers[0]["species"] == carriers[1]["species"]:
        raise OracleFailure("PC-Mail requires distinct carrier species")
    sender, trainer = held_mail._sender(source, descriptor)
    message = {"words": held_mail.WORDS, "species": carriers[0]["species"], "item_id": 200}
    replacement = dict(message, species=carriers[1]["species"])
    held_mail._mail(before[:36], message, sender, trainer)
    if any(struct.unpack_from("<H", before, i * 36 + 32)[0] != 0 for i in range(1, 16)):
        raise OracleFailure("PC-Mail parent must have one held letter and empty PC mailbox")
    # TakeMailFromMonAndSave copies all36 bytes to first free PC index6 and
    # clears only original itemId. Regiving Mail overwrites slot0 normally with
    # second carrier species and same message/sender. This local expected buffer
    # is a read-only oracle, never a save writer; untouched padding stays exact.
    expected_mail = bytearray(before)
    expected_mail[6 * 36:7 * 36] = before[:36]
    struct.pack_into("<H", expected_mail, 30, replacement["species"])
    if after != expected_mail:
        raise OracleFailure("PC-Mail copied letter/replacement/other mailbox bytes differ")
    held_mail._mail(after[:36], replacement, sender, trainer)
    held_mail._mail(after[216:252], message, sender, trainer)
    original_bag = held_mail._bag(logical_field(source, descriptor, 0x0106))
    expected_bag = original_bag.copy()
    if original_bag[200] != 2:
        raise OracleFailure("PC-Mail parent must retain two Bag Harbor Mail")
    expected_bag[200] = 1
    if held_mail._bag(logical_field(save, descriptor, 0x0106)) != expected_bag:
        raise OracleFailure("PC-Mail replacement Bag quantity/other items differ")
    population = check_population(save, descriptor, dict(recipe, bag_items=recipe["bag_items"] + [[200, 1]]))
    witnesses = [{"field_id": 0x0101, "sha256": hashlib.sha256(current_party).hexdigest(), "offset": 0,
                  "size": len(current_party), "min_nonzero_bytes": 1},
                 {"field_id": 0x010B, "sha256": hashlib.sha256(after).hexdigest(), "offset": 0,
                  "size": len(after), "min_nonzero_bytes": 1}]
    return {"save_sha256": hashlib.sha256(data).hexdigest(), "generation": save.generation,
            "source_sha256": source.sha256, "carriers": carriers, "held_party_index": 1,
            "held_mail_index": 0, "pc_mail_index": 6, "pc_message": message,
            "held_message": replacement, "population": population, "shared_witnesses": witnesses}


def cached_receipt(root: Path, inputs: dict, source, descriptor: bytes, recipe: dict, ancestry: list) -> dict:
    receipt = json.loads((root / "receipt.json").read_text())
    if receipt.get("inputs") != inputs or receipt.get("cache_key") != cache_key(inputs):
        raise OracleFailure("PC-Mail cache inputs differ")
    data = (root / "pc-mail.sav").read_bytes()
    checked = validate_pc_mail(data, source, descriptor, recipe, root / "pc-mail.sav")
    expected = ancestry + [{"path": str((root / "pc-mail.sav").resolve()), "sha256": hashlib.sha256(data).hexdigest()}]
    if receipt.get("validated") != checked or receipt.get("seed_lineage") != expected or receipt.get("cold_save_sha256") != checked["save_sha256"]:
        raise OracleFailure("PC-Mail cache semantic/ancestry/cold witness differs")
    if any((root / stage / "game.sav").read_bytes() != data for stage in ("author", "cold")):
        raise OracleFailure("PC-Mail cached runtime save bytes differ")
    images = receipt.get("screenshots")
    if not isinstance(images, dict) or not {"author/pc-mail-saved.png", "cold/cold-party.png", "author/pc-mail-sent-ready.png"} <= images.keys():
        raise OracleFailure("PC-Mail cache lacks boundary screenshots")
    for relative, digest in images.items():
        path = Path(relative)
        if path.is_absolute() or len(path.parts) != 2 or path.parts[0] not in ("author", "cold") or path.suffix != ".png":
            raise OracleFailure("PC-Mail cached screenshot path differs")
        image = (root / path).read_bytes()
        if not image.startswith(b"\x89PNG\r\n\x1a\n") or hashlib.sha256(image).hexdigest() != digest:
            raise OracleFailure("PC-Mail cached screenshot differs")
    return receipt


def author_player(plan: dict, name: str, held_root: Path, output: Path, config: Path) -> dict:
    harness.preflight(plan, profiles_ready=False)
    harness.check_c_space()
    players = [p for p in plan["players"] if p["name"] == name]
    if name not in ("a", "b") or len(players) != 1:
        raise OracleFailure("PC-Mail must select exactly one plan player a/b")
    player = players[0]
    recipe = held_mail.recipe_check(player)
    if output.resolve().drive.casefold() == Path(os.environ.get("SystemDrive", "C:")).drive.casefold():
        raise OracleFailure("PC-Mail requires spare-volume output")
    config_check(config)
    release = Path(plan["release_dir"])
    world = next(w for w in json.loads((release / "release_catalog.json").read_text())["worlds"] if w["world_id"] == 1)
    descriptor = bytes.fromhex(json.loads((release / "server-build-catalog.json").read_text())["shared_player_descriptor_hex"])
    rom, exe = release / world["rom_path"], release / "runtime/mgba.exe"
    harness.require_hash(rom, world["rom_sha256"], "PC-Mail signed Main ROM")
    initial, source, parent, ancestry = validate_parent(held_root, plan, player, descriptor, rom, exe, config)
    inputs = {"player": name, "source_sha256": source.sha256, "recipe": recipe, "ancestry": ancestry,
              "parent_cache_key": parent["cache_key"], "parent_receipt_sha256": harness.digest(held_root / "receipt.json"),
              "rom_sha256": harness.digest(rom), "emulator_sha256": harness.digest(exe), "config_sha256": harness.digest(config),
              "descriptor_sha256": hashlib.sha256(descriptor).hexdigest(), "signed_fixture": parent["inputs"]["signed_fixture"],
              "dependencies": {dep: harness.digest(Path(__file__).with_name(dep)) for dep in PARENT_DEPS + ("live_author_pc_mail.py",)}}
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
        (folder / "game.sav").write_bytes(initial_bytes)  # Exact retained copy only.
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
                for i, (mask, hold, wait) in enumerate(group):
                    image = script.act(label if i == len(group) - 1 else f"{label}-{i}", mask, hold=hold, wait=wait)
                    images[f"{stage}/{image.name}"] = harness.digest(image)
            captured = (folder / "game.sav").read_bytes()
        if (folder / "game.sav").read_bytes() != captured:
            raise OracleFailure("PC-Mail closure changed captured bytes")
        return captured

    authored = run("author", initial, CONTROLS)
    checked = validate_pc_mail(authored, source, descriptor, recipe, root / "pc-mail.sav")
    with (root / "pc-mail.sav").open("xb") as file:
        file.write(authored)
    if (root / "pc-mail.sav").read_bytes() != authored:
        raise OracleFailure("PC-Mail immutable retention differs")
    cold = run("cold", authored, (("cold-start", [(8, 8, 60)]), ("cold-party", [(1, 8, 180)])))
    if cold != authored:
        raise OracleFailure("PC-Mail cold Continue changed retained bytes")
    validate_parent(held_root, plan, player, descriptor, rom, exe, config)
    if harness.digest(held_root / "receipt.json") != inputs["parent_receipt_sha256"]:
        raise OracleFailure("PC-Mail held-parent receipt changed during authoring")
    receipt = {"cache_key": cache_key(inputs), "inputs": inputs, "validated": checked,
               "seed_lineage": ancestry + [{"path": str((root / "pc-mail.sav").resolve()), "sha256": hashlib.sha256(authored).hexdigest()}],
               "cold_save_sha256": hashlib.sha256(cold).hexdigest(), "screenshots": images,
               "ui_review": "pending: inspect Party Take/Save/cold-party screenshots",
               "scope": "normal PC-mail deposit/second-carrier replacement only; distinct species, no daycare/travel proof"}
    with (root / "receipt.json").open("x") as file:
        json.dump(receipt, file, indent=2)
    return cached_receipt(root, inputs, source, descriptor, recipe, ancestry)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", type=Path, required=True)
    parser.add_argument("--player", choices=("a", "b"), required=True)
    parser.add_argument("--held-mail-root", type=Path, required=True)
    parser.add_argument("--output-root", type=Path, required=True)
    parser.add_argument("--mgba-config", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(author_player(json.loads(args.plan.read_text(encoding="utf-8-sig")), args.player,
                                   args.held_mail_root, args.output_root, args.mgba_config), indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
