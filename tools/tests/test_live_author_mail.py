"""Synthetic semantic/cache tests; never launch an emulator or upload bytes."""
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
import live_author_mail as author
from live_harness_oracles import OracleFailure
from tools.tests import test_live_fixture_custody as unit_bytes


class FirstMailTests(unittest.TestCase):
    def setUp(self):
        self.fixture = unit_bytes.CustodyTests()
        self.fixture.setUp()
        self.data = b"unit-only-save"
        self.source = SimpleNamespace(generation=7, lineage=b"trainer", sha256="a" * 64)
        self.saved = SimpleNamespace(generation=8, lineage=b"trainer", sha256=hashlib.sha256(self.data).hexdigest())
        self.recipe = {"abi": "hoenn-box80-v1", "party_species": [25] * 6,
                       "party_level": 5, "pc_species": [25], "bag_items": [[3, 4]]}
        party = bytearray(604)
        party[0] = 6
        for slot in range(6):
            base = 4 + slot * 100
            party[base:base + 80] = self.fixture.box(17 + slot, 0)
            party[base + 84] = 5
            party[base + 85] = 255
        mail = bytearray(576)
        for slot in range(16):
            struct.pack_into("<9H", mail, slot * 36, *([65535] * 9))
        name = bytes.fromhex("bbbcbdbebfb0ffff")
        self.before = {0x0101: bytes(party), 0x010B: bytes(mail), 0x0301: bytes(self.fixture.box(23, 0)) + bytes(33600 - 80),
                       0x0106: struct.pack("<HHHH", 3, 4, 0, 0), 0x0200: name,
                       0x0205: self.fixture.trainer, 0x0100: b"\x08\x00\x0b\x00\x0d\x0a\xff\x00" + bytes(556)}
        self.after = copy.deepcopy(self.before)
        party[4:84] = self.fixture.box(17, 200)
        party[89] = 0
        self.after[0x0101] = bytes(party)
        letter = struct.pack("<9H", *author.WORDS) + name + self.fixture.trainer + struct.pack("<HH", 25, 200) + b"\xaa\xbb"
        mail[:36] = letter
        self.after[0x010B] = bytes(mail)
        self.after[0x0106] = struct.pack("<HHHH", 3, 4, 200, 2)

    @contextmanager
    def fields(self):
        def logical(save, descriptor, fid):
            return (self.before if save is self.source else self.after)[fid]
        with mock.patch.object(author, "read_flash_bytes", return_value=self.saved) as read, \
             mock.patch.object(author, "logical_field", side_effect=logical), \
             mock.patch("live_fixture_population.logical_field", side_effect=logical):
            yield read

    def verify(self):
        with self.fields():
            return author.validate_first_mail(self.data, self.source, b"descriptor", self.recipe, Path("unit.sav"))

    def test_valid_original_recipe_identity_and_words(self):
        receipt = self.verify()
        self.assertEqual(receipt["carrier"], {"species": 25, "personality": 17, "ot_id": 0x1234ABCD})
        self.assertEqual(receipt["message"]["words"], author.WORDS)
        self.assertEqual(receipt["population"]["bag_items"], [[3, 4], [200, 2]])
        self.assertEqual(receipt["sender_name_hex"], self.before[0x0200].hex())

    def test_lineage_and_generation_fail_before_retention(self):
        for field, value in (("generation", 7), ("generation", 9), ("lineage", b"other")):
            old = getattr(self.saved, field)
            setattr(self.saved, field, value)
            with tempfile.TemporaryDirectory() as folder, self.fields():
                target = Path(folder) / "first-mail.sav"
                with self.assertRaisesRegex(OracleFailure, "generation/trainer"):
                    author.retain_first_mail(self.data, target, self.source, b"", self.recipe)
                self.assertFalse(target.exists())
            setattr(self.saved, field, old)

    def test_each_semantic_boundary_rejects_changed_data(self):
        for fid, offset in ((0x0101, 89), (0x0101, 104), (0x0101, 36), (0x0301, 0),
                            (0x010B, 0), (0x010B, 18), (0x010B, 26), (0x010B, 32),
                            (0x010B, 40), (0x0106, 2), (0x0205, 0), (0x0100, 0)):
            original = self.after[fid]
            changed = bytearray(original)
            changed[offset] ^= 1
            self.after[fid] = bytes(changed)
            with self.subTest(fid=fid, offset=offset), self.assertRaises(OracleFailure):
                self.verify()
            self.after[fid] = original
        party = bytearray(self.after[0x0101])
        party[4:84] = self.fixture.box(18, 200)
        self.after[0x0101] = bytes(party)
        with self.assertRaisesRegex(OracleFailure, "identity"):
            self.verify()

    def test_name_padding_and_full_length_sender(self):
        for name, expected in ((b"\xbb\xff" + b"\xee" * 6, b"\xbb" + bytes(5) + b"\xff\xff"),
                               (b"\xbb" * 7 + b"\xff", b"\xbb" * 7 + b"\xff")):
            self.before[0x0200] = name
            with self.fields():
                self.assertEqual(author._sender(self.source, b"")[0], expected)

    def test_immutable_capture_is_validated_and_retained_once(self):
        with tempfile.TemporaryDirectory() as folder, self.fields() as read:
            target = Path(folder) / "first-mail.sav"
            author.retain_first_mail(self.data, target, self.source, b"descriptor", self.recipe)
            read.assert_called_once_with(self.data, target)
            self.assertEqual(target.read_bytes(), self.data)
            with self.assertRaises(FileExistsError):
                author.retain_first_mail(self.data, target, self.source, b"descriptor", self.recipe)

    def test_cache_requires_exact_inputs_retained_hash_and_cold_screenshots(self):
        with tempfile.TemporaryDirectory() as folder, self.fields():
            root = Path(folder)
            inputs = {"seed": "pinned", "driver": "pinned"}
            checked = self.verify()
            (root / "first-mail.sav").write_bytes(self.data)
            png = b"\x89PNG\r\n\x1a\nunit"
            screenshots = {}
            for name in ("author/first-mail-saved.png", "cold/cold-party.png"):
                path = root / name
                path.parent.mkdir(exist_ok=True)
                path.write_bytes(png)
                screenshots[name] = hashlib.sha256(png).hexdigest()
            (root / "cold/game.sav").write_bytes(self.data)
            receipt = {"inputs": inputs, "cache_key": author.cache_key(inputs), "first_mail": checked,
                       "cold_save_sha256": checked["save_sha256"], "screenshots": screenshots}
            (root / "receipt.json").write_text(json.dumps(receipt))
            self.assertEqual(author.cached_receipt(root, inputs, self.source, b"", self.recipe), receipt)
            with self.assertRaisesRegex(OracleFailure, "inputs changed"):
                author.cached_receipt(root, dict(inputs, driver="changed"), self.source, b"", self.recipe)
            (root / "first-mail.sav").write_bytes(self.data + b"changed")
            with mock.patch.object(author, "read_flash_bytes", side_effect=lambda data, path: SimpleNamespace(
                    generation=8, lineage=self.source.lineage, sha256=hashlib.sha256(data).hexdigest())):
                with self.assertRaisesRegex(OracleFailure, "validated exact cold"):
                    author.cached_receipt(root, inputs, self.source, b"", self.recipe)
            (root / "first-mail.sav").write_bytes(self.data)
            (root / "cold/game.sav").write_bytes(self.data + b"changed")
            with self.assertRaisesRegex(OracleFailure, "cached cold save"):
                author.cached_receipt(root, inputs, self.source, b"", self.recipe)
            (root / "cold/game.sav").write_bytes(self.data)
            for mutation in ("cold hash", "missing screenshot", "screenshot bytes", "semantic receipt"):
                damaged = copy.deepcopy(receipt)
                if mutation == "cold hash":
                    damaged["cold_save_sha256"] = "0" * 64
                elif mutation == "missing screenshot":
                    damaged["screenshots"].pop("cold/cold-party.png")
                elif mutation == "screenshot bytes":
                    damaged["screenshots"]["cold/cold-party.png"] = "0" * 64
                else:
                    damaged["first_mail"]["carrier"]["personality"] = 0
                (root / "receipt.json").write_text(json.dumps(damaged))
                with self.subTest(mutation=mutation), self.assertRaises(OracleFailure):
                    author.cached_receipt(root, inputs, self.source, b"", self.recipe)

    def test_recipe_and_preflight_failure_prevent_emulator_launch(self):
        player = {"population_recipe": self.recipe, "authoring_menu_profile": "hoenn-debug-v1"}
        self.assertEqual(author.recipe_check(player), self.recipe)
        for recipe in (None, dict(self.recipe, party_species=[1]), dict(self.recipe, bag_items=[[200, 1]])):
            with self.assertRaises(OracleFailure):
                author.recipe_check(dict(player, population_recipe=recipe))
        with mock.patch.object(author.harness, "preflight", side_effect=OracleFailure("preflight rejected")), \
             mock.patch.object(author, "owned_scripted_emulator") as launch:
            with self.assertRaisesRegex(OracleFailure, "preflight rejected"):
                author.author_player({}, player, Path("S:/unit"), Path("config"))
            launch.assert_not_called()

    def test_both_rom_written_harbor_warps_keep_exact_source_location(self):
        for warp in (0, 255):
            with self.subTest(warp=warp), self.fields():
                location = b"\x09\x00\x0c\x00\x0d\x0a" + bytes((warp, 0)) + bytes(556)
                self.before[0x0100] = self.after[0x0100] = location
                self.assertEqual(author.validate_first_mail(self.data, self.source, b"d", self.recipe, Path("unit.sav"))["generation"], 8)
                changed = bytearray(location)
                changed[6] = 255 if warp == 0 else 0
                self.after[0x0100] = bytes(changed)
                with self.assertRaisesRegex(OracleFailure, "position/map"):
                    author.validate_first_mail(self.data, self.source, b"d", self.recipe, Path("unit.sav"))

    def test_foreign_harbor_map_or_warp_fails_before_launch_or_api(self):
        with tempfile.TemporaryDirectory() as folder, self.fields():
            root = Path(folder)
            release = root / "release"
            release.mkdir()
            (release / "release_catalog.json").write_text(json.dumps({"worlds": [
                {"world_id": 1, "rom_path": "game.gba", "rom_sha256": "unit"}]}))
            (release / "server-build-catalog.json").write_text(json.dumps({"shared_player_descriptor_hex": "abcd"}))
            player = {"source_save": str(root / "source.sav"), "source_sha256": self.source.sha256,
                      "authoring_menu_profile": "hoenn-debug-v1", "population_recipe": self.recipe}
            plan = {"release_dir": str(release), "players": [player]}
            with mock.patch.object(author.harness, "preflight"), mock.patch.object(author.harness, "check_c_space"), \
                 mock.patch.object(author, "config_check"), mock.patch.object(author, "check_population"), \
                 mock.patch.object(author, "seed_bytes", return_value=(b"unit-only-source", self.source)), \
                 mock.patch.object(author, "owned_scripted_emulator") as launch, \
                 mock.patch("live_group_evidence._call") as api, \
                 mock.patch.dict(author.os.environ, {"SystemDrive": "Q:"}):
                for group, map_num, warp in ((13, 10, 1), (13, 10, 254), (12, 10, 0), (13, 9, 255)):
                    self.before[0x0100] = bytes(4) + bytes((group, map_num, warp, 0)) + bytes(556)
                    with self.subTest(group=group, map_num=map_num, warp=warp), self.assertRaisesRegex(OracleFailure, "Main harbor fixture"):
                        author.author_player(plan, player, root / "unused", root / "config")
                launch.assert_not_called()
                api.assert_not_called()
                self.assertFalse((root / "unused").exists())

    def test_authoring_itinerary_preserves_verified_save_waits(self):
        controls = dict(author.CONTROLS)
        self.assertEqual(controls["debug-menu"], [(264, 8, 60)])
        # A confirms writing. Wait out the printer, then B dismisses its final
        # PAUSE_UNTIL_PRESS; a separate B closes the returned Bag to Start.
        labels = [label for label, _ in author.CONTROLS]
        start = labels.index("mail-delivered")
        self.assertEqual(author.CONTROLS[start:start + 4], (
            ("mail-delivered", [(1, 8, 20)]),
            ("gift-message-ready", [(0, 1, 600)]),
            ("bag-after-mail", [(2, 8, 180)]),
            ("post-bag-menu", [(2, 8, 180)]),
        ))
        # Save Yes must follow a released-key printer wait, then overwrite Yes
        # follows its own released-key wait. The final write gets 600 frames.
        save = labels.index("save-confirm")
        self.assertEqual(author.CONTROLS[save:], (
            ("save-confirm", [(128, 8, 20)] * 4 + [(1, 8, 60)]),
            ("save-confirm-ready", [(0, 1, 300)]),
            ("overwrite-confirm", [(1, 8, 20)]),
            ("overwrite-ready", [(0, 1, 300)]),
            ("first-mail-saved", [(1, 8, 600)]),
        ))
        self.assertEqual(controls["overwrite-ready"], [(0, 1, 300)])
        self.assertEqual(controls["first-mail-saved"], [(1, 8, 600)])

    def test_orchestration_closes_both_owned_sessions_reuses_cache_and_rechecks_source(self):
        for warp, failure in ((warp, failure) for warp in (0, 255)
                              for failure in (None, "input", "source", "initial source")):
            with self.subTest(warp=warp, failure=failure), tempfile.TemporaryDirectory() as folder, self.fields():
                location = b"\x09\x00\x0c\x00\x0d\x0a" + bytes((warp, 0)) + bytes(556)
                self.before[0x0100] = self.after[0x0100] = location
                root = Path(folder)
                release = root / "release"
                release.mkdir()
                (release / "runtime").mkdir()
                (release / "runtime/mgba.exe").write_bytes(b"unit-only-executable")
                (release / "game.gba").write_bytes(b"unit-only-ROM")
                (release / "release_catalog.json").write_text(json.dumps({"worlds": [
                    {"world_id": 1, "rom_path": "game.gba", "rom_sha256": hashlib.sha256(b"unit-only-ROM").hexdigest()}]}))
                (release / "server-build-catalog.json").write_text(json.dumps({"shared_player_descriptor_hex": "abcd"}))
                config = root / "config.ini"
                config.write_text("unit-only-config")
                source_path = root / "source.sav"
                source_path.write_bytes(b"unit-only-seed")
                player = {"source_save": str(source_path), "source_sha256": self.source.sha256,
                          "authoring_menu_profile": "hoenn-debug-v1", "population_recipe": self.recipe}
                plan = {"release_dir": str(release), "players": [player],
                        "release_id": "unit", "envelope_sha256": "unit-envelope",
                        "catalog_sha256": "unit-catalog", "server_catalog_sha256": "unit-server"}
                entered, closed = [], []
                owner = self

                class Script:
                    def __init__(self, path):
                        self.path = path

                    def act(self, label, mask, **kwargs):
                        if failure == "input" and label == "mail-editor":
                            raise OracleFailure("injected input failure")
                        image = self.path / (label + ".png")
                        image.write_bytes(b"\x89PNG\r\n\x1a\nunit")
                        if label == "first-mail-saved":
                            (self.path / "game.sav").write_bytes(owner.data)
                        return image

                @contextmanager
                def owned(exe, path, env):
                    entered.append(path.name)
                    try:
                        yield Script(path)
                    finally:
                        closed.append(path.name)

                seeds = [(b"unit-only-seed", self.source)]
                seeds += [OracleFailure("injected changed source")] if failure == "source" else [(b"unit-only-seed", self.source)] * 2
                if failure == "initial source":
                    seeds = [OracleFailure("injected source hash mismatch")]
                with mock.patch.object(author.harness, "preflight") as preflight, \
                     mock.patch.object(author.harness, "check_c_space"), \
                     mock.patch.object(author, "config_check"), \
                     mock.patch.object(author, "seed_bytes", side_effect=seeds) as source_check, \
                     mock.patch.object(author, "owned_scripted_emulator", side_effect=owned), \
                     mock.patch.dict(author.os.environ, {"SystemDrive": "Z:"}):
                    if failure:
                        with self.assertRaisesRegex(OracleFailure, "injected"):
                            author.author_player(plan, player, root / "output", config)
                        self.assertFalse(list((root / "output").glob("*/receipt.json")))
                    else:
                        receipt = author.author_player(plan, player, root / "output", config)
                        self.assertEqual(receipt["cold_save_sha256"], self.saved.sha256)
                        self.assertEqual(source_check.call_count, 2)
                        reused = author.author_player(plan, player, root / "output", config)
                        self.assertEqual(reused, receipt)
                        self.assertEqual(entered, ["author", "cold"])
                    self.assertEqual(entered, closed)
                    self.assertEqual(source_path.read_bytes(), b"unit-only-seed")
                    preflight.assert_called_with(plan, profiles_ready=False)


if __name__ == "__main__":
    unittest.main()
