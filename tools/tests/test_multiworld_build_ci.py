"""Registry-driven CI build planning and N-world verification checks."""

import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from tools.coop import multiworld_build_ci as ci


class MultiworldBuildCiTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.registry = self.root / "rom_worlds.json"
        self.arrivals = self.root / "rom_world_arrivals.json"
        self.entries = [
            {"name": "main", "world_id": 1, "build_bit": 1, "game_version": None,
             "map_version": None, "build_name": None, "title": None, "game_code": None},
            {"name": "cormoria", "world_id": 2, "build_bit": 2, "game_version": "EMERALD",
             "map_version": "emerald", "build_name": "emerald-cormoria",
             "title": "CORMORIA", "game_code": "BPCO"},
            {"name": "third", "world_id": 7, "build_bit": 4, "game_version": "FIRERED",
             "map_version": "firered", "build_name": "firered-third",
             "title": "THIRD", "game_code": "BPTH"},
        ]
        self.write_registry()
        self.write_arrivals()

    def write_arrivals(self, include_third=True):
        worlds = {
            "main": [{"map_group": 13, "map_number": 10, "map_layout_id": 88}],
            "cormoria": [{"map_group": 82, "map_number": 21, "map_layout_id": 1314}],
        }
        if include_third:
            worlds["third"] = [{"map_group": 4, "map_number": 5, "map_layout_id": 6}]
        self.arrivals.write_text(json.dumps({"schema_version": 1, "worlds": worlds}), encoding="utf-8")

    def write_registry(self):
        self.registry.write_text(json.dumps({"schema_version": 1, "default_world": "main",
                                             "identity_lock": "rom_world_ids.lock.json",
                                             "worlds": self.entries}), encoding="utf-8")
        (self.root / "rom_world_ids.lock.json").write_text(
            json.dumps({"schema_version": 1, "worlds": [
                {"name": entry["name"], "world_id": entry["world_id"]}
                for entry in self.entries]}), encoding="utf-8")

    def test_third_world_uses_own_version_build_name_and_code(self):
        self.assertEqual(ci.world_plan(self.registry), [
            ("main", "EMERALD", "emerald", "BPEE"),
            ("cormoria", "EMERALD", "emerald-cormoria", "BPCO"),
            ("third", "FIRERED", "firered-third", "BPTH"),
        ])

    def test_requires_two_to_sixteen_worlds(self):
        self.entries[:] = self.entries[:1]
        self.write_registry()
        with self.assertRaisesRegex(ValueError, "2 through 16"):
            ci.world_plan(self.registry)

    def test_rejects_non_emerald_base_version(self):
        with self.assertRaisesRegex(ValueError, "GAME_VERSION=EMERALD"):
            ci.world_plan(self.registry, "FIRERED")

    def test_rejects_registered_world_without_smoke_arrival(self):
        self.write_arrivals(include_third=False)
        with self.assertRaisesRegex(ValueError, "exactly the registered worlds"):
            ci.load_arrivals(self.arrivals, {"main", "cormoria", "third"})

    def test_builds_each_registered_world_with_its_version_and_output_name(self):
        plan = ci.world_plan(self.registry)
        with patch.object(ci.subprocess, "run") as run, patch.object(ci.shutil, "copyfile") as copy:
            ci.build_worlds(plan, self.root / "dist")
        make_commands = [call.args[0] for call in run.call_args_list if call.args[0][0] == "make"]
        self.assertEqual(len(make_commands), 3)
        self.assertIn("ROM_WORLD=third", make_commands[2])
        self.assertIn("GAME_VERSION=FIRERED", make_commands[2])
        self.assertEqual(str(copy.call_args_list[-2].args[0]), "pokefirered-third.gba")
        self.assertEqual(str(copy.call_args_list[-1].args[0]), "pokefirered-third.elf")
        self.assertEqual(len(run.call_args_list), 3 * (1 + len(ci.MANIFESTS)))

    def test_three_world_verification_checks_all_manifests_and_pilot_arrivals(self):
        plan = ci.world_plan(self.registry)
        dist = self.root / "dist"
        for index, (name, _, _, code) in enumerate(plan):
            root = dist / name
            root.mkdir(parents=True)
            rom = bytearray(0xC0)
            rom[0xAC:0xB0] = code.encode("ascii")
            rom[0] = index
            (root / "game.gba").write_bytes(rom)
            digest = hashlib.sha256(rom).hexdigest()
            for kind, (_, output) in ci.MANIFESTS.items():
                manifest = {"game_build": {"rom_sha256": digest}} if kind == "bridge" else {"rom_sha256": digest}
                (root / output).write_text(json.dumps(manifest), encoding="utf-8")
        with (patch.object(ci, "verify_arrival") as arrival,
              patch.object(ci, "require_same_transfer_schema") as transfer,
              patch.object(ci, "require_same_experience_tables") as experience,
              patch.object(ci, "require_same_scalar_tables") as scalar):
            ci.verify_worlds(plan, dist, self.arrivals)
            self.assertEqual(arrival.call_count, 3)
            self.assertEqual(arrival.call_args_list[-1].args[2:], (4, 5, 6))
            for compare in (transfer, experience, scalar):
                self.assertEqual(set(compare.call_args.args[0]), {"main", "cormoria", "third"})
        third = dist / "third" / "object_scalar_manifest.json"
        third.write_text(json.dumps({"rom_sha256": "wrong"}), encoding="utf-8")
        with patch.object(ci, "verify_arrival"):
            with self.assertRaisesRegex(ValueError, "third scalar/ROM mismatch"):
                ci.verify_worlds(plan, dist, self.arrivals)


if __name__ == "__main__":
    unittest.main()
