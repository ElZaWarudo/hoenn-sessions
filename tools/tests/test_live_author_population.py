"""Synthetic bytes/fake owned drivers; no emulator or server is launched."""
from contextlib import contextmanager
import copy
import hashlib
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
import live_author_population as author
from live_harness_oracles import OracleFailure
from tools.tests import test_live_fixture_custody as custody_bytes
import live_fixture_population


class PopulationTests(unittest.TestCase):
    def setUp(self):
        fixture = custody_bytes.CustodyTests()
        fixture.setUp()
        self.source = SimpleNamespace(generation=1, lineage=b"unit-lineage", sha256="a" * 64,
                                      path=Path("unit-harbor/harbor.sav"))
        self.data = b"synthetic-population-never-uploaded"
        self.saved = SimpleNamespace(generation=2, lineage=self.source.lineage, sha256=hashlib.sha256(self.data).hexdigest())
        self.recipe = {"abi": "hoenn-box80-v1", "party_species": [25] * 6,
                       "party_level": 5, "pc_species": [25], "bag_items": [[2, 3]]}
        self.before = {0x0200: b"\xbb\xff" + bytes(6), 0x0205: fixture.trainer,
                       0x0102: struct.pack("<I", 3000),
                       0x0100: struct.pack("<hh", 9, 12) + bytes((13, 10, 0, 0)) + bytes(556),
                       0x0106: bytes(8)}
        self.after = copy.deepcopy(self.before)
        party = bytearray(604)
        party[0] = 6
        boxes = []
        for slot in range(7):
            box = bytearray(fixture.box(17 + slot, 0))
            box[20:27] = b"\xbb\xff" + bytes(5)
            boxes.append(bytes(box))
            if slot < 6:
                start = 4 + slot * 100
                party[start:start + 80] = box
                party[start + 84] = 5
                party[start + 85] = 255
        self.after[0x0101] = bytes(party)
        self.after[0x0301] = boxes[6] + bytes(33600 - 80)
        self.after[0x0106] = struct.pack("<HHHH", 2, 3, 0, 0)

    @contextmanager
    def oracle(self):
        with mock.patch.object(author, "read_flash_bytes", return_value=self.saved), \
             mock.patch.object(live_fixture_population, "logical_field", side_effect=lambda save, d, f: (self.before if save is self.source else self.after)[f]), \
             mock.patch.object(author, "logical_field", side_effect=lambda save, d, f: (self.before if save is self.source else self.after)[f]):
            yield

    def test_gen2_population_real_species_checksum_and_source_derived_ots(self):
        with self.oracle():
            result = author.validate_population(self.data, self.source, b"d", self.recipe, Path("unit.sav"))
        self.assertEqual(len(result["identities"]), 7)
        self.assertEqual([i["personality"] for i in result["identities"]], list(range(17, 24)))
        self.assertTrue(all(i["held_item"] == 0 for i in result["identities"]))
        self.assertEqual(len(result["population"]["shared_witnesses"]), 3)

    def test_wrong_generation_lineage_money_location_ot_name_and_checksum_rejected(self):
        baseline = copy.deepcopy(self.after)
        with self.oracle():
            for generation in (1, 3):
                self.saved.generation = generation
                with self.assertRaises(OracleFailure):
                    author.validate_population(self.data, self.source, b"d", self.recipe, Path("unit.sav"))
            self.saved.generation = 2
            self.saved.lineage = b"other"
            with self.assertRaises(OracleFailure):
                author.validate_population(self.data, self.source, b"d", self.recipe, Path("unit.sav"))
            self.saved.lineage = self.source.lineage
            for field, offset in ((0x0102, 0), (0x0100, 6), (0x0101, 24), (0x0101, 8), (0x0101, 36)):
                self.after = copy.deepcopy(baseline)
                value = bytearray(self.after[field])
                value[offset] ^= 1
                self.after[field] = bytes(value)
                with self.subTest(field=field, offset=offset), self.assertRaises(OracleFailure):
                    author.validate_population(self.data, self.source, b"d", self.recipe, Path("unit.sav"))

    def test_source_recipe_and_save_cursor_are_explicit_bounded_controls(self):
        controls = author.controls(self.recipe)
        self.assertEqual([label for label, _ in controls[:7]], [f"gift-{i}" for i in range(7)])
        self.assertEqual(dict(controls)["population-save-prompt"], [(128, 8, 20)] * 5 + [(1, 8, 60)])
        self.assertEqual(controls[-3:], (("population-overwrite-prompt", [(1, 8, 60)]),
            ("population-overwrite-ready", [(0, 1, 300)]), ("population-saved", [(1, 8, 600)])))
        self.assertLess(sum(h + w for _, group in controls for _, h, w in group), 72000)
        bad = dict(self.recipe, party_species=[33] * 6)
        with self.assertRaises(OracleFailure):
            author.controls(bad)

    def test_packed_held_item_rejects_actual_items_and_preserves_upper_six_bits(self):
        fixture = custody_bytes.CustodyTests()
        fixture.setUp()
        for held in (0, 1, 1023):
            with self.subTest(held=held):
                # Independent canonical encoder sets all six upper growth bits;
                # real held item occupies only bits16..25 in this fork's ABI.
                record = bytearray(fixture.box(17, held))
                record[20:27] = b"\xbb\xff" + bytes(5)
                self.assertEqual(author.box_species(record), 25)
                party = bytearray(self.after[0x0101])
                party[4:84] = record
                self.after[0x0101] = bytes(party)
                with self.oracle():
                    if held:
                        with self.assertRaisesRegex(OracleFailure, "holds an item"):
                            author.validate_population(self.data, self.source, b"d", self.recipe, Path("unit.sav"))
                    else:
                        result = author.validate_population(self.data, self.source, b"d", self.recipe, Path("unit.sav"))
                        self.assertEqual(result["identities"][0]["held_item"], 0)
                        self.assertEqual(result["identities"][0]["box_sha256"], hashlib.sha256(record).hexdigest())

    def test_harbor_source_admission_checks_current_bindings_player_and_dirty_bag(self):
        with tempfile.TemporaryDirectory() as folder, self.oracle():
            root = Path(folder)
            files = [root / name for name in ("ROM", "exe", "config")]
            for file in files:
                file.write_bytes(file.name.encode())
            bindings = {k: "unit" for k in ("release_id", "envelope_sha256", "catalog_sha256", "server_catalog_sha256")}
            inputs = {"name": "A", "initial_save": "absent", "menu_profile": "hoenn-debug-v1",
                "signed_fixture": bindings, "pillow_version": author.harbor.PILLOW_VERSION,
                "intro_templates": {k: {"roi": list(v[0]), "sha256": v[1]} for k, v in author.harbor.TEMPLATES.items()},
                "descriptor_sha256": hashlib.sha256(b"d").hexdigest(),
                "dependencies": {dep: author.harness.digest(Path(author.__file__).with_name(dep)) for dep in author.HARBOR_DEPS},
                **{key: author.harness.digest(file) for key, file in zip(("rom_sha256", "emulator_sha256", "config_sha256"), files)}}
            receipt = {"harbor": {"save_sha256": self.source.sha256}}
            with mock.patch.object(author.harbor, "cached_receipt", return_value=receipt), \
                 mock.patch.object(author, "seed_bytes", return_value=(b"source", self.source)), \
                 mock.patch.object(author, "owned_scripted_emulator") as launch:
                for changed in (None, "name", "dependencies", "config_sha256", "signed_fixture", "dirty"):
                    candidate = copy.deepcopy(inputs)
                    self.before[0x0106] = bytes(8)
                    if changed == "dirty":
                        self.before[0x0106] = struct.pack("<HHHH", 2, 1, 0, 0)
                    elif changed:
                        candidate[changed] = "invalid"
                    (root / "inputs.json").write_text(json.dumps(candidate))
                    if changed:
                        with self.assertRaises(OracleFailure):
                            author.validate_harbor_root(root, bindings, "a", b"d", *files)
                    else:
                        self.assertEqual(author.validate_harbor_root(root, bindings, "a", b"d", *files)[1], self.source)
                self.before[0x0106] = bytes(8)
                (root / "inputs.json").write_text(json.dumps(inputs))
                self.source.generation = 2
                with self.assertRaises(OracleFailure):
                    author.validate_harbor_root(root, bindings, "a", b"d", *files)
                (root / "inputs.json").write_text("null")
                with self.assertRaises(OracleFailure):
                    author.validate_harbor_root(root, bindings, "a", b"d", *files)
                launch.assert_not_called()

    def test_preflight_rejection_has_zero_owned_launches(self):
        with mock.patch.object(author.harness, "preflight", side_effect=OracleFailure("signed rejection")), \
             mock.patch.object(author, "owned_scripted_emulator") as launch:
            with self.assertRaises(OracleFailure):
                author.author_player({}, "a", Path("harbor"), Path("unused"), Path("config"))
            launch.assert_not_called()

    def test_bad_harbor_source_rejected_before_any_owned_launch_or_output(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "runtime").mkdir()
            (root / "runtime/mgba.exe").write_bytes(b"unit-exe")
            (root / "game.gba").write_bytes(b"unit-ROM")
            (root / "release_catalog.json").write_text(json.dumps({"worlds": [{"world_id": 1,
                "rom_path": "game.gba", "rom_sha256": hashlib.sha256(b"unit-ROM").hexdigest()}]}))
            (root / "server-build-catalog.json").write_text(json.dumps({"shared_player_descriptor_hex": "abcd"}))
            plan = {"release_dir": str(root), "players": [{"name": "a", "authoring_menu_profile": "hoenn-debug-v1", "population_recipe": self.recipe}]}
            output = root / "unused"
            with mock.patch.object(author.harness, "preflight"), mock.patch.object(author.harness, "check_c_space"), \
                 mock.patch.object(author, "config_check"), \
                 mock.patch.object(author, "validate_harbor_root", side_effect=OracleFailure("dirty/gen2/wrong trainer")), \
                 mock.patch.object(author, "owned_scripted_emulator") as launch, \
                 mock.patch.dict(author.os.environ, {"SystemDrive": "Q:"}):
                with self.assertRaises(OracleFailure):
                    author.author_player(plan, "a", root, output, root / "config")
                launch.assert_not_called()
                self.assertFalse(output.exists())

    def test_owned_cleanup_semantic_before_cold_immutable_cache_and_source_recheck(self):
        for failure in (None, "semantic", "input", "closure", "cold", "source"):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as folder, self.oracle():
                self.saved.generation = 1 if failure == "semantic" else 2
                root = Path(folder)
                release = root / "release"
                (release / "runtime").mkdir(parents=True)
                (release / "runtime/mgba.exe").write_bytes(b"unit-exe")
                (release / "game.gba").write_bytes(b"unit-ROM")
                (release / "release_catalog.json").write_text(json.dumps({"worlds": [{"world_id": 1,
                    "rom_path": "game.gba", "rom_sha256": hashlib.sha256(b"unit-ROM").hexdigest()}]}))
                (release / "server-build-catalog.json").write_text(json.dumps({"shared_player_descriptor_hex": "abcd"}))
                config = root / "config"
                config.write_bytes(b"unit-config")
                harbor_root = root / "harbor"
                harbor_root.mkdir()
                (harbor_root / "receipt.json").write_bytes(b"unit-receipt")
                initial = b"unit-gen1"
                (harbor_root / "harbor.sav").write_bytes(initial)
                self.source.path = harbor_root / "harbor.sav"
                parent = {"cache_key": "unit", "inputs": {"signed_fixture": {}}}
                plan = {"release_dir": str(release), "players": [{"name": "a", "authoring_menu_profile": "hoenn-debug-v1", "population_recipe": self.recipe}]}
                opened, closed = [], []
                test = self
                @contextmanager
                def driver(exe, stage, env):
                    opened.append(stage.name)
                    test.assertEqual((stage / "game.sav").read_bytes(), initial if stage.name == "author" else test.data)
                    class Script:
                        def act(self, label, mask, hold, wait):
                            if failure == "input" and label == "gift-0-0":
                                raise OracleFailure("input failure")
                            image = stage / (label + ".png")
                            image.write_bytes(b"\x89PNG\r\n\x1a\nunit")
                            if label == "population-saved":
                                (stage / "game.sav").write_bytes(test.data)
                            if label == "cold-party":
                                if failure == "cold":
                                    (stage / "game.sav").write_bytes(b"different")
                                if failure == "source":
                                    (harbor_root / "harbor.sav").write_bytes(b"different")
                            return image
                    try:
                        yield Script()
                    finally:
                        closed.append(stage.name)
                        if failure == "closure" and stage.name == "author":
                            (stage / "game.sav").write_bytes(b"different")
                def validate_parent(*args):
                    if (harbor_root / "harbor.sav").read_bytes() != initial:
                        raise OracleFailure("source changed")
                    return initial, self.source, parent
                with mock.patch.object(author.harness, "preflight"), mock.patch.object(author.harness, "check_c_space"), \
                     mock.patch.object(author, "config_check"), mock.patch.object(author, "validate_harbor_root", side_effect=validate_parent), \
                     mock.patch.object(author, "owned_scripted_emulator", side_effect=driver), \
                     mock.patch.dict(author.os.environ, {"SystemDrive": "Q:"}):
                    output = root / "output"
                    if failure:
                        with self.assertRaises(OracleFailure):
                            author.author_player(plan, "a", harbor_root, output, config)
                        self.assertEqual(opened, closed)
                        if failure == "semantic":
                            self.assertEqual(opened, ["author"])
                        self.assertFalse(list(output.glob("*/receipt.json")))
                    else:
                        receipt = author.author_player(plan, "a", harbor_root, output, config)
                        self.assertEqual(opened, closed)
                        self.assertEqual(author.author_player(plan, "a", harbor_root, output, config), receipt)
                        self.assertEqual(opened, ["author", "cold"])
                        self.assertEqual(receipt["seed_lineage"][0]["sha256"], self.source.sha256)
                        cached = output / receipt["cache_key"]
                        original_receipt = (cached / "receipt.json").read_bytes()
                        wrong_source_path = copy.deepcopy(receipt["seed_lineage"])
                        wrong_source_path[0]["path"] = str(root / "substituted.sav")
                        wrong_hash = copy.deepcopy(receipt["seed_lineage"])
                        wrong_hash[1]["sha256"] = "b" * 64
                        for lineage in (None, [], receipt["seed_lineage"][::-1], wrong_source_path,
                                        wrong_hash, receipt["seed_lineage"] + [receipt["seed_lineage"][0]]):
                            candidate = dict(receipt, seed_lineage=lineage)
                            (cached / "receipt.json").write_text(json.dumps(candidate))
                            with self.assertRaisesRegex(OracleFailure, "ancestry export"):
                                author.author_player(plan, "a", harbor_root, output, config)
                            self.assertEqual(opened, ["author", "cold"])
                        missing = dict(receipt)
                        missing.pop("seed_lineage")
                        (cached / "receipt.json").write_text(json.dumps(missing))
                        with self.assertRaisesRegex(OracleFailure, "ancestry export"):
                            author.author_player(plan, "a", harbor_root, output, config)
                        (cached / "receipt.json").write_bytes(original_receipt)
                        (cached / "cold/game.sav").write_bytes(b"tampered")
                        with self.assertRaises(OracleFailure):
                            author.author_player(plan, "a", harbor_root, output, config)
                        self.assertEqual(opened, ["author", "cold"])


if __name__ == "__main__":
    unittest.main()
