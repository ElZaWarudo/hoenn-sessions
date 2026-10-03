"""TEST ONLY: derive an immutable seed-only plan from verified custody caches.

No registration, account, API, emulator or profile operations occur here.
Before signed travel, replace character IDs with actual IDs from the seed
receipt and prepare/verify those profiles. Original IDs are preserved as draft
placeholders. Region/portal/leg configuration remains owned by the base plan.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
from pathlib import Path

import live_author_daycare as daycare
from live_author_fixtures import config_check, seed_bytes
from live_fixture_validation import validate_player_fixture
from live_harness_oracles import OracleFailure, logical_field
import live_region_harness as harness
from live_seed_players import validate_lineages

ROOT_KEYS = {"held_mail_root", "pc_mail_root", "third_mail_root", "daycare_root"}
CUSTODY_FIELDS = {0x0101, 0x010B, 0x010D}


def _parents(mapping: dict) -> dict:
    if not isinstance(mapping, dict) or set(mapping) != {"a", "b"}:
        raise OracleFailure("custody plan needs exactly a/b parent mappings")
    checked = {}
    for name, item in mapping.items():
        if not isinstance(item, dict) or set(item) != ROOT_KEYS:
            raise OracleFailure("custody parent mapping fields differ")
        checked[name] = {}
        for key, value in item.items():
            if not isinstance(value, str) or not value.strip():
                raise OracleFailure("custody parent root needs an absolute path")
            path = Path(value)
            if not path.is_absolute() or not path.is_dir():
                raise OracleFailure("custody parent directory must already exist")
            checked[name][key] = path.resolve()
    return checked


def validate_cache(roots: dict, base: dict, player: dict, descriptor: bytes, rom: Path, exe: Path, config: Path):
    """Reconstruct the exact current Daycare author inputs before cache use."""
    _, source, parent, ancestry = daycare.validate_parent(roots["third_mail_root"], roots["pc_mail_root"],
        roots["held_mail_root"], base, player, descriptor, rom, exe, config)
    recipe = daycare.held_mail.recipe_check(player)
    expected = {"player": player["name"], "source_sha256": source.sha256, "recipe": recipe, "ancestry": ancestry,
        "parent_cache_key": parent["cache_key"], "parent_receipt_sha256": harness.digest(roots["third_mail_root"] / "receipt.json"),
        "rom_sha256": harness.digest(rom), "emulator_sha256": harness.digest(exe), "config_sha256": harness.digest(config),
        "descriptor_sha256": hashlib.sha256(descriptor).hexdigest(), "signed_fixture": parent["inputs"]["signed_fixture"],
        "max_post_deposit_steps": daycare.MAX_POST_DEPOSIT_STEPS, "max_total_steps": daycare.MAX_TOTAL_STEPS,
        "dependencies": {dep: harness.digest(Path(__file__).with_name(dep))
                         for dep in daycare.PARENT_DEPS + ("live_author_daycare.py",)}}
    root = roots["daycare_root"]
    if json.loads((root / "inputs.json").read_text()) != expected:
        raise OracleFailure("custody Daycare cache current signed/source/dependency inputs differ")
    receipt = daycare.cached_receipt(root, expected, source, descriptor, recipe, ancestry)
    _, final = seed_bytes(root / "daycare.sav", receipt["validated"]["save_sha256"])
    chain = ancestry + [{"path": str(final.path.resolve()), "sha256": final.sha256}]
    if final.generation != 6 or receipt["seed_lineage"] != chain or len(chain) != 6:
        raise OracleFailure("custody cache must export canonical generation-one through-six ancestry")
    return final, receipt


def merge_witnesses(original: list, required: list) -> list:
    if not isinstance(original, list) or not isinstance(required, list):
        raise OracleFailure("custody plan witnesses must be lists")
    for witnesses in (original, required):
        ids = [w.get("field_id") for w in witnesses if isinstance(w, dict)]
        if len(ids) != len(witnesses) or any(type(fid) is not int for fid in ids) or len(ids) != len(set(ids)):
            raise OracleFailure("custody plan witnesses contain malformed/duplicate fields")
    if {w["field_id"] for w in required} != CUSTODY_FIELDS:
        raise OracleFailure("custody plan needs all three full-field custody witnesses")
    return copy.deepcopy([w for w in original if w["field_id"] not in CUSTODY_FIELDS] + required)


def derive_plan(base: dict, parents: dict, config: Path) -> dict:
    harness.preflight(base, profiles_ready=False)
    harness.check_c_space()
    _, server = harness._health_url(base)
    if server.hostname not in ("localhost", "127.0.0.1", "::1"):
        raise OracleFailure("custody seed draft requires a loopback server")
    actors = base.get("players")
    if (not isinstance(actors, list) or len(actors) != 2
            or {player.get("name") for player in actors if isinstance(player, dict)} != {"a", "b"}):
        raise OracleFailure("custody plan requires exactly actors a/b")
    # This factory's Main menu ABI names actors A/B. Preserve the base profile
    # paths but refuse accidentally switched actor directories.
    profiles = [Path(player["profile_localappdata"]).resolve() for player in actors]
    if (len({str(path).casefold() for path in profiles}) != 2
            or any(path.name.casefold() != player["name"] for path, player in zip(profiles, actors))):
        raise OracleFailure("custody actor profile paths are duplicated or swapped")
    roots = _parents(parents)
    config_check(config)
    release = Path(base["release_dir"])
    world = next(w for w in json.loads((release / "release_catalog.json").read_text())["worlds"] if w["world_id"] == 1)
    descriptor = bytes.fromhex(json.loads((release / "server-build-catalog.json").read_text())["shared_player_descriptor_hex"])
    rom, exe = release / world["rom_path"], release / "runtime/mgba.exe"
    harness.require_hash(rom, world["rom_sha256"], "custody signed Main ROM")
    result = copy.deepcopy(base)
    identities, provenance = [], {}
    for player in result["players"]:
        name = player["name"]
        original = next(p for p in actors if p["name"] == name)
        _, original_save = seed_bytes(Path(original["source_save"]), original["source_sha256"])
        save, receipt = validate_cache(roots[name], base, original, descriptor, rom, exe, config)
        expected_name_prefix = bytes((187 if name == "a" else 188, 255))
        identity = {fid: logical_field(save, descriptor, fid) for fid in (0x0200, 0x0205, 0x0102)}
        # New Game writes EOS padding; accept any retained trailing bytes only
        # when the entire name field still equals its pinned original source.
        if (len(identity[0x0200]) != 8 or identity[0x0200][:2] != expected_name_prefix
                or any(value != logical_field(original_save, descriptor, fid) for fid, value in identity.items())):
            raise OracleFailure("custody actor ROM name/trainer/money changed or parents were swapped")
        if logical_field(save, descriptor, 0x0100)[:8] != bytes((8, 0, 11, 0, 13, 10, 0, 0)):
            raise OracleFailure("custody actor must be at exact Main ferry8,11")
        player.update(source_save=str(save.path.resolve()), source_sha256=save.sha256,
                      seed_lineage=copy.deepcopy(receipt["seed_lineage"]),
                      population_recipe=copy.deepcopy(receipt["validated"]["population_recipe"]),
                      custody_recipe=copy.deepcopy(receipt["validated"]["custody_recipe"]))
        player["shared_witnesses"] = merge_witnesses(original.get("shared_witnesses", []), receipt["validated"]["shared_witnesses"])
        # Every retained noncustody witness must still match. Never silently
        # discard changed fields merely to make the new plan pass.
        validate_player_fixture(save, descriptor, player)
        identities.append((save.sha256, identity))
        provenance[name] = {"roots": {key: str(path) for key, path in roots[name].items()},
                            "daycare_receipt_sha256": harness.digest(roots[name]["daycare_root"] / "receipt.json"),
                            "daycare_cache_key": receipt["cache_key"]}
    if any(identities[0][1][fid] == identities[1][1][fid] for fid in (0x0200, 0x0205, 0x0102)) or identities[0][0] == identities[1][0]:
        raise OracleFailure("custody actors need distinct names/trainer IDs/money/save hashes")
    result["custody_plan"] = {"schema": 1, "adapter": "hoenn-main-menu-v1", "purpose": "seed-only-draft",
        "character_ids": "unverified placeholders preserved from base plan",
        "before_signed_journey": "Replace each character_id from the successful seed receipt, then prepare and verify those signed profiles.",
        "parents": provenance, "publisher_sha256": harness.digest(Path(__file__))}
    harness.preflight(result, profiles_ready=False)
    validate_lineages(result)
    return result


def publish_plan(plan_path: Path, parents: dict, config: Path, output: Path) -> Path:
    original_bytes = plan_path.read_bytes()
    base = json.loads(original_bytes.decode("utf-8-sig"))
    roots = _parents(parents)
    target = output.resolve()
    protected = [plan_path.resolve(), config.resolve()]
    protected += [Path(player["source_save"]).resolve() for player in base["players"]]
    protected += [Path(item["path"]).resolve() for player in base["players"] for item in player.get("seed_lineage", [])]
    if target in protected or any(target.is_relative_to(root) for actor in roots.values() for root in actor.values()):
        raise OracleFailure("custody output collides with protected source/base/parent artifacts")
    if not target.parent.is_dir():
        raise OracleFailure("custody output directory must already exist")
    result = derive_plan(base, parents, config)
    result["custody_plan"]["base_plan_sha256"] = hashlib.sha256(original_bytes).hexdigest()
    if plan_path.read_bytes() != original_bytes:
        raise OracleFailure("custody base plan changed during validation")
    encoded = (json.dumps(result, sort_keys=True, indent=2) + "\n").encode("utf-8")
    if target.exists():
        if target.read_bytes() != encoded:
            raise OracleFailure("custody immutable output already exists with different inputs/content")
        return target
    pending = target.with_suffix(target.suffix + ".tmp")
    owned = False
    try:
        with pending.open("xb") as file:
            owned = True; file.write(encoded); file.flush(); os.fsync(file.fileno())
        if os.name == "nt":
            pending.rename(target)
        else:
            os.link(pending, target); pending.unlink()
        owned = False
    finally:
        if owned: pending.unlink(missing_ok=True)
    return target


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", type=Path, required=True)
    parser.add_argument("--parents-json", type=Path, required=True)
    parser.add_argument("--mgba-config", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    print(publish_plan(args.plan, json.loads(args.parents_json.read_text(encoding="utf-8-sig")), args.mgba_config, args.output))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
