"""Synthetic custody bytes and fake owned stages, never uploaded fixtures."""
from contextlib import ExitStack, contextmanager
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
import live_author_daycare as author
from live_harness_oracles import OracleFailure
from tools.tests import test_live_author_third_mail as canonical


class DaycareTests(unittest.TestCase):
    def setUp(self):
        fixture = canonical.ThirdMailTests()
        fixture.setUp()
        self.recipe = fixture.recipe
        self.data = b"synthetic-daycare-never-upload"
        self.source = SimpleNamespace(generation=5, lineage=b"unit", sha256="a" * 64, path=Path("third/third-mail.sav"),
                                      field=lambda storage, offset, size: struct.pack("<H", self.before_friend))
        self.saved = SimpleNamespace(generation=6, lineage=b"unit", sha256=hashlib.sha256(self.data).hexdigest(),
                                     field=lambda storage, offset, size: struct.pack("<H", self.after_friend))
        self.before_friend, self.after_friend = 4, 14
        self.before = copy.deepcopy(fixture.after)
        party = bytearray(self.before[0x0101])
        for slot in range(6):
            party[4 + slot * 100 + 18] = 2
            struct.pack_into("<H", party, 4 + slot * 100 + 88, 20 + slot)
        party[212:222] = b"\xbd\xff\x52" + bytes(7)
        self.before[0x0101] = bytes(party)
        dc = bytearray(288)
        struct.pack_into("<I", dc, 284, 4)
        self.before[0x010D] = bytes(dc)
        stats = [0] * 64
        stats[0], stats[5] = 5, 4
        self.before[0x0113] = struct.pack("<64I", *stats)
        self.after = copy.deepcopy(self.before)
        compact = bytearray(604)
        compact[:4] = bytes((5,)) + party[1:4]
        compact[4:204] = party[4:204]
        compact[204:504] = party[304:604]
        struct.pack_into("<H", compact, 504 + 30, 25)
        compact[504 + 85] = 255
        self.after[0x0101] = bytes(compact)
        original = party[204:284]
        box = canonical.canonical.PcMailTests.box(27, 19, 0)
        box[:28] = original[:28]
        box[30:32] = original[30:32]
        dc[:80] = box
        dc[80:116] = self.before[0x010B][36:72]
        dc[116:118] = b"\xbb\xff"
        dc[124:134] = original[8:18]
        dc[135] = 0x22
        struct.pack_into("<I", dc, 136, 6)
        struct.pack_into("<I", dc, 284, 14)
        self.after[0x010D] = bytes(dc)
        mail = bytearray(self.before[0x010B])
        struct.pack_into("<H", mail, 68, 0)
        self.after[0x010B] = bytes(mail)
        stats[0], stats[5], stats[47] = 6, 14, 1
        self.after[0x0113] = struct.pack("<64I", *stats)
        self.after[0x0100] = struct.pack("<hh", 8, 11) + bytes((13, 10, 0, 0))

    def oracle(self):
        stack = ExitStack()
        def field(save, descriptor, fid): return (self.before if save is self.source else self.after)[fid]
        stack.enter_context(mock.patch.object(author, "read_flash_bytes", return_value=self.saved))
        for module in (author, author.held_mail, sys.modules["live_fixture_custody"], sys.modules["live_fixture_population"]):
            stack.enter_context(mock.patch.object(module, "logical_field", side_effect=field))
        stack.enter_context(mock.patch.object(author, "parse_schema_payload", return_value={"fields": [
            {"id": 0x0113, "offset": 2000, "storage": 0, "size": 256},
            {"id": 0x0108, "offset": 1000, "storage": 0, "size": 1000}]}))
        return stack

    def check(self, **kwargs):
        with self.oracle(): return author.validate_daycare(self.data, self.source, b"", self.recipe, Path("unit"), **kwargs)

    def test_full_custody_recipe_and_compacted_party(self):
        checked = self.check()
        self.assertEqual(checked["population_recipe"]["party_species"], [25, 26, 28, 29, 30])
        self.assertEqual([(w["field_id"], w["size"]) for w in checked["shared_witnesses"]], [(0x0101, 604), (0x010B, 576), (0x010D, 288)])
        self.assertEqual(checked["custody_recipe"]["daycare"][0]["steps"], 6)
        self.assertEqual(checked["custody_recipe"]["daycare"][0]["mon_name_hex"], "bdff520000000000000000")
        self.assertEqual((checked["total_steps"], checked["post_deposit_steps"]), (10, 6))

    def test_native_empty_tail_is_not_all_zero(self):
        self.check()
        changed = bytearray(self.after[0x0101])
        changed[504:] = bytes(100)
        self.after[0x0101] = bytes(changed)
        with self.assertRaises(OracleFailure): self.check()

    def test_compaction_other_records_and_tail_reject(self):
        old = self.after[0x0101]
        for offset in (0, 104, 205, 405, 534, 589):
            changed = bytearray(old); changed[offset] ^= 1
            self.after[0x0101] = bytes(changed)
            with self.assertRaises(OracleFailure): self.check()
        self.after[0x0101] = old

    def test_daycare_checksum_identity_held_metadata_and_pp_reject(self):
        old = self.after[0x010D]
        for offset in (0, 28, 32, 80, 98, 110, 114, 116, 124, 134, 135, 141, 280, 284):
            changed = bytearray(old); changed[offset] ^= 1
            self.after[0x010D] = bytes(changed)
            with self.assertRaises(OracleFailure): self.check()
        self.after[0x010D] = old

    def test_steps_friendship_and_statistics_reject(self):
        old = self.after[0x010D]
        for steps in (0, 11, 0xFFFFFFFF):
            changed = bytearray(old); struct.pack_into("<I", changed, 136, steps)
            self.after[0x010D] = bytes(changed)
            with self.assertRaises(OracleFailure): self.check()
        self.after[0x010D] = old
        self.after_friend = 15
        with self.assertRaises(OracleFailure): self.check()
        self.before_friend, self.after_friend = 120, 130
        with self.assertRaises(OracleFailure): self.check()
        self.before_friend, self.after_friend = 4, 14
        old = self.after[0x0113]
        for stat in (0, 5, 47, 20):
            changed = bytearray(old); changed[stat * 4] ^= 1
            self.after[0x0113] = bytes(changed)
            with self.assertRaises(OracleFailure): self.check()
        self.after[0x0113] = old

    def test_mail_cleanup_prior_letters_and_bag_pc_identity_reject(self):
        for fid, offsets in ((0x010B, (0, 36, 68, 216)), (0x0106, (0,)), (0x0301, (0,)),
                              (0x0102, (0,)), (0x0200, (0,)), (0x0205, (0,))):
            old = self.after[fid]
            for offset in offsets:
                changed = bytearray(old); changed[offset] ^= 1
                self.after[fid] = bytes(changed)
                with self.assertRaises(OracleFailure): self.check()
            self.after[fid] = old

    def test_position_generation_and_explicit_manual_oracle(self):
        old = self.after[0x0100]
        self.after[0x0100] = struct.pack("<hh", 9, 12) + bytes((13, 10, 0, 0))
        with self.assertRaises(OracleFailure): self.check()
        self.check(expected_position=(9, 12))
        self.after[0x0100] = old
        self.saved.generation = 7
        stats = bytearray(self.after[0x0113]); struct.pack_into("<I", stats, 0, 7)
        self.after[0x0113] = bytes(stats)
        with self.assertRaises(OracleFailure): self.check()
        self.check(expected_generation=7)

    def test_source_dirty_daycare_wrong_generation_or_attachment_reject(self):
        for fid, offset in ((0x010D, 0), (0x010D, 284), (0x0101, 289)):
            old = self.before[fid]
            changed = bytearray(old); changed[offset] = 200 if fid == 0x010D else 255
            self.before[fid] = bytes(changed)
            with self.assertRaises(OracleFailure): self.check()
            self.before[fid] = old
        self.source.generation = 4
        with self.assertRaises(OracleFailure): self.check()

    def test_controls_have_observed_chooser_and_walking_save_boundaries(self):
        controls = dict(author.CONTROLS)
        self.assertEqual(controls["daycare-party-button"], [(128, 8, 60)] * 5)
        self.assertEqual(controls["daycare-third-selected"], [(128, 8, 60)] * 2 + [(1, 8, 120)])
        self.assertEqual(controls["ferry-position"], [(32, 24, 60), (32, 8, 60), (32, 8, 60), (64, 24, 60), (64, 8, 60), (64, 8, 60)])
        self.assertEqual([a[0] for a in controls["daycare-save-confirm"]], [128] * 5 + [1])
        self.assertEqual(controls["daycare-save-ready"], [(0, 1, 300)])
        self.assertLess(sum(h + w for _, actions in author.CONTROLS for _, h, w in actions), 72000)

    def make_cache(self, root):
        inputs = {"unit": "cache"}
        ancestry = [{"path": f"gen{i}", "sha256": str(i) * 64} for i in range(1, 6)]
        images = {}
        for stage, label in (("author", "daycare-third-selected"), ("author", "daycare-deposited-question"),
                             ("author", "daycare-saved"), ("cold", "cold-party")):
            folder = root / stage; folder.mkdir(exist_ok=True)
            (folder / "game.sav").write_bytes(self.data)
            image = b"\x89PNG\r\n\x1a\nunit"
            (folder / (label + ".png")).write_bytes(image)
            images[f"{stage}/{label}.png"] = hashlib.sha256(image).hexdigest()
        (root / "daycare.sav").write_bytes(self.data)
        (root / "inputs.json").write_text(json.dumps(inputs))
        receipt = {"inputs": inputs, "cache_key": author.cache_key(inputs), "validated": self.check(),
                   "seed_lineage": ancestry + [{"path": str((root / "daycare.sav").resolve()), "sha256": self.saved.sha256}],
                   "cold_save_sha256": self.saved.sha256, "screenshots": images}
        (root / "receipt.json").write_text(json.dumps(receipt))
        return inputs, ancestry, receipt

    def test_cache_exact_ancestry_and_runtime_inputs_images(self):
        with tempfile.TemporaryDirectory() as temp, self.oracle(), mock.patch.object(author, "owned_scripted_emulator") as launch:
            root = Path(temp); inputs, ancestry, receipt = self.make_cache(root)
            self.assertEqual(author.cached_receipt(root, inputs, self.source, b"", self.recipe, ancestry), receipt)
            launch.assert_not_called()
            for chain in (receipt["seed_lineage"][:-1], list(reversed(receipt["seed_lineage"])),
                          [dict(receipt["seed_lineage"][0], path="substituted")] + receipt["seed_lineage"][1:]):
                (root / "receipt.json").write_text(json.dumps(dict(receipt, seed_lineage=chain)))
                with self.assertRaises(OracleFailure): author.cached_receipt(root, inputs, self.source, b"", self.recipe, ancestry)
            (root / "receipt.json").write_text(json.dumps(receipt))
            for path in (root / "cold/game.sav", root / "author/game.sav", root / "inputs.json", root / "author/daycare-saved.png"):
                old = path.read_bytes(); path.write_bytes(b'"tampered"')
                with self.assertRaises(OracleFailure): author.cached_receipt(root, inputs, self.source, b"", self.recipe, ancestry)
                path.write_bytes(old)

    @contextmanager
    def environment(self, temp, failure=None):
        base = Path(temp); release = base / "release"
        (release / "runtime").mkdir(parents=True)
        (release / "runtime/mgba.exe").write_bytes(b"exe"); (release / "main.gba").write_bytes(b"rom")
        (release / "release_catalog.json").write_text(json.dumps({"worlds": [{"world_id": 1, "rom_path": "main.gba", "rom_sha256": "unit"}]}))
        (release / "server-build-catalog.json").write_text(json.dumps({"shared_player_descriptor_hex": ""}))
        config = base / "config.ini"; config.write_bytes(b"config")
        third, pc, held = (base / name for name in ("third", "pc", "held"))
        for folder in (third, pc, held): folder.mkdir()
        (third / "receipt.json").write_bytes(b"parent")
        parent = {"cache_key": "unit", "inputs": {"signed_fixture": {"unit": True}}}
        ancestry = [{"path": f"gen{i}", "sha256": str(i) * 64} for i in range(1, 6)]
        plan = {"release_dir": str(release), "players": [{"name": "a", "population_recipe": self.recipe, "authoring_menu_profile": "hoenn-debug-v1"}]}
        launches, closed = [], []
        @contextmanager
        def driver(exe, folder, env):
            launches.append(folder.name)
            class Driver:
                def act(inner, label, mask, *, hold, wait):
                    if failure == "input" and label == "daycare-raise": raise OracleFailure("input")
                    if label == "daycare-saved": (folder / "game.sav").write_bytes(self.data)
                    if failure == "cold" and label == "cold-party": (folder / "game.sav").write_bytes(b"cold changed")
                    image = folder / (label + ".png"); image.write_bytes(b"\x89PNG\r\n\x1a\nunit"); return image
            try: yield Driver()
            finally:
                closed.append(folder.name)
                if failure == "close" and folder.name == "author": (folder / "game.sav").write_bytes(b"close changed")
        with ExitStack() as stack:
            stack.enter_context(self.oracle())
            for name in ("preflight", "check_c_space", "require_hash"): stack.enter_context(mock.patch.object(author.harness, name))
            stack.enter_context(mock.patch.object(author, "config_check")); stack.enter_context(mock.patch.dict("os.environ", {"SystemDrive": "Q:"}))
            prerequisite = stack.enter_context(mock.patch.object(author, "validate_parent", return_value=(b"source", self.source, parent, ancestry)))
            stack.enter_context(mock.patch.object(author, "owned_scripted_emulator", side_effect=driver))
            if failure == "semantic": stack.enter_context(mock.patch.object(author, "validate_daycare", side_effect=OracleFailure("semantic")))
            yield plan, third, pc, held, base / "output", config, launches, closed, prerequisite

    def test_owned_stages_and_cache_zero_relaunch(self):
        with tempfile.TemporaryDirectory() as temp, self.environment(temp) as args:
            plan, third, pc, held, output, config, launches, closed, prerequisite = args
            receipt = author.author_player(plan, "a", third, pc, held, output, config)
            self.assertEqual(author.author_player(plan, "a", third, pc, held, output, config), receipt)
            self.assertEqual(launches, ["author", "cold"]); self.assertEqual(closed, launches)
            self.assertEqual(prerequisite.call_count, 3); self.assertEqual(len(receipt["seed_lineage"]), 6)

    def test_failure_cleanup_and_no_later_launch(self):
        for failure in ("input", "semantic", "close", "cold"):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as temp, self.environment(temp, failure) as args:
                plan, third, pc, held, output, config, launches, closed, _ = args
                with self.assertRaises(OracleFailure): author.author_player(plan, "a", third, pc, held, output, config)
                self.assertEqual(launches, ["author", "cold"] if failure == "cold" else ["author"])
                self.assertEqual(closed, launches); self.assertFalse(list(output.glob("*/receipt.json")))

    def test_parent_rejection_before_launch_or_output(self):
        with tempfile.TemporaryDirectory() as temp, self.environment(temp) as args:
            plan, third, pc, held, output, config, launches, _, prerequisite = args
            prerequisite.side_effect = OracleFailure("stale parent")
            with self.assertRaises(OracleFailure): author.author_player(plan, "a", third, pc, held, output, config)
            self.assertEqual(launches, []); self.assertFalse(output.exists())

    def test_current_third_parent_reconstructed_before_cached_validation(self):
        with tempfile.TemporaryDirectory() as temp, ExitStack() as stack:
            base = Path(temp)
            third, pc, held = (base / name for name in ("third", "pc", "held"))
            for folder in (third, pc, held): folder.mkdir()
            (pc / "receipt.json").write_bytes(b"pc-parent")
            paths = [base / name for name in ("rom", "exe", "config")]
            for path in paths: path.write_bytes(path.name.encode())
            previous = SimpleNamespace(sha256="b" * 64)
            ancestry = [{"path": f"gen{i}", "sha256": str(i) * 64} for i in range(1, 5)]
            parent = {"cache_key": "pc-key", "inputs": {"signed_fixture": {"unit": True}}}
            expected = {"player": "a", "source_sha256": previous.sha256, "recipe": self.recipe, "ancestry": ancestry,
                "parent_cache_key": parent["cache_key"], "parent_receipt_sha256": author.harness.digest(pc / "receipt.json"),
                "rom_sha256": author.harness.digest(paths[0]), "emulator_sha256": author.harness.digest(paths[1]),
                "config_sha256": author.harness.digest(paths[2]), "descriptor_sha256": hashlib.sha256(b"").hexdigest(),
                "signed_fixture": parent["inputs"]["signed_fixture"],
                "dependencies": {name: author.harness.digest(Path(author.__file__).with_name(name)) for name in author.PARENT_DEPS}}
            (third / "inputs.json").write_text(json.dumps(expected))
            self.source.path = third / "third-mail.sav"
            receipt = {"validated": {"save_sha256": self.source.sha256}, "seed_lineage": ancestry + [
                {"path": str(self.source.path.resolve()), "sha256": self.source.sha256}]}
            stack.enter_context(mock.patch.object(author.third_mail, "validate_parent", return_value=(b"pc", previous, parent, ancestry)))
            cached = stack.enter_context(mock.patch.object(author.third_mail, "cached_receipt", return_value=receipt))
            stack.enter_context(mock.patch.object(author, "seed_bytes", return_value=(b"third", self.source)))
            stack.enter_context(self.oracle())
            player = {"name": "a", "population_recipe": self.recipe, "authoring_menu_profile": "hoenn-debug-v1"}
            self.assertEqual(len(author.validate_parent(third, pc, held, {}, player, b"", *paths)[3]), 5)
            for key in ("signed_fixture", "dependencies", "source_sha256", "ancestry", "parent_receipt_sha256", "player"):
                (third / "inputs.json").write_text(json.dumps(dict(expected, **{key: "forged"})))
                cached.reset_mock()
                with self.assertRaises(OracleFailure): author.validate_parent(third, pc, held, {}, player, b"", *paths)
                cached.assert_not_called()
            (third / "inputs.json").write_text(json.dumps(expected))
            receipt["seed_lineage"] = list(reversed(receipt["seed_lineage"]))
            with self.assertRaises(OracleFailure): author.validate_parent(third, pc, held, {}, player, b"", *paths)


if __name__ == "__main__":
    unittest.main()
