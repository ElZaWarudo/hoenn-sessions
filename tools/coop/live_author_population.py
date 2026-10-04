"""TEST ONLY: populate a validated clean harbor receipt using ROM debug menus.

Retains ROM-written generation-one ancestry and generation-two normal Save.
No harbor reauthoring, save patches, memory writes, server calls, or migration.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct

from live_author_fixtures import cache_key, config_check, recipe_check, seed_bytes
import live_author_harbor as harbor
from live_fixture_population import GROWTH_OFFSETS, box_species, check_population
from live_harness_oracles import OracleFailure, logical_field, read_flash_bytes
import live_region_harness as harness
from live_scripted_input import owned_scripted_emulator

HARBOR_DEPS = ("live_author_harbor.py", "live_author_fixtures.py", "live_fixture_population.py",
               "live_scripted_input.py", "live_harness_oracles.py", "player_transfer_manifest.py",
               "live_region_harness.py", "live_fixture_validation.py", "live_fixture_custody.py")


def controls(recipe: dict):
    recipe_check(recipe)
    enter = [(264, 8, 60)] + [(128, 8, 20)] * 3 + [(1, 8, 60)]
    result = []
    for slot, species in enumerate(recipe["party_species"] + recipe["pc_species"]):
        result.append((f"gift-{slot}", enter + [(128, 8, 20), (1, 8, 60)]
                       + [(64, 8, 20)] * (species - 1) + [(1, 8, 60)]
                       + [(64, 8, 20)] * (recipe["party_level"] - 1) + [(1, 8, 180)]))
    result += [("bag-gift", enter + [(1, 8, 60), (64, 8, 20), (1, 8, 60)]
                + [(64, 8, 20)] * (recipe["bag_items"][0][1] - 1) + [(1, 8, 180)]),
               # Cold boot resets sStartMenuCursorPos to zero. Gifts add only
               # Pokemon: Pokemon/Bag/Player/Online/Character/Save is row 5.
               ("population-save-menu", [(8, 8, 90)]),
               ("population-save-prompt", [(128, 8, 20)] * 5 + [(1, 8, 60)]),
               ("population-save-ready", [(0, 1, 300)]),
               ("population-overwrite-prompt", [(1, 8, 60)]),
               ("population-overwrite-ready", [(0, 1, 300)]),
               ("population-saved", [(1, 8, 600)])]
    return tuple(result)


def validate_harbor_root(root: Path, plan: dict, name: str, descriptor: bytes, rom: Path, exe: Path, config: Path):
    if name not in ("a", "b"):
        raise OracleFailure("population needs plan player a or b")
    inputs = json.loads((root / "inputs.json").read_text())
    if not isinstance(inputs, dict):
        raise OracleFailure("population harbor inputs must be an object")
    expected_bindings = {k: plan[k] for k in ("release_id", "envelope_sha256", "catalog_sha256", "server_catalog_sha256")}
    if (inputs.get("name") != name.upper() or inputs.get("initial_save") != "absent"
            or inputs.get("menu_profile") != "hoenn-debug-v1" or inputs.get("signed_fixture") != expected_bindings
            or inputs.get("pillow_version") != harbor.PILLOW_VERSION
            or inputs.get("intro_templates") != {key: {"roi": list(value[0]), "sha256": value[1]} for key, value in harbor.TEMPLATES.items()}
            or inputs.get("descriptor_sha256") != hashlib.sha256(descriptor).hexdigest()
            or inputs.get("dependencies") != {dep: harness.digest(Path(__file__).with_name(dep)) for dep in HARBOR_DEPS}):
        raise OracleFailure("population harbor receipt ownership/signed/current inputs differ")
    for field, path in (("rom_sha256", rom), ("emulator_sha256", exe), ("config_sha256", config)):
        if inputs.get(field) != harness.digest(path):
            raise OracleFailure("population harbor ROM/emulator/config input differs")
    receipt = harbor.cached_receipt(root, inputs, descriptor, name.upper())
    data, source = seed_bytes(root / "harbor.sav", receipt["harbor"]["save_sha256"])
    if source.generation != 1:
        raise OracleFailure("population harbor ancestry must start at generation one")
    # Harbor helper proves canonical empty party/PC; gifts require empty Bag.
    bag = logical_field(source, descriptor, 0x0106)
    if len(bag) % 4 or any(item or quantity for item, quantity in struct.iter_unpack("<HH", bag)):
        raise OracleFailure("population harbor Bag is dirty")
    return data, source, receipt


def validate_population(data: bytes, source, descriptor: bytes, recipe: dict, path: Path) -> dict:
    save = read_flash_bytes(data, path)
    if save.generation != 2 or source.generation != 1 or save.lineage != source.lineage:
        raise OracleFailure("population generation-one ancestry/generation-two lineage differs")
    for field in (0x0102, 0x0200, 0x0205):
        if logical_field(save, descriptor, field) != logical_field(source, descriptor, field):
            raise OracleFailure("population changed player identity or money")
    if logical_field(save, descriptor, 0x0100)[:8] != logical_field(source, descriptor, 0x0100)[:8]:
        raise OracleFailure("population changed harbor location")
    population = check_population(save, descriptor, recipe)
    trainer = logical_field(source, descriptor, 0x0205)
    name = logical_field(source, descriptor, 0x0200)
    party = logical_field(save, descriptor, 0x0101)
    pc = logical_field(save, descriptor, 0x0301)
    boxes = [(f"party-{i}", party[4 + i * 100:84 + i * 100]) for i in range(6)]
    boxes += [(f"pc-{i // 80}", pc[i:i + 80]) for i in range(0, len(pc), 80) if pc[i + 19] & 2]
    identities = []
    for slot, record in boxes:
        species = box_species(record)  # Validate checksum before decryption.
        personality, ot_id = struct.unpack_from("<II", record)
        ot_name = record[20:27]
        if record[4:8] != trainer or 255 not in ot_name or ot_name.split(b"\xff", 1)[0] != name.split(b"\xff", 1)[0]:
            raise OracleFailure("population gift OT differs from original ROM trainer")
        words = [word ^ personality ^ ot_id for (word,) in struct.iter_unpack("<I", record[32:80])]
        # This fork's PokemonSubstruct0 packs heldItem:10 followed by six
        # unused_02/met-location bits (include/pokemon.h). Preserve those
        # upper bits in witnesses; they are not a 16-bit held item.
        held = (words[GROWTH_OFFSETS[personality % 24] * 3] >> 16) & 0x3FF
        if held != 0:
            raise OracleFailure("population gift unexpectedly holds an item")
        identities.append({"slot": slot, "species": species, "personality": personality,
                           "ot_id": ot_id, "ot_name_hex": ot_name.hex(), "held_item": held,
                           "box_sha256": hashlib.sha256(record).hexdigest()})
    return {"population": population, "identities": identities, "source_sha256": source.sha256,
            "lineage_hex": source.lineage.hex()}


def cached_receipt(root: Path, inputs: dict, source, descriptor: bytes, recipe: dict) -> dict:
    receipt = json.loads((root / "receipt.json").read_text())
    if receipt.get("inputs") != inputs or receipt.get("cache_key") != cache_key(inputs):
        raise OracleFailure("population cache inputs differ")
    data = (root / "population.sav").read_bytes()
    checked = validate_population(data, source, descriptor, recipe, root / "population.sav")
    expected_lineage = [{"path": str(source.path.resolve()), "sha256": source.sha256},
                        {"path": str((root / "population.sav").resolve()),
                         "sha256": hashlib.sha256(data).hexdigest()}]
    if receipt.get("seed_lineage") != expected_lineage:
        raise OracleFailure("population cache ancestry export differs")
    if receipt.get("validated") != checked or receipt.get("cold_save_sha256") != hashlib.sha256(data).hexdigest():
        raise OracleFailure("population cache semantic/cold witness differs")
    if any((root / stage / "game.sav").read_bytes() != data for stage in ("author", "cold")):
        raise OracleFailure("population cached runtime save bytes differ")
    images = receipt.get("screenshots")
    required = {"author/population-saved.png", "cold/cold-party.png"} | {f"author/gift-{i}.png" for i in range(7)}
    if not isinstance(images, dict) or not required <= images.keys():
        raise OracleFailure("population cache lacks required screenshots")
    for relative, digest in images.items():
        path = Path(relative)
        if path.is_absolute() or len(path.parts) != 2 or path.parts[0] not in ("author", "cold") or path.suffix != ".png":
            raise OracleFailure("population cached screenshot path differs")
        image = (root / path).read_bytes()
        if not image.startswith(b"\x89PNG\r\n\x1a\n") or hashlib.sha256(image).hexdigest() != digest:
            raise OracleFailure("population cached screenshot differs")
    return receipt


def author_player(plan: dict, name: str, harbor_root: Path, output: Path, config: Path) -> dict:
    harness.preflight(plan, profiles_ready=False)
    harness.check_c_space()
    players = [p for p in plan["players"] if p["name"] == name]
    if name not in ("a", "b") or len(players) != 1 or players[0].get("authoring_menu_profile") != "hoenn-debug-v1":
        raise OracleFailure("population must select exactly one supported plan player")
    recipe = players[0]["population_recipe"]
    itinerary = controls(recipe)
    if output.resolve().drive.casefold() == Path(os.environ.get("SystemDrive", "C:")).drive.casefold():
        raise OracleFailure("population requires spare-volume output")
    config_check(config)
    release = Path(plan["release_dir"])
    world = next(w for w in json.loads((release / "release_catalog.json").read_text())["worlds"] if w["world_id"] == 1)
    descriptor = bytes.fromhex(json.loads((release / "server-build-catalog.json").read_text())["shared_player_descriptor_hex"])
    rom, exe = release / world["rom_path"], release / "runtime/mgba.exe"
    harness.require_hash(rom, world["rom_sha256"], "population signed Main ROM")
    initial, source, harbor_receipt = validate_harbor_root(harbor_root, plan, name, descriptor, rom, exe, config)
    inputs = {"player": name, "source_sha256": source.sha256, "recipe": recipe,
              "harbor_cache_key": harbor_receipt["cache_key"], "harbor_receipt_sha256": harness.digest(harbor_root / "receipt.json"),
              "rom_sha256": harness.digest(rom), "emulator_sha256": harness.digest(exe), "config_sha256": harness.digest(config),
              "descriptor_sha256": hashlib.sha256(descriptor).hexdigest(),
              "signed_fixture": harbor_receipt["inputs"]["signed_fixture"],
              "dependencies": {dep: harness.digest(Path(__file__).with_name(dep)) for dep in HARBOR_DEPS + ("live_author_population.py",)}}
    root = output / cache_key(inputs)
    if root.exists():
        return cached_receipt(root, inputs, source, descriptor, recipe)
    root.mkdir(parents=True, exist_ok=False)
    (root / "inputs.json").write_text(json.dumps(inputs, indent=2))
    images = {}

    def run(stage, initial_bytes, actions):
        folder = root / stage
        folder.mkdir()
        shutil.copyfile(rom, folder / "game.gba")
        (folder / "game.sav").write_bytes(initial_bytes)  # Exact copy, never patched.
        local = folder / "appdata/mGBA/config.ini"
        local.parent.mkdir(parents=True)
        shutil.copyfile(config, local)
        env = dict(os.environ, APPDATA=str(folder / "appdata"), LOCALAPPDATA=str(folder / "localappdata"))
        with owned_scripted_emulator(exe, folder, env) as script:
            for label, mask, hold, wait in harbor.COLD:
                image = script.act(label, mask, hold=hold, wait=wait)
                images[f"{stage}/{image.name}"] = harness.digest(image)
            for label, group in actions:
                harness.check_c_space()
                for i, (mask, hold, wait) in enumerate(group):
                    image = script.act(label if i == len(group) - 1 else f"{label}-{i}", mask, hold=hold, wait=wait)
                    images[f"{stage}/{image.name}"] = harness.digest(image)
            captured = (folder / "game.sav").read_bytes()
        if (folder / "game.sav").read_bytes() != captured:
            raise OracleFailure("population closure changed captured save")
        return captured

    authored = run("author", initial, itinerary)
    checked = validate_population(authored, source, descriptor, recipe, root / "population.sav")
    with (root / "population.sav").open("xb") as file:
        file.write(authored)
    if (root / "population.sav").read_bytes() != authored:
        raise OracleFailure("population immutable retained bytes differ")
    cold = run("cold", authored, (("cold-start", [(8, 8, 60)]), ("cold-party", [(1, 8, 180)])))
    if cold != authored:
        raise OracleFailure("population cold Continue changed retained bytes")
    validate_harbor_root(harbor_root, plan, name, descriptor, rom, exe, config)
    if harness.digest(harbor_root / "receipt.json") != inputs["harbor_receipt_sha256"]:
        raise OracleFailure("population harbor receipt changed during authoring")
    receipt = {"cache_key": cache_key(inputs), "inputs": inputs, "validated": checked,
               "cold_save_sha256": hashlib.sha256(cold).hexdigest(), "screenshots": images,
               "seed_lineage": [{"path": str(source.path.resolve()), "sha256": source.sha256},
                                {"path": str((root / "population.sav").resolve()), "sha256": hashlib.sha256(authored).hexdigest()}],
               "ui_review": "pending: inspect gift/Save/cold-party screenshots",
               "scope": "ROM gifts/gen2 normal Save/cold Continue only; no custody/server/travel proof"}
    with (root / "receipt.json").open("x") as file:
        json.dump(receipt, file, indent=2)
    return cached_receipt(root, inputs, source, descriptor, recipe)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", type=Path, required=True)
    parser.add_argument("--player", choices=("a", "b"), required=True)
    parser.add_argument("--harbor-root", type=Path, required=True)
    parser.add_argument("--output-root", type=Path, required=True)
    parser.add_argument("--mgba-config", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(author_player(json.loads(args.plan.read_text(encoding="utf-8-sig")), args.player,
                                   args.harbor_root, args.output_root, args.mgba_config), indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
