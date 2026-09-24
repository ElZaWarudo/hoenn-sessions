"""Contract tests for release-derived server identities across three ROMs."""

from __future__ import annotations

import hashlib
import json
import unittest
from pathlib import Path

from tools.coop.generate_server_build_catalog import generate
from tools.tests import test_rom_release_catalog as release_tests


class ServerBuildCatalogTests(unittest.TestCase):
    def setUp(self) -> None:
        self.fixture = release_tests.RomReleaseCatalogTests("test_three_world_cycle_and_return_edges")
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        for world in self.fixture.catalog["worlds"]:
            path = self.fixture.root / world["bridge_path"]
            manifest = json.loads(path.read_text(encoding="utf-8"))
            manifest["game_build"]["id"] = f"game-{world['name']}"
            manifest["net_bridge"] = {"abi_version": 1, "game_protocol_version": 1}
            manifest["emulator"] = {
                "name": "mGBA", "version": "0.11.0", "build_id": "test-build",
                "source_commit": "abc", "platform": "windows-x64", "variant": "Qt",
                "archive_sha256": "a" * 64, "executable_sha256": "b" * 64,
            }
            self._save_bridge(world, manifest)

    def _save_bridge(self, world: dict, manifest: dict) -> None:
        path = self.fixture.root / world["bridge_path"]
        path.write_text(json.dumps(manifest), encoding="utf-8")
        world["bridge_sha256"] = hashlib.sha256(path.read_bytes()).hexdigest()

    def _generate(self) -> tuple[bytes, str]:
        self.fixture.build_path.write_text(json.dumps(self.fixture.builds), encoding="utf-8")
        self.fixture.catalog_path.write_text(json.dumps(self.fixture.catalog), encoding="utf-8")
        trusted = hashlib.sha256(self.fixture.catalog_path.read_bytes()).hexdigest()
        return generate(self.fixture.build_path, self.fixture.catalog_path, trusted)

    def test_three_worlds_are_sorted_and_bytes_are_deterministic(self) -> None:
        data, digest = self._generate()
        self.assertEqual(hashlib.sha256(data).hexdigest(), digest)
        self.assertEqual((data, digest), self._generate())
        fixture = (Path(__file__).resolve().parents[2] / "coop" / "crates" / "coop-server"
                   / "src" / "phase2" / "saves" / "fixtures" / "travel-catalog-v3.json")
        self.assertEqual(data, fixture.read_bytes())
        catalog = json.loads(data)
        self.assertEqual(catalog["schema_version"], 3)
        transfer = self.fixture.root / self.fixture.catalog["worlds"][0]["player_transfer_path"]
        self.assertEqual(catalog["shared_player_descriptor_sha256"],
                         json.loads(transfer.read_text(encoding="utf-8"))["sha256"])
        self.assertEqual(hashlib.sha256(bytes.fromhex(catalog["shared_player_descriptor_hex"])).hexdigest(),
                         catalog["shared_player_descriptor_sha256"])
        self.assertEqual([world["world_id"] for world in catalog["worlds"]], [1, 2, 3])
        self.assertEqual([world["build"]["game_build_id"] for world in catalog["worlds"]],
                         ["game-main", "game-cormoria", "game-third"])
        self.assertEqual([world["build"]["mgba_version"] for world in catalog["worlds"]],
                         ["0.11.0"] * 3)
        self.assertEqual([world["build"]["bridge_abi"] for world in catalog["worlds"]], [1] * 3)
        self.assertTrue(all(world["arrivals"] for world in catalog["worlds"]))
        self.assertTrue(all(world["portals"] for world in catalog["worlds"]))

    def test_rejects_changed_arrival_template(self) -> None:
        template = self.fixture.root / self.fixture.catalog["worlds"][1]["arrivals"]["from_previous"]["template_sav_path"]
        template.write_bytes(b"changed")
        with self.assertRaisesRegex(ValueError, "digest mismatch"):
            self._generate()

    def test_release_order_does_not_change_server_bytes(self) -> None:
        expected = self._generate()
        self.fixture.catalog["worlds"].reverse()
        self.assertEqual(self._generate(), expected)

    def test_rejects_duplicate_build_ids(self) -> None:
        world = self.fixture.catalog["worlds"][2]
        path = self.fixture.root / world["bridge_path"]
        manifest = json.loads(path.read_text(encoding="utf-8"))
        manifest["game_build"]["id"] = "game-main"
        self._save_bridge(world, manifest)
        with self.assertRaisesRegex(ValueError, "duplicate server build identity"):
            self._generate()

    def test_rejects_missing_emulator_fields(self) -> None:
        world = self.fixture.catalog["worlds"][1]
        path = self.fixture.root / world["bridge_path"]
        manifest = json.loads(path.read_text(encoding="utf-8"))
        del manifest["emulator"]["version"]
        self._save_bridge(world, manifest)
        with self.assertRaisesRegex(ValueError, "incomplete bridge build identity"):
            self._generate()

    def test_rejects_wrong_rom_or_protocol(self) -> None:
        world = self.fixture.catalog["worlds"][2]
        path = self.fixture.root / world["bridge_path"]
        manifest = json.loads(path.read_text(encoding="utf-8"))
        manifest["net_bridge"]["game_protocol_version"] = 2
        self._save_bridge(world, manifest)
        with self.assertRaisesRegex(ValueError, "invalid bridge build identity"):
            self._generate()

    def test_rejects_untrusted_release_catalog(self) -> None:
        self.fixture.build_path.write_text(json.dumps(self.fixture.builds), encoding="utf-8")
        self.fixture.catalog_path.write_text(json.dumps(self.fixture.catalog), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "trusted digest"):
            generate(self.fixture.build_path, self.fixture.catalog_path, "0" * 64)


if __name__ == "__main__":
    unittest.main()
