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
V8B_REGION_SHA256 = "63105285f26803e69954bfbc29e1e9fefab59972f78efc63f2e6f8942af0b241"
V8B_SERVER_SHA256 = "8d8b333362eb995dc6183f7d390716f20d92b10b1d58e3fa5c4b8ee85d266dd7"
V8B_OBJECT_CATALOG_SHA256 = "d6cca6d7297e17519be8a7357cb6f8a26a8b175fc7da56ded5fb81c365f116d6"
# The retired TEST-ONLY placeholder. No release output may ever carry it again.
RETIRED_PROVISIONAL_SHA256 = hashlib.sha256(
    b"TEST-ONLY PROVISIONAL object catalog digest; no semantic attestation\n").hexdigest()
EXPERIENCE_PAYLOAD = bytes(range(256)) * 4


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def contract_manifests(rom_sha256: str, shift: int) -> dict[str, dict]:
    """Shape-faithful contract manifests; `shift` moves every per-world address."""
    base = 0x08000000 + shift
    tables = {}
    for index, name in enumerate(asm._OBJECT_TABLES):
        table = {"address": base + 0x1000 * (index + 1), "size": 96, "record_count": 2,
                 "record_stride": 48, "pointer_offsets": [8, 12] if index != 4 else [],
                 "text_offsets": [12] if index != 4 else [],
                 "raw_sha256": sha(f"raw-{name}-{shift}".encode()),
                 "scalar_sha256": sha(f"scalar-{name}".encode()),
                 "pointer_presence_sha256": sha(f"presence-{name}".encode()),
                 "display_text_sha256": sha(f"text-{name}".encode())}
        if name == "gMovesInfo":
            table.update(additional_effect_stride=12, additional_effect_count_offset=9,
                         additional_effect_count_mask=56,
                         additional_effect_sha256=sha(b"effects"))
        tables[name] = table
    return {
        "experience_table": {"schema_version": 1, "rom_sha256": rom_sha256,
                             "symbol": "gExperienceTables", "address": base,
                             "size": len(EXPERIENCE_PAYLOAD), "sha256": sha(EXPERIENCE_PAYLOAD)},
        "object_scalar": {"schema_version": 4, "scope": "linked-table-scalars",
                          "rom_sha256": rom_sha256,
                          "descriptor": {"address": base + 0x100, "size": 552,
                                         "sha256": sha(b"descriptor")},
                          "count_probe": {"address": base + 0x400, "size": 68},
                          "tables": tables},
        "player_transfer": {"schema_version": 3, "rom_sha256": rom_sha256,
                            "address": base + 0x800, "symbol": "gCoopPlayerTransferSchema",
                            "size": 764, "descriptor_size": 764, "field_count": 1,
                            "sha256": sha(b"transfer"),
                            "fields": [{"id": 256, "offset": 0, "ownership": 2, "size": 564,
                                        "storage": 0}],
                            "spans": {"save_block1": 16928}, "rekey_field_ids": [258],
                            "daycare_custody_field_ids": [269]},
    }


class Fixture:
    """Fake per-world build outputs plus attestations that match them."""

    def __init__(self, root: Path) -> None:
        self.root = root
        self.dist = root / "dist"
        self.repo = root / "repo"
        self.roms = {}
        attested = json.loads(ARRIVALS.read_text(encoding="utf-8"))
        for shift, name in ((0x40, "main"), (0x200, "cormoria")):
            prefix = f"rom-{name}".encode() * 64
            rom = prefix[:shift] + EXPERIENCE_PAYLOAD + prefix
            self.roms[name] = sha(rom)
            world = self.dist / name
            world.mkdir(parents=True)
            (world / "game.gba").write_bytes(rom)
            (world / "bridge_manifest.json").write_text(json.dumps(
                {"game_build": {"rom_sha256": sha(rom)}, "save": {"schema_version": 2}}))
            for contract, filename, *_ in asm.FINGERPRINT_CONTRACTS:
                self.write_manifest(name, filename, contract_manifests(sha(rom), shift)[contract])
            (world / "map_binding_manifest.json").write_text(json.dumps({"rom_sha256": sha(rom)}))
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

    def write_manifest(self, world: str, filename: str, manifest: dict) -> None:
        (self.dist / world / filename).write_text(json.dumps(manifest, indent=2))

    def read_manifest(self, world: str, filename: str) -> dict:
        return json.loads((self.dist / world / filename).read_text())

    def assemble(self, out: str = "out", **overrides):
        options = dict(registry=REGISTRY, release_config=CONFIG, release_arrivals=self.arrivals,
                       smoke_arrivals=SMOKE, repo_root=self.repo)
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
        self.assertEqual(main["object_catalog_sha256"], summary["object_catalog_sha256"])
        self.assertEqual(cormoria["object_catalog_sha256"], summary["object_catalog_sha256"])
        self.assertEqual(region, asm.canonical_json(catalog))
        server = self.fixture.root / "out/server-catalog" / summary["server_build_catalog_sha256"]
        files = sorted(p.relative_to(server).as_posix() for p in server.rglob("*") if p.is_file())
        self.assertEqual(files, ["server-build-catalog.json", "worlds/1/arrival.sav",
                                 "worlds/2/arrival.sav"])
        document = (self.fixture.root / "out/object-catalog-fingerprint.json").read_bytes()
        self.assertEqual(sha(document), summary["object_catalog_sha256"])

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
                         "--smoke-arrivals", str(SMOKE), "--repo-root", str(self.fixture.repo)])
        self.assertEqual(code, 3)

    def test_cli_rejects_retired_provisional_flag(self, validate, generate) -> None:
        with self.assertRaises(SystemExit) as raised, mock.patch("sys.stderr"):
            asm.main(["assemble", "--out", str(self.fixture.root / "cli"),
                      "--test-only-provisional-object-catalog"])
        self.assertEqual(raised.exception.code, 2)

    def test_refuses_altered_attested_save(self, validate, generate) -> None:
        (self.fixture.repo / "data/release_arrivals/main.sav").write_bytes(b"tampered")
        with self.assertRaisesRegex(asm.AssemblyError, "missing or altered"):
            self.fixture.assemble()

    def test_production_mode_derives_object_catalog_without_configured_constant(
            self, validate, generate) -> None:
        config = json.loads(CONFIG.read_text(encoding="utf-8"))
        self.assertNotIn("sha256", config["object_catalog"])
        summary = self.fixture.assemble()
        self.assertRegex(summary["object_catalog_sha256"], r"^[0-9a-f]{64}$")
        self.assertNotEqual(summary["object_catalog_sha256"], RETIRED_PROVISIONAL_SHA256)
        self.assertEqual(summary["object_catalog_source"], "shared-object-contract-fingerprint")
        self.assertEqual(summary["object_catalog_version"], 1)
        self.assertNotIn("provisional_object_catalog", summary)
        projection = asm.contract_projection(contract_manifests("0" * 64, 0))
        self.assertEqual(summary["object_catalog_sha256"],
                         sha(asm.fingerprint_document(projection)))

    def test_refuses_configured_or_versioned_object_catalog(self, validate, generate) -> None:
        for entry in ({"sha256": "ab" * 32},
                      {"source": "shared-object-contract-fingerprint", "version": 2},
                      {"source": "shared-object-contract-fingerprint", "version": 1,
                       "sha256": "ab" * 32},
                      {"sha256": None, "status": "undefined"}):
            config = json.loads(CONFIG.read_text(encoding="utf-8"))
            config["object_catalog"] = entry
            path = self.fixture.root / "config.json"
            path.write_text(json.dumps(config))
            with self.assertRaisesRegex(asm.AssemblyError, "configured digests are not accepted"):
                self.fixture.assemble(release_config=path)
        self.assertFalse((self.fixture.root / "out").exists())

    def test_refuses_cross_world_object_contract_mismatch(self, validate, generate) -> None:
        name = "object_scalar_manifest.json"
        manifest = self.fixture.read_manifest("cormoria", name)
        manifest["tables"]["gItemsInfo"]["display_text_sha256"] = "cd" * 32
        self.fixture.write_manifest("cormoria", name, manifest)
        with self.assertRaisesRegex(asm.AssemblyError,
                                    "'cormoria' shared object contract differs .*object_scalar"):
            self.fixture.assemble()
        validate.assert_not_called()
        self.assertFalse((self.fixture.root / "out").exists())

    def test_refuses_cross_world_experience_mismatch(self, validate, generate) -> None:
        # Same attested ROM, but its manifest binds a different (genuine) table.
        rom = (self.fixture.dist / "cormoria/game.gba").read_bytes()
        name = "experience_table_manifest.json"
        manifest = self.fixture.read_manifest("cormoria", name)
        manifest["address"] = 0x08000000
        manifest["sha256"] = sha(rom[:manifest["size"]])
        self.fixture.write_manifest("cormoria", name, manifest)
        with self.assertRaisesRegex(asm.AssemblyError, "differs .*experience_table"):
            self.fixture.assemble()

    def test_refuses_manifest_for_another_rom(self, validate, generate) -> None:
        name = "player_transfer_manifest.json"
        manifest = self.fixture.read_manifest("main", name)
        manifest["rom_sha256"] = self.fixture.roms["cormoria"]
        self.fixture.write_manifest("main", name, manifest)
        with self.assertRaisesRegex(asm.AssemblyError, "does not describe the built ROM"):
            self.fixture.assemble()

    def test_refuses_experience_digest_not_in_rom(self, validate, generate) -> None:
        name = "experience_table_manifest.json"
        manifest = self.fixture.read_manifest("main", name)
        manifest["address"] += 4
        self.fixture.write_manifest("main", name, manifest)
        with self.assertRaisesRegex(asm.AssemblyError, "does not match the built ROM bytes"):
            self.fixture.assemble()

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


class FingerprintTests(unittest.TestCase):
    def digest(self, manifests: dict) -> str:
        return sha(asm.fingerprint_document(asm.contract_projection(manifests)))

    def test_document_carries_schema_and_version_tag(self) -> None:
        document = json.loads(asm.fingerprint_document(
            asm.contract_projection(contract_manifests("0" * 64, 0))))
        self.assertEqual(document["schema"], "hoenn-sessions/shared-object-contract-fingerprint")
        self.assertEqual(document["version"], 1)
        self.assertEqual(sorted(document["contracts"]),
                         ["experience_table", "object_scalar", "player_transfer"])

    def test_canonical_bytes_are_sorted_compact_ascii(self) -> None:
        document = asm.fingerprint_document(asm.contract_projection(contract_manifests("0" * 64, 0)))
        self.assertTrue(document.endswith(b"}\n"))
        self.assertNotIn(b": ", document)
        self.assertEqual(document, asm.canonical_json(json.loads(document)))

    def test_field_order_does_not_change_digest(self) -> None:
        def reverse(value):
            if isinstance(value, dict):
                return {key: reverse(value[key]) for key in reversed(list(value))}
            return value
        manifests = contract_manifests("0" * 64, 0)
        self.assertEqual(self.digest(manifests), self.digest(reverse(manifests)))

    def test_per_world_fields_are_excluded(self) -> None:
        main = contract_manifests("1" * 64, 0x40)
        other = contract_manifests("2" * 64, 0x9000)
        self.assertNotEqual(main, other)
        self.assertEqual(self.digest(main), self.digest(other))
        document = asm.fingerprint_document(asm.contract_projection(main))
        for excluded in (b"rom_sha256", b"address", b"raw_sha256", b"1" * 64):
            self.assertNotIn(excluded, document)

    def test_every_included_field_changes_digest(self) -> None:
        reference = self.digest(contract_manifests("0" * 64, 0))
        paths = [("experience_table", "sha256"), ("experience_table", "size"),
                 ("player_transfer", "spans"), ("player_transfer", "rekey_field_ids"),
                 ("object_scalar", "scope"), ("object_scalar", "descriptor", "sha256"),
                 ("object_scalar", "count_probe", "size")]
        for name in asm._OBJECT_TABLES:
            for key in asm._OBJECT_TABLE_KEYS[0]:
                paths.append(("object_scalar", "tables", name, key))
        for key in asm._OBJECT_MOVE_KEYS:
            paths.append(("object_scalar", "tables", "gMovesInfo", key))
        for path in paths:
            manifests = contract_manifests("0" * 64, 0)
            target = manifests
            for key in path[:-1]:
                target = target[key]
            value = target[path[-1]]
            target[path[-1]] = ("ee" * 32 if isinstance(value, str) and len(value) == 64
                                else "changed" if isinstance(value, str)
                                else [*value, 99] if isinstance(value, list)
                                else {**value, "extra": 1} if isinstance(value, dict)
                                else value + 1)
            self.assertNotEqual(self.digest(manifests), reference, path)

    def test_unknown_field_requires_version_bump(self) -> None:
        for path in (("experience_table",), ("player_transfer",), ("object_scalar",),
                     ("object_scalar", "descriptor"), ("object_scalar", "tables", "gSpeciesInfo")):
            manifests = contract_manifests("0" * 64, 0)
            target = manifests[path[0]]
            for key in path[1:]:
                target = target[key]
            target["graphics_sha256"] = "ab" * 32
            with self.assertRaisesRegex(asm.AssemblyError, "requires a fingerprint version bump"):
                asm.contract_projection(manifests)

    def test_missing_field_or_other_generator_schema_is_refused(self) -> None:
        manifests = contract_manifests("0" * 64, 0)
        del manifests["object_scalar"]["tables"]["gMovesInfo"]["additional_effect_sha256"]
        with self.assertRaisesRegex(asm.AssemblyError, "missing"):
            asm.contract_projection(manifests)
        for contract, version in (("object_scalar", 5), ("experience_table", 2),
                                  ("player_transfer", 2)):
            manifests = contract_manifests("0" * 64, 0)
            manifests[contract]["schema_version"] = version
            with self.assertRaisesRegex(asm.AssemblyError, "schema is not the one"):
                asm.contract_projection(manifests)
        manifests = contract_manifests("0" * 64, 0)
        del manifests["player_transfer"]
        with self.assertRaisesRegex(asm.AssemblyError, "exactly the experience"):
            asm.contract_projection(manifests)

    def test_malformed_digest_is_refused(self) -> None:
        manifests = contract_manifests("0" * 64, 0)
        manifests["object_scalar"]["tables"]["gItemsInfo"]["scalar_sha256"] = "AB" * 32
        with self.assertRaisesRegex(asm.AssemblyError, "lowercase SHA-256"):
            asm.contract_projection(manifests)


class TrackedDataTests(unittest.TestCase):
    def test_attested_saves_match_tracked_files(self) -> None:
        data = json.loads(ARRIVALS.read_text(encoding="utf-8"))
        for world in data["worlds"].values():
            for proof in world["arrivals"].values():
                path = REPO / proof["sav_path"]
                self.assertIn(path.stat().st_size, (131072, 131088))
                self.assertEqual(sha(path.read_bytes()), proof["sav_sha256"])

    def test_tracked_object_catalog_names_the_derivation(self) -> None:
        config = json.loads(CONFIG.read_text(encoding="utf-8"))
        self.assertEqual(config["object_catalog"],
                         {"source": asm.FINGERPRINT_SOURCE, "version": asm.FINGERPRINT_VERSION})
        self.assertNotIn(RETIRED_PROVISIONAL_SHA256, CONFIG.read_text(encoding="utf-8"))
        asm.object_catalog_config(config)

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
                                   repo_root=REPO,
                                   arrival_verifier=Path(verifier) if verifier else None)
        self.assertEqual(summary["object_catalog_sha256"], V8B_OBJECT_CATALOG_SHA256)
        self.assertNotEqual(summary["object_catalog_sha256"], RETIRED_PROVISIONAL_SHA256)
        self.assertEqual(summary["release_catalog_sha256"], V8B_REGION_SHA256)
        self.assertEqual(summary["server_build_catalog_sha256"], V8B_SERVER_SHA256)


if __name__ == "__main__":
    unittest.main()
