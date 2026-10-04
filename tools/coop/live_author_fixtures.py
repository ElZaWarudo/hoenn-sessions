"""Test-only, cached ROM menu authoring; never patches a save or contacts a server."""
from __future__ import annotations

import argparse
import configparser
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

from live_fixture_population import check_population
from live_harness_oracles import OracleFailure, logical_field, read_flash_bytes
import live_region_harness as harness


def recipe_check(recipe: dict) -> None:
    # This bounded menu itinerary starts with an empty party/PC. It is not a
    # general save editor; other ABIs/menu families require an explicit driver.
    if recipe.get("abi") != "hoenn-box80-v1":
        raise OracleFailure("unsupported population ABI")
    if not isinstance(recipe.get("party_species"), list) or not isinstance(recipe.get("pc_species"), list):
        raise OracleFailure("authoring species must be lists")
    species = recipe["party_species"] + recipe["pc_species"]
    if len(recipe.get("party_species", [])) != 6 or len(recipe.get("pc_species", [])) != 1:
        raise OracleFailure("authoring requires six party gifts and one PC gift")
    if any(type(s) is not int or not 1 <= s <= 32 for s in species):
        raise OracleFailure("bounded authoring species must be 1..32")
    level = recipe.get("party_level")
    if type(level) is not int or not 1 <= level <= 20:
        raise OracleFailure("bounded authoring level must be 1..20")
    items = recipe.get("bag_items")
    if (not isinstance(items, list) or len(items) != 1
            or not isinstance(items[0], list) or len(items[0]) != 2
            or type(items[0][0]) is not int or items[0][0] != 2):
        raise OracleFailure("bounded authoring requires one Great Ball recipe")
    quantity = items[0][1]
    if type(quantity) is not int or not 1 <= quantity <= 20:
        raise OracleFailure("bounded authoring quantity must be 1..20")


def config_check(path: Path) -> None:
    config = configparser.ConfigParser()
    config.read(path, encoding="utf-8")
    expected = {"keyRight": 16777236, "keyDown": 16777237, "keyR": 83,
                "keyB": 90, "keyUp": 16777235, "keyLeft": 16777234,
                "keyA": 88, "keyStart": 16777220}
    if not config.has_section("gba.input.QT_K") or any(
            config.getint("gba.input.QT_K", k, fallback=-1) != v for k, v in expected.items()):
        raise OracleFailure("authoring mGBA keyboard mapping differs from tested itinerary")


def retain_population(data: bytes, target: Path, descriptor: bytes,
                      recipe: dict, generation: int, lineage: bytes | None = None) -> dict:
    """Validate/hash/write the same immutable read; never copy a later live file."""
    save = read_flash_bytes(data, target)
    if save.generation != generation:
        raise OracleFailure("authoring: unexpected ROM save generation")
    if lineage is not None and save.lineage != lineage:
        raise OracleFailure("authoring: trainer lineage changed")
    result = check_population(save, descriptor, recipe)
    with target.open("xb") as out:
        out.write(data)
    if target.read_bytes() != data:
        raise OracleFailure("authoring: retained save bytes changed")
    return result


def seed_bytes(path: Path, expected: str):
    data = path.read_bytes()
    if hashlib.sha256(data).hexdigest() != expected:
        raise OracleFailure("authoring seed bytes changed")
    return data, read_flash_bytes(data, path)


@contextmanager
def owned_emulator(exe: Path, root: Path, env: dict, stage: str):
    with (root / (stage + "-stdout.log")).open("wb") as out, (root / (stage + "-stderr.log")).open("wb") as err:
        process = subprocess.Popen([str(exe), str(root / "game.gba")], cwd=root, env=env, stdout=out, stderr=err)
        primary = None
        try:
            yield process
        except BaseException as exc:
            primary = exc
            raise
        finally:
            try:
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=10)
            except Exception as cleanup:
                if primary is None:
                    raise
                primary.add_note("owned emulator cleanup failed: " + str(cleanup))


def cache_key(inputs: dict) -> str:
    return hashlib.sha256(json.dumps(inputs, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def cached_receipt(root: Path, inputs: dict, descriptor: bytes, recipe: dict) -> dict:
    receipt = json.loads((root / "receipt.json").read_text())
    if receipt.get("inputs") != inputs or receipt.get("cache_key") != cache_key(inputs):
        raise OracleFailure("authoring cache inputs changed")
    data = (root / "population.sav").read_bytes()
    actual = check_population(read_flash_bytes(data, root / "population.sav"), descriptor, recipe)
    if receipt.get("population") != actual:
        raise OracleFailure("authoring cache population changed")
    if receipt.get("cold_save_sha256") != actual["save_sha256"]:
        raise OracleFailure("authoring cache lacks exact cold-reload bytes")
    for name, digest in receipt.get("screenshots", {}).items():
        if Path(name).name != name or harness.digest(root / name) != digest:
            raise OracleFailure("authoring cache screenshot changed")
    if "cold-party.png" not in receipt.get("screenshots", {}):
        raise OracleFailure("authoring cache lacks cold screenshot")
    return receipt


def author_player(plan: dict, player: dict, output: Path, config: Path,
                  world_id: int) -> dict:
    from live_harness_windows import Win32Adapter, wait_game_window, tap, focus_game, capture_game
    recipe = player["population_recipe"]
    recipe_check(recipe)
    if player.get("authoring_menu_profile") != "hoenn-debug-v1":
        raise OracleFailure("authoring needs explicit hoenn-debug-v1 menu profile")
    release = Path(plan["release_dir"])
    catalog = json.loads((release / "release_catalog.json").read_text())
    world = next(w for w in catalog["worlds"] if w["world_id"] == world_id)
    descriptor = bytes.fromhex(json.loads((release / "server-build-catalog.json").read_text())["shared_player_descriptor_hex"])
    seed = Path(player["source_save"])
    initial_bytes, source = seed_bytes(seed, player["source_sha256"])
    rom, exe = release / world["rom_path"], release / "runtime/mgba.exe"
    harness.require_hash(rom, world["rom_sha256"], "authoring ROM")
    inputs = {"rom_sha256": world["rom_sha256"], "seed_sha256": source.sha256,
              "emulator_sha256": harness.digest(exe), "descriptor_sha256": hashlib.sha256(descriptor).hexdigest(),
              "config_sha256": harness.digest(config), "recipe": recipe, "menu_profile": "hoenn-debug-v1",
              "driver_sha256": harness.digest(Path(__file__)),
              "validator_sha256": harness.digest(Path(__file__).with_name("live_fixture_population.py")),
              "windows_sha256": harness.digest(Path(__file__).with_name("live_harness_windows.py")),
              "oracle_sha256": harness.digest(Path(__file__).with_name("live_harness_oracles.py"))}
    root = output / cache_key(inputs)
    if root.exists():
        return cached_receipt(root, inputs, descriptor, recipe)
    party, pc = logical_field(source, descriptor, 0x0101), logical_field(source, descriptor, 0x0301)
    if party[0] or any(pc[i + 19] & 2 for i in range(0, len(pc), 80)):
        raise OracleFailure("authoring requires empty source party and PC")
    root.mkdir(parents=True, exist_ok=False)
    (root / "inputs.json").write_text(json.dumps(inputs, indent=2))
    shutil.copyfile(rom, root / "game.gba")
    (root / "game.sav").write_bytes(initial_bytes)
    env = dict(os.environ, APPDATA=str(root / "appdata"), LOCALAPPDATA=str(root / "localappdata"))
    local_config = root / "appdata/mGBA/config.ini"
    local_config.parent.mkdir(parents=True)
    shutil.copyfile(config, local_config)
    adapter = Win32Adapter()
    snapshots = {}

    def screenshot(pid: int, label: str) -> None:
        capture_game(pid, root, label, adapter)
        name = label + ".png"
        snapshots[name] = harness.digest(root / name)

    def run(stage: str, actions):
        with owned_emulator(exe, root, env, stage) as process:
            wait_game_window(process.pid, adapter, timeout=30, rom_title="POKEMON EMER")
            time.sleep(4)
            for key, wait in [("gba_b", 20), ("gba_start", 10), ("gba_start", 4), ("gba_a", 5)]:
                tap(process.pid, key, adapter)
                time.sleep(wait)
            actions(process.pid)

    population = None

    def populate(pid: int) -> None:
        nonlocal population
        def key(k: str, count: int = 1, wait: float = .25):
            for _ in range(count):
                tap(pid, k, adapter)
                time.sleep(wait)
        def give_menu():
            focus_game(pid, adapter)
            adapter.key(0x53, True)
            try:
                time.sleep(.1)
                tap(pid, "gba_start", adapter)
            finally:
                adapter.key(0x53, False)
            time.sleep(.7)
            key("down", 3)
            key("gba_a", wait=.5)
        for species in recipe["party_species"] + recipe["pc_species"]:
            give_menu()
            key("down")
            key("gba_a", wait=.5)
            key("up", species - 1)
            key("gba_a")
            key("up", recipe["party_level"] - 1)
            key("gba_a", wait=1)
            screenshot(pid, "gift-" + str(species))
        give_menu()
        key("gba_a")
        key("up")
        key("gba_a")
        key("up", recipe["bag_items"][0][1] - 1)
        key("gba_a", wait=1)
        key("gba_start", wait=1)
        key("up", 3)
        key("gba_a", wait=2)
        key("gba_a", wait=2)
        for attempt in range(8):
            key("gba_a", wait=2)
            data = (root / "game.sav").read_bytes()
            try:
                current = read_flash_bytes(data, root / "game.sav")
            except OracleFailure:
                continue  # ROM may be midway through sector writes.
            screenshot(pid, "save-step-" + str(attempt))
            if current.generation > source.generation:
                population = retain_population(data, root / "population.sav", descriptor, recipe, source.generation + 1, source.lineage)
                return
        raise OracleFailure("normal Save did not produce a valid new generation")

    run("author", populate)
    retained = (root / "population.sav").read_bytes()
    (root / "game.sav").write_bytes(retained)  # Copy only; never alters retained bytes.

    def cold(pid: int) -> None:
        tap(pid, "gba_start", adapter)
        time.sleep(1)
        tap(pid, "gba_a", adapter)
        time.sleep(3)
        screenshot(pid, "cold-party")
    run("cold", cold)
    if (root / "game.sav").read_bytes() != retained:
        raise OracleFailure("cold reload changed fixture bytes")
    receipt = {"cache_key": cache_key(inputs), "inputs": inputs, "population": population,
               "cold_save_sha256": hashlib.sha256(retained).hexdigest(), "screenshots": snapshots,
               "ui_review": "pending: inspect cold-party.png; byte checks do not recognize UI",
               "scope": "ROM gifts and normal Save; source lineage ancestry unchanged; no server writes"}
    with (root / "receipt.json").open("x") as out:
        json.dump(receipt, out, indent=2)
    return cached_receipt(root, inputs, descriptor, recipe)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", type=Path, required=True)
    parser.add_argument("--output-root", type=Path, required=True)
    parser.add_argument("--mgba-config", type=Path, required=True)
    parser.add_argument("--world-id", type=int, required=True)
    args = parser.parse_args()
    if os.name != "nt":
        parser.error("live authoring requires Windows")
    # S: and volume GUID paths work; reject C: aliases as well as literal C:.
    if args.output_root.resolve().drive.casefold() == Path(os.environ.get("SystemDrive", "C:")).drive.casefold():
        parser.error("large authoring outputs must use the spare volume")
    plan = json.loads(args.plan.read_text(encoding="utf-8-sig"))
    harness.preflight(plan, profiles_ready=False)
    config_check(args.mgba_config)
    for player in plan["players"]:
        recipe_check(player["population_recipe"])
        if player.get("authoring_menu_profile") != "hoenn-debug-v1":
            parser.error("both players require an explicit supported authoring menu profile")
    results = []
    for player in plan["players"]:
        harness.check_c_space()
        try:
            receipt = author_player(plan, player, args.output_root, args.mgba_config, args.world_id)
        except Exception as exc:
            args.output_root.mkdir(parents=True, exist_ok=True)
            failure = {"player": player["name"], "release_id": plan["release_id"],
                       "error_type": type(exc).__name__, "error": str(exc),
                       "scope": "authoring failed; no server writes; retain failed cache directory"}
            with (args.output_root / ("failure-" + str(time.time_ns()) + ".json")).open("x") as out:
                json.dump(failure, out, indent=2)
            raise
        results.append({"name": player["name"], "receipt": receipt})
    print(json.dumps(results, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
