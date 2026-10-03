"""Read-only draft derivation with fake parsed saves; no API/UI/emulator."""
from contextlib import ExitStack
import copy
import json
from pathlib import Path
import struct
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools/coop"))
import live_custody_plan as publisher
from live_harness_oracles import OracleFailure


class CustodyPlanTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.config = self.root / "config.ini"; self.config.write_bytes(b"config")
        release = self.root / "release"; (release / "runtime").mkdir(parents=True)
        (release / "main.gba").write_bytes(b"rom"); (release / "runtime/mgba.exe").write_bytes(b"exe")
        (release / "release_catalog.json").write_text(json.dumps({"worlds": [{"world_id": 1, "rom_path": "main.gba", "rom_sha256": "unit"}]}))
        (release / "server-build-catalog.json").write_text(json.dumps({"shared_player_descriptor_hex": ""}))
        self.parents, self.saves, self.receipts, self.fields, self.originals = {}, {}, {}, {}, {}
        self.base = {"release_dir": str(release), "server_url": "http://127.0.0.1:8080", "legs": [{"world": "unchanged"}],
                     "region_catalog": {"future": "unchanged"}, "players": []}
        self.required = [{"field_id": fid, "sha256": str(fid), "offset": 0, "size": size, "min_nonzero_bytes": 1}
                         for fid, size in ((0x0101, 604), (0x010B, 576), (0x010D, 288))]
        for name, trainer, money, letter in (("a", b"aaaa", 3000, 187), ("b", b"bbbb", 999999, 188)):
            self.parents[name] = {}
            for key in publisher.ROOT_KEYS:
                folder = self.root / name / key; folder.mkdir(parents=True)
                (folder / "receipt.json").write_bytes(key.encode())
                self.parents[name][key] = str(folder)
            source = self.root / name / "source.sav"; source.write_bytes(b"original")
            original = SimpleNamespace(path=source, sha256=name * 64, name=name, generation=2)
            final_path = Path(self.parents[name]["daycare_root"]) / "daycare.sav"
            final_path.write_bytes(b"final")
            final = SimpleNamespace(path=final_path, sha256=("1" if name == "a" else "2") * 64, name=name, generation=6)
            self.saves[name], self.originals[name] = final, original
            fields = {0x0200: bytes((letter, 255)) + bytes(6), 0x0205: trainer, 0x0102: struct.pack("<I", money),
                      0x0100: bytes((8, 0, 11, 0, 13, 10, 0, 0))}
            self.fields[id(original)] = fields.copy(); self.fields[id(final)] = fields.copy()
            self.receipts[name] = {"cache_key": name + "key", "seed_lineage": [{"path": f"{name}-gen{i}", "sha256": str(i) * 64} for i in range(1, 7)],
                "validated": {"population_recipe": {"party_species": [1, 2, 4, 5, 6]}, "custody_recipe": {"unit": name},
                              "shared_witnesses": copy.deepcopy(self.required)}}
            self.base["players"].append({"name": name, "source_save": str(source), "source_sha256": original.sha256,
                "profile_localappdata": str(self.root / "profiles" / name), "character_id": name + "-placeholder",
                "population_recipe": {"unit": "old"}, "seed_lineage": [{"path": str(source), "sha256": original.sha256}],
                "shared_witnesses": [{"field_id": 0x0101, "sha256": "old-party"}, {"field_id": 0x0301, "sha256": "preserved-pc"}]})
        self.plan_path = self.root / "authoring-plan.json"; self.plan_path.write_text(json.dumps(self.base))
        self.output = self.root / "draft.json"

    def environment(self):
        stack = ExitStack()
        self.preflight = stack.enter_context(mock.patch.object(publisher.harness, "preflight"))
        stack.enter_context(mock.patch.object(publisher.harness, "check_c_space"))
        stack.enter_context(mock.patch.object(publisher.harness, "require_hash"))
        stack.enter_context(mock.patch.object(publisher, "config_check"))
        self.lineages = stack.enter_context(mock.patch.object(publisher, "validate_lineages"))
        self.fixtures = stack.enter_context(mock.patch.object(publisher, "validate_player_fixture"))
        self.cached = stack.enter_context(mock.patch.object(publisher, "validate_cache", side_effect=lambda roots, base, player, *args:
            (self.saves[player["name"]], self.receipts[player["name"]])))
        stack.enter_context(mock.patch.object(publisher, "seed_bytes", side_effect=lambda path, sha:
            (b"original", next(save for save in self.originals.values() if save.path == path))))
        stack.enter_context(mock.patch.object(publisher, "logical_field", side_effect=lambda save, d, fid: self.fields[id(save)][fid]))
        self.api = stack.enter_context(mock.patch("live_seed_players._call"))
        self.launch = stack.enter_context(mock.patch.object(publisher.daycare, "owned_scripted_emulator"))
        return stack

    def test_derive_preserves_leg_profiles_ids_and_non_custody_witnesses(self):
        original = copy.deepcopy(self.base)
        with self.environment():
            result = publisher.derive_plan(self.base, self.parents, self.config)
            self.assertEqual(self.base, original)
            self.assertEqual(result["legs"], original["legs"]); self.assertEqual(result["region_catalog"], original["region_catalog"])
            for before, after in zip(original["players"], result["players"]):
                self.assertEqual(after["character_id"], before["character_id"])
                self.assertEqual(after["profile_localappdata"], before["profile_localappdata"])
                self.assertEqual(after["shared_witnesses"], [before["shared_witnesses"][1]] + self.required)
                self.assertEqual(len(after["seed_lineage"]), 6)
            self.assertEqual(result["custody_plan"]["purpose"], "seed-only-draft")
            self.assertIn("seed receipt", result["custody_plan"]["before_signed_journey"])
            self.assertEqual(self.preflight.call_count, 2); self.lineages.assert_called_once_with(result)
            self.api.assert_not_called(); self.launch.assert_not_called()

    def test_preserved_stale_other_witness_is_not_dropped(self):
        with self.environment():
            self.fixtures.side_effect = OracleFailure("stale PC witness")
            with self.assertRaises(OracleFailure): publisher.publish_plan(self.plan_path, self.parents, self.config, self.output)
            self.assertFalse(self.output.exists()); self.assertFalse(self.output.with_suffix(".json.tmp").exists())
            self.api.assert_not_called(); self.launch.assert_not_called()

    def test_rom_eos_padding_is_accepted_only_with_exact_source_name_field(self):
        for name, letter in (("a", 187), ("b", 188)):
            value = bytes((letter,)) + bytes((255,)) * 7
            self.fields[id(self.originals[name])][0x0200] = value
            self.fields[id(self.saves[name])][0x0200] = value
        with self.environment():
            result = publisher.derive_plan(self.base, self.parents, self.config)
            self.assertEqual([p["name"] for p in result["players"]], ["a", "b"])
            # Preserve semantic A+EOS while changing only its padding: rejected.
            self.fields[id(self.saves["a"])][0x0200] = b"\xbb\xff" + bytes(6)
            with self.assertRaises(OracleFailure): publisher.derive_plan(self.base, self.parents, self.config)
            # Even matching source/current padding cannot authorize a wrong name.
            for obj in (self.originals["a"], self.saves["a"]):
                self.fields[id(obj)][0x0200] = b"\xbc" + b"\xff" * 7
            with self.assertRaises(OracleFailure): publisher.derive_plan(self.base, self.parents, self.config)

    def test_immutable_identical_reuse_and_no_overwrite(self):
        with self.environment():
            publisher.publish_plan(self.plan_path, self.parents, self.config, self.output)
            stat, data = self.output.stat(), self.output.read_bytes()
            publisher.publish_plan(self.plan_path, self.parents, self.config, self.output)
            self.assertEqual(self.output.stat().st_mtime_ns, stat.st_mtime_ns)
            self.assertEqual(self.output.read_bytes(), data)
            self.output.write_bytes(b"retained-different-plan")
            with self.assertRaises(OracleFailure): publisher.publish_plan(self.plan_path, self.parents, self.config, self.output)
            self.assertEqual(self.output.read_bytes(), b"retained-different-plan")

    def test_atomic_flush_failure_and_destination_race_preserve_existing(self):
        with self.environment():
            import os
            with mock.patch.object(publisher.os, "fsync", side_effect=OSError("disk failure")):
                with self.assertRaises(OSError): publisher.publish_plan(self.plan_path, self.parents, self.config, self.output)
            self.assertFalse(self.output.exists()); self.assertFalse(self.output.with_suffix(".json.tmp").exists())
            real_fsync = os.fsync
            def sync(fd):
                self.assertFalse(self.output.exists())
                pending = self.output.with_suffix(".json.tmp")
                self.assertEqual(json.loads(pending.read_text())["custody_plan"]["purpose"], "seed-only-draft")
                real_fsync(fd)
                self.output.write_bytes(b"other-writer")
            with mock.patch.object(publisher.os, "fsync", side_effect=sync):
                with self.assertRaises(FileExistsError): publisher.publish_plan(self.plan_path, self.parents, self.config, self.output)
            self.assertEqual(self.output.read_bytes(), b"other-writer")
            self.assertFalse(self.output.with_suffix(".json.tmp").exists())

    def test_base_changed_during_validation_rejects_before_output(self):
        with self.environment():
            self.lineages.side_effect = lambda plan: self.plan_path.write_bytes(b"base-changed")
            with self.assertRaises(OracleFailure): publisher.publish_plan(self.plan_path, self.parents, self.config, self.output)
            self.assertFalse(self.output.exists())

    def test_collision_base_sources_parents_and_config_before_derivation(self):
        with self.environment():
            for output in (self.plan_path, self.config, self.originals["a"].path,
                           Path(self.parents["a"]["daycare_root"]) / "new-plan.json"):
                with self.subTest(output=output), self.assertRaises(OracleFailure):
                    publisher.publish_plan(self.plan_path, self.parents, self.config, output)
            self.preflight.assert_not_called(); self.cached.assert_not_called()

    def test_swap_name_money_trainer_hash_and_profile_reject(self):
        with self.environment():
            for fid in (0x0200, 0x0205, 0x0102, 0x0100):
                old = self.fields[id(self.saves["a"])][fid]
                self.fields[id(self.saves["a"])][fid] = self.fields[id(self.saves["b"])][fid] if fid != 0x0100 else bytes(8)
                with self.assertRaises(OracleFailure): publisher.derive_plan(self.base, self.parents, self.config)
                self.fields[id(self.saves["a"])][fid] = old
            self.saves["b"].sha256 = self.saves["a"].sha256
            with self.assertRaises(OracleFailure): publisher.derive_plan(self.base, self.parents, self.config)
            self.saves["b"].sha256 = "2" * 64
            paths = [p["profile_localappdata"] for p in self.base["players"]]
            for player, value in zip(self.base["players"], reversed(paths)): player["profile_localappdata"] = value
            with self.assertRaises(OracleFailure): publisher.derive_plan(self.base, self.parents, self.config)
            self.api.assert_not_called(); self.launch.assert_not_called()

    def test_preflight_lineage_and_parent_rejection_no_output(self):
        for boundary in ("preflight", "lineages", "cached"):
            with self.subTest(boundary=boundary), self.environment():
                getattr(self, boundary).side_effect = OracleFailure(boundary)
                with self.assertRaises(OracleFailure): publisher.publish_plan(self.plan_path, self.parents, self.config, self.output)
                self.assertFalse(self.output.exists()); self.api.assert_not_called(); self.launch.assert_not_called()

    def test_merge_rejects_partial_duplicate_or_malformed_witnesses(self):
        for original, required in (([{"field_id": 1}, {"field_id": 1}], self.required),
                                   ([None], self.required), ([], self.required[:-1]),
                                   ([], self.required + [self.required[0]])):
            with self.assertRaises(OracleFailure): publisher.merge_witnesses(original, required)

    def test_parent_mapping_and_loopback_reject(self):
        with self.environment():
            for parents in ({}, {"a": self.parents["a"]}, dict(self.parents, b={}),
                            dict(self.parents, a=dict(self.parents["a"], daycare_root="relative"))):
                with self.assertRaises(OracleFailure): publisher.derive_plan(self.base, parents, self.config)
            self.base["server_url"] = "http://example.com"; self.base["allow_remote_server_checks"] = True
            with self.assertRaises(OracleFailure): publisher.derive_plan(self.base, self.parents, self.config)

    def test_reconstruct_exact_daycare_inputs_and_ancestry(self):
        validate_cache = publisher.validate_cache
        with self.environment(), ExitStack() as stack:
            roots = {key: Path(value) for key, value in self.parents["a"].items()}
            original = self.base["players"][0]
            recipe = {"unit": "old"}
            source = SimpleNamespace(sha256="c" * 64)
            ancestry = [{"path": f"gen{i}", "sha256": str(i) * 64} for i in range(1, 6)]
            parent = {"cache_key": "third-key", "inputs": {"signed_fixture": {"unit": True}}}
            release = Path(self.base["release_dir"]); paths = (release / "main.gba", release / "runtime/mgba.exe", self.config)
            expected = {"player": "a", "source_sha256": source.sha256, "recipe": recipe, "ancestry": ancestry,
                "parent_cache_key": parent["cache_key"], "parent_receipt_sha256": publisher.harness.digest(roots["third_mail_root"] / "receipt.json"),
                "rom_sha256": publisher.harness.digest(paths[0]), "emulator_sha256": publisher.harness.digest(paths[1]), "config_sha256": publisher.harness.digest(paths[2]),
                "descriptor_sha256": publisher.hashlib.sha256(b"").hexdigest(), "signed_fixture": parent["inputs"]["signed_fixture"],
                "max_post_deposit_steps": publisher.daycare.MAX_POST_DEPOSIT_STEPS, "max_total_steps": publisher.daycare.MAX_TOTAL_STEPS,
                "dependencies": {name: publisher.harness.digest(Path(publisher.__file__).with_name(name)) for name in publisher.daycare.PARENT_DEPS + ("live_author_daycare.py",)}}
            (roots["daycare_root"] / "inputs.json").write_text(json.dumps(expected))
            receipt = {"validated": {"save_sha256": self.saves["a"].sha256}, "seed_lineage": ancestry + [
                {"path": str(self.saves["a"].path.resolve()), "sha256": self.saves["a"].sha256}]}
            stack.enter_context(mock.patch.object(publisher.daycare, "validate_parent", return_value=(b"source", source, parent, ancestry)))
            stack.enter_context(mock.patch.object(publisher.daycare.held_mail, "recipe_check", return_value=recipe))
            cached = stack.enter_context(mock.patch.object(publisher.daycare, "cached_receipt", return_value=receipt))
            stack.enter_context(mock.patch.object(publisher, "seed_bytes", return_value=(b"final", self.saves["a"])))
            self.assertEqual(validate_cache(roots, self.base, original, b"", *paths)[0], self.saves["a"])
            for key in ("dependencies", "signed_fixture", "ancestry", "source_sha256", "parent_receipt_sha256", "max_post_deposit_steps", "player"):
                (roots["daycare_root"] / "inputs.json").write_text(json.dumps(dict(expected, **{key: "forged"})))
                cached.reset_mock()
                with self.assertRaises(OracleFailure): validate_cache(roots, self.base, original, b"", *paths)
                cached.assert_not_called()
            (roots["daycare_root"] / "inputs.json").write_text(json.dumps(expected))
            receipt["seed_lineage"] = list(reversed(receipt["seed_lineage"]))
            with self.assertRaises(OracleFailure): validate_cache(roots, self.base, original, b"", *paths)


if __name__ == "__main__":
    unittest.main()
