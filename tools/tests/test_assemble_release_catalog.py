"""Tests for the multi-world release catalog assembler."""

from __future__ import annotations

import copy
import hashlib
import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.coop import assemble_release_catalog as asm

REPO = Path(__file__).resolve().parents[2]
REGISTRY = REPO / "data/rom_worlds.json"
CONFIG = REPO / "data/rom_world_release.json"
ARRIVALS = REPO / "data/release_arrivals.json"
SMOKE = REPO / "data/rom_world_arrivals.json"
V8B_REGION_SHA256 = "bc112b0915796dd6df388d108dfda1b9779feb9016a32aad76ea6369a4417b6d"
V8B_SERVER_SHA256 = "8d8b333362eb995dc6183f7d390716f20d92b10b1d58e3fa5c4b8ee85d266dd7"


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


class Fixture:
    """Fake per-world build outputs plus attestations that match them."""

    def __init__(self, root: Path) -> None:
        self.root = root
        self.dist = root / "dist"
        self.repo = root / "repo"
        self.roms = {}
        attested = json.loads(ARRIVALS.read_text(encoding="utf-8"))
        for name in ("main", "cormoria"):
            rom = f"rom-{name}".encode() * 64
            self.roms[name] = sha(rom)
            world = self.dist / name
            world.mkdir(parents=True)
            (world / "game.gba").write_bytes(rom)
            (world / "bridge_manifest.json").write_text(json.dumps(
                {"game_build": {"rom_sha256": sha(rom)}, "save": {"schema_version": 2}}))
            (world / "player_transfer_manifest.json").write_text(json.dumps({"rom_sha256": sha(rom)}))
            for other in ("experience_table_manifest.json", "map_binding_manifest.json",
                          "object_scalar_manifest.json"):
                (world / other).write_text(json.dumps({"rom_sha256": sha(rom), "kind": other}))
            (world / "generated_addresses.lua").write_text("return {}\n")
            save = f"save-{name}".encode() * 4096
            save_path = self.repo / f"data/release_arrivals/{name}.sav"
            save_path.parent.mkdir(parents=True, exist_ok=True)
            save_path.write_bytes(save)
            for proof in attested["worlds"][name]["arrivals"].values():
                proof["rom_sha256"] = sha(rom)
                proof["sav_sha256"] = sha(save)
        self.arrivals = root / "release_arrivals.json"
        self.arrivals.write_text(json.dumps(attested))

    def assemble(self, out: str = "out", **overrides):
        options = dict(registry=REGISTRY, release_config=CONFIG, release_arrivals=self.arrivals,
                       smoke_arrivals=SMOKE, repo_root=self.repo,
                       provisional_object_catalog=True)
        options.update(overrides)
        return asm.assemble(self.dist, self.root / out, **options)


def fake_generate(registry, catalog, digest):
    data = asm.canonical_json({"schema_version": 3, "region": digest})
    return data, sha(data)


@mock.patch.object(asm.generate_server_build_catalog, "generate", side_effect=fake_generate)
@mock.patch.object(asm.rom_release_catalog, "validate_catalog", return_value={1: {}, 2: {}})
class AssembleTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.fixture = Fixture(Path(self.tmp.name))

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def test_writes_region_and_server_catalogs(self, validate, generate) -> None:
        summary = self.fixture.assemble()
        stage = self.fixture.root / "out/catalog"
        region = (stage / "release_catalog.json").read_bytes()
        self.assertEqual(summary["release_catalog_sha256"], sha(region))
        validate.assert_called_once()
        self.assertEqual(validate.call_args.args[2], sha(region))
        catalog = json.loads(region)
        self.assertEqual([w["world_id"] for w in catalog["worlds"]], [1, 2])
        main, cormoria = catalog["worlds"]
        self.assertEqual(main["portals"], [{"id": "to_cormoria", "destination_world_id": 2,
                                            "arrival_portal_id": "from_main",
                                            "return_portal_id": "to_main"}])
        self.assertEqual(main["arrivals"]["from_cormoria"]["template_sav_path"], "worlds/1/arrival.sav")
        self.assertEqual(cormoria["save_namespace"], "save_cormoria")
        self.assertEqual(main["object_catalog_sha256"], asm.PROVISIONAL_OBJECT_CATALOG_SHA256)
        self.assertEqual(region, asm.canonical_json(catalog))
        server = self.fixture.root / "out/server-catalog" / summary["server_build_catalog_sha256"]
        files = sorted(p.relative_to(server).as_posix() for p in server.rglob("*") if p.is_file())
        self.assertEqual(files, ["server-build-catalog.json", "worlds/1/arrival.sav",
                                 "worlds/2/arrival.sav"])
        self.assertTrue(summary["provisional_object_catalog"])

    def test_refuses_rom_hash_mismatch_with_recertify_message(self, validate, generate) -> None:
        (self.fixture.dist / "cormoria/game.gba").write_bytes(b"rebuilt-differently")
        with self.assertRaises(asm.RecertifyError) as raised:
            self.fixture.assemble()
        self.assertIn("recertify arrival saves", str(raised.exception))
        self.assertIn("cormoria", str(raised.exception))
        validate.assert_not_called()
        self.assertFalse((self.fixture.root / "out/catalog/release_catalog.json").exists())

    def test_cli_exit_code_for_recertification(self, validate, generate) -> None:
        (self.fixture.dist / "main/game.gba").write_bytes(b"other")
        code = asm.main(["assemble", "--dist", str(self.fixture.dist),
                         "--out", str(self.fixture.root / "cli"),
                         "--release-arrivals", str(self.fixture.arrivals),
                         "--registry", str(REGISTRY), "--release-config", str(CONFIG),
                         "--smoke-arrivals", str(SMOKE), "--repo-root", str(self.fixture.repo),
                         "--test-only-provisional-object-catalog"])
        self.assertEqual(code, 3)

    def test_refuses_altered_attested_save(self, validate, generate) -> None:
        (self.fixture.repo / "data/release_arrivals/main.sav").write_bytes(b"tampered")
        with self.assertRaisesRegex(asm.AssemblyError, "missing or altered"):
            self.fixture.assemble()

    def test_production_mode_refuses_undefined_object_catalog(self, validate, generate) -> None:
        with self.assertRaisesRegex(asm.AssemblyError, "object catalog digest is undefined"):
            self.fixture.assemble(provisional_object_catalog=False)
        self.assertFalse((self.fixture.root / "out").exists())

    def test_configured_object_digest_is_used_and_excludes_provisional(self, validate, generate) -> None:
        config = json.loads(CONFIG.read_text(encoding="utf-8"))
        config["object_catalog"]["sha256"] = "ab" * 32
        path = self.fixture.root / "config.json"
        path.write_text(json.dumps(config))
        with self.assertRaisesRegex(asm.AssemblyError, "refusing the test-only"):
            self.fixture.assemble(release_config=path)
        summary = self.fixture.assemble(release_config=path, provisional_object_catalog=False)
        self.assertEqual(summary["object_catalog_sha256"], "ab" * 32)

    def test_refuses_arrival_outside_build_smoke_checks(self, validate, generate) -> None:
        config = json.loads(CONFIG.read_text(encoding="utf-8"))
        config["worlds"]["main"]["arrivals"]["from_cormoria"]["map_number"] = 11
        path = self.fixture.root / "config.json"
        path.write_text(json.dumps(config))
        with self.assertRaisesRegex(asm.AssemblyError, "not covered"):
            self.fixture.assemble(release_config=path)

    def test_refuses_missing_world_attestation(self, validate, generate) -> None:
        data = json.loads(self.fixture.arrivals.read_text())
        del data["worlds"]["cormoria"]
        self.fixture.arrivals.write_text(json.dumps(data))
        with self.assertRaisesRegex(asm.AssemblyError, "exactly the registered worlds"):
            self.fixture.assemble()

    def test_refuses_existing_output(self, validate, generate) -> None:
        (self.fixture.root / "out").mkdir()
        with self.assertRaisesRegex(asm.AssemblyError, "existing output"):
            self.fixture.assemble()

    def test_signing_artifacts_order_and_copies(self, validate, generate) -> None:
        self.fixture.assemble()
        stage = self.fixture.root / "out/catalog"
        bundle = self.fixture.root / "game-bundle"
        bundle.mkdir()
        (bundle / "game.gba").write_bytes((stage / "worlds/1/game.gba").read_bytes())
        (bundle / "bridge_manifest.json").write_bytes((stage / "worlds/1/bridge_manifest.json").read_bytes())
        lines = asm.signing_artifacts(stage, bundle, "game")
        self.assertEqual([line.split("=", 1)[0] for line in lines], [
            "region-catalog", "world-1-rom", "world-1-compatibility", "world-1-player-transfer",
            "world-2-rom", "world-2-compatibility", "world-2-player-transfer"])
        for line in lines:
            self.assertTrue(Path(line.split("=", 1)[1]).is_file())
        (bundle / "game.gba").write_bytes(b"not world 1")
        with self.assertRaisesRegex(asm.AssemblyError, "world 1"):
            asm.signing_artifacts(stage, bundle, "game")

    def test_signing_artifacts_rejects_world_file_drift(self, validate, generate) -> None:
        self.fixture.assemble()
        stage = self.fixture.root / "out/catalog"
        (stage / "worlds/2/player_transfer.json").write_text("{}")
        with self.assertRaisesRegex(asm.AssemblyError, "does not match the region catalog"):
            asm.signing_artifacts(stage, self.fixture.root / "b", "windows")


class TrackedDataTests(unittest.TestCase):
    def test_attested_saves_match_tracked_files(self) -> None:
        data = json.loads(ARRIVALS.read_text(encoding="utf-8"))
        for world in data["worlds"].values():
            for proof in world["arrivals"].values():
                path = REPO / proof["sav_path"]
                self.assertIn(path.stat().st_size, (131072, 131088))
                self.assertEqual(sha(path.read_bytes()), proof["sav_sha256"])

    def test_tracked_object_catalog_is_not_a_placeholder(self) -> None:
        config = json.loads(CONFIG.read_text(encoding="utf-8"))
        self.assertNotEqual(config["object_catalog"]["sha256"], asm.PROVISIONAL_OBJECT_CATALOG_SHA256)

    def test_world_plan_matches_registry(self) -> None:
        plan = asm.plan_worlds(REGISTRY, json.loads(CONFIG.read_text(encoding="utf-8")),
                               json.loads(ARRIVALS.read_text(encoding="utf-8")), SMOKE)
        self.assertEqual([(w["name"], w["world_id"]) for w in plan], [("main", 1), ("cormoria", 2)])


@unittest.skipUnless(os.environ.get("COOP_RELEASE_V8B_DIST"),
                     "set COOP_RELEASE_V8B_DIST (and optionally COOP_ARRIVAL_VERIFIER) for the "
                     "byte-equivalence check against the v8b family")
class V8bEquivalenceTests(unittest.TestCase):
    def test_reproduces_v8b_catalogs(self) -> None:
        verifier = os.environ.get("COOP_ARRIVAL_VERIFIER")
        with tempfile.TemporaryDirectory() as tmp:
            summary = asm.assemble(Path(os.environ["COOP_RELEASE_V8B_DIST"]), Path(tmp) / "out",
                                   registry=REGISTRY, release_config=CONFIG,
                                   release_arrivals=ARRIVALS, smoke_arrivals=SMOKE,
                                   repo_root=REPO, provisional_object_catalog=True,
                                   arrival_verifier=Path(verifier) if verifier else None)
        self.assertEqual(summary["release_catalog_sha256"], V8B_REGION_SHA256)
        self.assertEqual(summary["server_build_catalog_sha256"], V8B_SERVER_SHA256)


if __name__ == "__main__":
    unittest.main()
