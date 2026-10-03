"""Synthetic oracle bytes and fake owned drivers, never live save fixtures."""
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
import live_author_third_mail as author
from live_harness_oracles import OracleFailure
from tools.tests import test_live_author_pc_mail as canonical


class ThirdMailTests(unittest.TestCase):
    def setUp(self):
        fixture = canonical.PcMailTests()
        fixture.setUp()
        self.source = SimpleNamespace(generation=4, lineage=b"unit", sha256="a" * 64,
                                      path=Path("pc/pc-mail.sav"))
        self.data = b"synthetic-third-mail-never-uploaded"
        self.saved = SimpleNamespace(generation=5, lineage=b"unit", sha256=hashlib.sha256(self.data).hexdigest())
        self.recipe = fixture.recipe
        self.before = copy.deepcopy(fixture.after)
        self.after = copy.deepcopy(self.before)
        party = bytearray(self.after[0x0101])
        party[204:284] = canonical.PcMailTests.box(27, 19, 200)
        party[289] = 1
        self.after[0x0101] = bytes(party)
        mail = bytearray(self.after[0x010B])
        letter = bytearray(mail[:36])
        struct.pack_into("<H", letter, 30, 27)
        letter[34:36] = mail[70:72]
        mail[36:72] = letter
        self.after[0x010B] = bytes(mail)
        self.after[0x0106] = struct.pack("<4H", 2, 3, 0, 0)

    def oracle(self):
        stack = ExitStack()
        def field(save, descriptor, fid):
            return (self.before if save is self.source else self.after)[fid]
        stack.enter_context(mock.patch.object(author, "read_flash_bytes", return_value=self.saved))
        for module in (author, author.held_mail, sys.modules["live_fixture_population"]):
            stack.enter_context(mock.patch.object(module, "logical_field", side_effect=field))
        return stack

    def check(self):
        with self.oracle():
            return author.validate_third_mail(self.data, self.source, b"", self.recipe, Path("unit"))

    def test_target_identity_and_complete_party_mail_witnesses(self):
        checked = self.check()
        self.assertEqual(checked["carrier"], {"species": 27, "personality": 19, "ot_id": 0x1234ABCD})
        self.assertEqual((checked["generation"], checked["party_index"], checked["mail_index"]), (5, 2, 1))
        self.assertEqual([(w["field_id"], w["size"]) for w in checked["shared_witnesses"]], [(0x0101, 604), (0x010B, 576)])
        self.assertEqual(checked["shared_witnesses"][0]["sha256"], hashlib.sha256(self.after[0x0101]).hexdigest())

    def test_generation_and_lineage_reject(self):
        for obj, field, bad in ((self.source, "generation", 3), (self.saved, "generation", 4),
                                (self.saved, "lineage", b"wrong")):
            old = getattr(obj, field)
            setattr(obj, field, bad)
            with self.assertRaises(OracleFailure): self.check()
            setattr(obj, field, old)

    def test_wrong_carrier_checksum_held_item_and_attachment_reject(self):
        old = self.after[0x0101]
        for box in (canonical.PcMailTests.box(27, 19, 0), canonical.PcMailTests.box(27, 20, 200),
                    canonical.PcMailTests.box(28, 19, 200)):
            changed = bytearray(old)
            changed[204:284] = box
            self.after[0x0101] = bytes(changed)
            with self.assertRaises(OracleFailure): self.check()
        for offset in (232, 289, 300, 10, 190, 400):
            changed = bytearray(old)
            changed[offset] ^= 1
            self.after[0x0101] = bytes(changed)
            with self.assertRaises(OracleFailure): self.check()
        self.after[0x0101] = old

    def test_mail_words_sender_species_padding_and_prior_letters_reject(self):
        old = self.after[0x010B]
        for offset in (36, 54, 62, 66, 68, 70, 0, 216, 300):
            changed = bytearray(old)
            changed[offset] ^= 1
            self.after[0x010B] = bytes(changed)
            with self.assertRaises(OracleFailure): self.check()
        self.after[0x010B] = old

    def test_source_requires_last_mail_first_free_slot_and_harbor(self):
        for fid, offset in ((0x0106, 6), (0x0101, 89), (0x0101, 189), (0x0101, 289),
                            (0x0100, 4), (0x010B, 68)):
            old = self.before[fid]
            changed = bytearray(old)
            changed[offset] ^= 1
            self.before[fid] = bytes(changed)
            with self.assertRaises(OracleFailure): self.check()
            self.before[fid] = old

    def test_bag_pc_money_location_and_sender_identity_reject(self):
        for fid in (0x0106, 0x0301, 0x0102, 0x0100, 0x0200, 0x0205):
            old = self.after[fid]
            self.after[fid] = bytes([old[0] ^ 1]) + old[1:]
            with self.assertRaises(OracleFailure): self.check()
            self.after[fid] = old

    def test_source_control_route_and_printer_save_waits(self):
        controls = dict(author.CONTROLS)
        self.assertEqual(author.CONTROLS[0], ("third-start", [(8, 8, 60)]))
        self.assertEqual(controls["third-mail-editor"], [(128, 8, 20)] * 2 + [(1, 8, 60)])
        self.assertEqual([a[0] for a in controls["third-bag-mail"]], [128, 1])
        self.assertEqual(controls["third-gift-message-ready"], [(0, 1, 600)])
        self.assertEqual(controls["third-bag-after-mail"], [(2, 8, 180)])
        self.assertEqual(controls["third-post-bag-menu"], [(2, 8, 180)])
        self.assertEqual([a[0] for a in controls["third-save-confirm"]], [128] * 4 + [1])
        self.assertLess(sum(h + w for _, actions in author.CONTROLS for _, h, w in actions), 72000)

    def make_cache(self, root):
        inputs = {"unit": "cache"}
        ancestry = [{"path": f"gen{i}", "sha256": str(i) * 64} for i in range(1, 5)]
        images = {}
        for stage, label in (("author", "third-first-mail-saved"), ("author", "third-gift-message-ready"), ("cold", "cold-party")):
            folder = root / stage
            folder.mkdir(exist_ok=True)
            (folder / "game.sav").write_bytes(self.data)
            image = b"\x89PNG\r\n\x1a\nunit"
            (folder / (label + ".png")).write_bytes(image)
            images[f"{stage}/{label}.png"] = hashlib.sha256(image).hexdigest()
        (root / "third-mail.sav").write_bytes(self.data)
        (root / "inputs.json").write_text(json.dumps(inputs))
        receipt = {"inputs": inputs, "cache_key": author.cache_key(inputs), "validated": self.check(),
                   "seed_lineage": ancestry + [{"path": str((root / "third-mail.sav").resolve()), "sha256": self.saved.sha256}],
                   "cold_save_sha256": self.saved.sha256, "screenshots": images}
        (root / "receipt.json").write_text(json.dumps(receipt))
        return inputs, ancestry, receipt

    def test_cache_exact_five_ancestry_entries_and_no_launch(self):
        with tempfile.TemporaryDirectory() as temp, self.oracle(), mock.patch.object(author, "owned_scripted_emulator") as launch:
            root = Path(temp)
            inputs, ancestry, receipt = self.make_cache(root)
            self.assertEqual(author.cached_receipt(root, inputs, self.source, b"", self.recipe, ancestry), receipt)
            launch.assert_not_called()
            for chain in (receipt["seed_lineage"][:-1], list(reversed(receipt["seed_lineage"])),
                          [dict(receipt["seed_lineage"][0], path="substituted")] + receipt["seed_lineage"][1:]):
                (root / "receipt.json").write_text(json.dumps(dict(receipt, seed_lineage=chain)))
                with self.assertRaises(OracleFailure): author.cached_receipt(root, inputs, self.source, b"", self.recipe, ancestry)

    def test_cache_input_runtime_screenshot_and_partial_reject(self):
        with tempfile.TemporaryDirectory() as temp, self.oracle():
            root = Path(temp)
            inputs, ancestry, receipt = self.make_cache(root)
            with self.assertRaises(OracleFailure): author.cached_receipt(root, {"unit": "changed"}, self.source, b"", self.recipe, ancestry)
            for path in (root / "cold/game.sav", root / "author/game.sav", root / "author/third-first-mail-saved.png", root / "inputs.json"):
                old = path.read_bytes()
                path.write_bytes(b'"tampered"')
                with self.assertRaises(OracleFailure): author.cached_receipt(root, inputs, self.source, b"", self.recipe, ancestry)
                path.write_bytes(old)
            (root / "receipt.json").unlink()
            with self.assertRaises(FileNotFoundError): author.cached_receipt(root, inputs, self.source, b"", self.recipe, ancestry)

    @contextmanager
    def fake_author_environment(self, temp, *, failure=None):
        base = Path(temp)
        release = base / "release"
        (release / "runtime").mkdir(parents=True)
        (release / "runtime/mgba.exe").write_bytes(b"unit-exe")
        (release / "main.gba").write_bytes(b"unit-rom")
        (release / "release_catalog.json").write_text(json.dumps({"worlds": [
            {"world_id": 1, "rom_path": "main.gba", "rom_sha256": "unit"}]}))
        (release / "server-build-catalog.json").write_text(json.dumps({"shared_player_descriptor_hex": ""}))
        config = base / "config.ini"
        config.write_bytes(b"unit-config")
        pc, held = base / "pc", base / "held"
        pc.mkdir()
        held.mkdir()
        (pc / "receipt.json").write_bytes(b"unit-pc-receipt")
        self.source.path = pc / "pc-mail.sav"
        parent = {"cache_key": "unit-pc", "inputs": {"signed_fixture": {"unit": True}}}
        ancestry = [{"path": f"gen{i}", "sha256": str(i) * 64} for i in range(1, 4)] + [
            {"path": str(self.source.path.resolve()), "sha256": self.source.sha256}]
        player = {"name": "a", "population_recipe": self.recipe, "authoring_menu_profile": "hoenn-debug-v1"}
        plan = {"release_dir": str(release), "players": [player]}
        launches, closed = [], []
        @contextmanager
        def driver(exe, folder, env):
            launches.append(folder.name)
            class Driver:
                def act(inner, label, mask, *, hold, wait):
                    if failure == "input" and label == "third-mail-editor": raise OracleFailure("unit input failure")
                    if label == "third-first-mail-saved":
                        (folder / "game.sav").write_bytes(self.data if failure != "semantic" else b"unit-broken")
                    if failure == "cold" and label == "cold-party": (folder / "game.sav").write_bytes(b"unit-cold-changed")
                    image = folder / (label + ".png")
                    image.write_bytes(b"\x89PNG\r\n\x1a\nunit")
                    return image
            try:
                yield Driver()
            finally:
                closed.append(folder.name)
                if failure == "close" and folder.name == "author": (folder / "game.sav").write_bytes(b"unit-close-changed")
        with ExitStack() as stack:
            stack.enter_context(self.oracle())
            for name in ("preflight", "check_c_space", "require_hash"):
                stack.enter_context(mock.patch.object(author.harness, name))
            stack.enter_context(mock.patch.object(author, "config_check"))
            stack.enter_context(mock.patch.dict("os.environ", {"SystemDrive": "Q:"}))
            prerequisite = stack.enter_context(mock.patch.object(author, "validate_parent", return_value=(b"unit-parent", self.source, parent, ancestry)))
            stack.enter_context(mock.patch.object(author, "owned_scripted_emulator", side_effect=driver))
            if failure == "semantic": stack.enter_context(mock.patch.object(author, "validate_third_mail", side_effect=OracleFailure("unit semantic")))
            yield plan, pc, held, base / "output", config, launches, closed, prerequisite

    def test_owned_author_cold_cache_reuse_and_cleanup(self):
        with tempfile.TemporaryDirectory() as temp, self.fake_author_environment(temp) as args:
            plan, pc, held, output, config, launches, closed, prerequisite = args
            receipt = author.author_player(plan, "a", pc, held, output, config)
            self.assertEqual(author.author_player(plan, "a", pc, held, output, config), receipt)
            self.assertEqual(launches, ["author", "cold"])
            self.assertEqual(closed, launches)
            self.assertEqual(len(receipt["seed_lineage"]), 5)
            self.assertEqual(prerequisite.call_count, 3)

    def test_fail_first_boundary_cleanup_no_later_launch(self):
        for failure in ("input", "semantic", "close", "cold"):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as temp:
                with self.fake_author_environment(temp, failure=failure) as args:
                    plan, pc, held, output, config, launches, closed, _ = args
                    with self.assertRaises(OracleFailure): author.author_player(plan, "a", pc, held, output, config)
                    self.assertEqual(launches, ["author", "cold"] if failure == "cold" else ["author"])
                    self.assertEqual(closed, launches)
                    self.assertFalse(list(output.glob("*/receipt.json")))

    def test_parent_rejects_before_launch_or_output(self):
        with tempfile.TemporaryDirectory() as temp, self.fake_author_environment(temp) as args:
            plan, pc, held, output, config, launches, _, prerequisite = args
            prerequisite.side_effect = OracleFailure("stale parent")
            with self.assertRaises(OracleFailure): author.author_player(plan, "a", pc, held, output, config)
            self.assertEqual(launches, [])
            self.assertFalse(output.exists())

    def test_reconstruct_pc_parent_current_inputs_before_receipt_validation(self):
        with tempfile.TemporaryDirectory() as temp, ExitStack() as stack:
            root = Path(temp)
            pc, held = root / "pc", root / "held"
            pc.mkdir()
            held.mkdir()
            (held / "receipt.json").write_bytes(b"held-unit")
            held_source = SimpleNamespace(sha256="b" * 64)
            ancestry = [{"path": f"gen{i}", "sha256": str(i) * 64} for i in range(1, 4)]
            parent = {"cache_key": "held-key", "inputs": {"signed_fixture": {"unit": True}}}
            paths = [root / name for name in ("rom", "exe", "config")]
            for path in paths: path.write_bytes(path.name.encode())
            expected = {"player": "a", "source_sha256": held_source.sha256, "recipe": self.recipe,
                "ancestry": ancestry, "parent_cache_key": parent["cache_key"], "parent_receipt_sha256": author.harness.digest(held / "receipt.json"),
                "rom_sha256": author.harness.digest(paths[0]), "emulator_sha256": author.harness.digest(paths[1]),
                "config_sha256": author.harness.digest(paths[2]), "descriptor_sha256": hashlib.sha256(b"").hexdigest(),
                "signed_fixture": parent["inputs"]["signed_fixture"],
                "dependencies": {name: author.harness.digest(Path(author.__file__).with_name(name)) for name in author.PARENT_DEPS}}
            (pc / "inputs.json").write_text(json.dumps(expected))
            self.source.path = pc / "pc-mail.sav"
            receipt = {"validated": {"save_sha256": self.source.sha256}, "seed_lineage": ancestry + [
                {"path": str(self.source.path.resolve()), "sha256": self.source.sha256}]}
            stack.enter_context(mock.patch.object(author.pc_mail, "validate_parent", return_value=(b"held", held_source, parent, ancestry)))
            cached = stack.enter_context(mock.patch.object(author.pc_mail, "cached_receipt", return_value=receipt))
            stack.enter_context(mock.patch.object(author, "seed_bytes", return_value=(b"pc", self.source)))
            stack.enter_context(self.oracle())
            player = {"name": "a", "population_recipe": self.recipe, "authoring_menu_profile": "hoenn-debug-v1"}
            self.assertEqual(len(author.validate_parent(pc, held, {}, player, b"", *paths)[3]), 4)
            for key in ("signed_fixture", "dependencies", "source_sha256", "ancestry", "parent_receipt_sha256", "player"):
                (pc / "inputs.json").write_text(json.dumps(dict(expected, **{key: "forged"})))
                cached.reset_mock()
                with self.assertRaises(OracleFailure): author.validate_parent(pc, held, {}, player, b"", *paths)
                cached.assert_not_called()
            (pc / "inputs.json").write_text(json.dumps(expected))
            receipt["seed_lineage"] = list(reversed(receipt["seed_lineage"]))
            with self.assertRaises(OracleFailure): author.validate_parent(pc, held, {}, player, b"", *paths)


if __name__ == "__main__":
    unittest.main()
