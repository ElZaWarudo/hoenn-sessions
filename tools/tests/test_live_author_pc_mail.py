"""Synthetic oracle bytes and fake owned drivers; never upload or launch."""
from contextlib import ExitStack, contextmanager
import copy
import hashlib
from itertools import permutations
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
import live_author_pc_mail as author
from live_harness_oracles import OracleFailure


class PcMailTests(unittest.TestCase):
    @staticmethod
    def box(species, personality, held):
        result = bytearray(80)
        trainer = 0x1234ABCD
        struct.pack_into("<II", result, 0, personality, trainer)
        result[19] = 2
        words = [0] * 12
        growth = tuple(permutations(range(4)))[personality % 24].index(0) * 3
        words[growth] = species | (held << 16) | (63 << 26)
        struct.pack_into("<H", result, 28, sum(w + (w >> 16) for w in words) & 65535)
        struct.pack_into("<12I", result, 32, *(w ^ personality ^ trainer for w in words))
        return result

    def setUp(self):
        self.source = SimpleNamespace(generation=3, lineage=b"unit", sha256="a" * 64,
                                      path=Path("held/first-mail.sav"))
        self.data = b"synthetic-pc-mail-never-upload"
        self.saved = SimpleNamespace(generation=4, lineage=b"unit", sha256=hashlib.sha256(self.data).hexdigest())
        self.recipe = {"abi": "hoenn-box80-v1", "party_species": list(range(25, 31)),
                       "party_level": 5, "pc_species": [31], "bag_items": [[2, 3]]}
        self.before = {0x0200: b"\xbb\xff" + bytes(6), 0x0205: struct.pack("<I", 0x1234ABCD),
                       0x0102: struct.pack("<I", 3000), 0x0100: bytes((9, 0, 12, 0, 13, 10, 0, 0)),
                       0x0106: struct.pack("<4H", 2, 3, 200, 2)}
        party = bytearray(604)
        party[0] = 6
        for slot in range(6):
            start = 4 + slot * 100
            party[start:start + 80] = self.box(25 + slot, 17 + slot, 200 if slot == 0 else 0)
            party[start + 84] = 5
            party[start + 85] = 0 if slot == 0 else 255
        self.before[0x0101] = bytes(party)
        pc = bytearray(33600)
        pc[:80] = self.box(31, 23, 0)
        self.before[0x0301] = bytes(pc)
        mail = bytearray(576)
        for slot in range(16):
            struct.pack_into("<9H", mail, slot * 36, *([65535] * 9))
        sender = b"\xbb" + bytes(5) + b"\xff\xff"
        letter = (struct.pack("<9H", *author.held_mail.WORDS) + sender
                  + self.before[0x0205] + struct.pack("<HH", 25, 200) + b"\x91\x92")
        mail[:36] = letter
        self.before[0x010B] = bytes(mail)
        self.after = copy.deepcopy(self.before)
        party[4:84] = self.box(25, 17, 0)
        party[89] = 255
        party[104:184] = self.box(26, 18, 200)
        party[189] = 0
        self.after[0x0101] = bytes(party)
        mail[216:252] = letter
        struct.pack_into("<H", mail, 30, 26)
        self.after[0x010B] = bytes(mail)
        self.after[0x0106] = struct.pack("<4H", 2, 3, 200, 1)

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
            return author.validate_pc_mail(self.data, self.source, b"", self.recipe, Path("unit"))

    def test_distinct_species_identity_and_full_witness(self):
        result = self.check()
        self.assertEqual([c["species"] for c in result["carriers"]], [25, 26])
        self.assertEqual(result["held_party_index"], 1)
        self.assertEqual(result["pc_message"]["species"], 25)
        self.assertEqual(result["held_message"]["species"], 26)
        witness = result["shared_witnesses"][0]
        self.assertEqual((witness["size"], witness["sha256"]), (604, hashlib.sha256(self.after[0x0101]).hexdigest()))

    def test_generation_and_lineage_reject(self):
        for field, value in (("generation", 3), ("lineage", b"wrong")):
            with self.subTest(field=field):
                old = getattr(self.saved, field)
                setattr(self.saved, field, value)
                with self.assertRaises(OracleFailure): self.check()
                setattr(self.saved, field, old)

    def test_attachment_and_other_party_reject(self):
        valid = self.after[0x0101]
        for offset, value in ((89, 0), (189, 255), (205, 42)):
            with self.subTest(offset=offset):
                changed = bytearray(valid)
                changed[offset] = value
                self.after[0x0101] = bytes(changed)
                with self.assertRaises(OracleFailure): self.check()
        self.after[0x0101] = valid

    def test_held_identity_checksum_and_unrelated_carrier_reject(self):
        valid = self.after[0x0101]
        for box in (self.box(26, 18, 0), self.box(26, 19, 200)):
            changed = bytearray(valid)
            changed[104:184] = box
            self.after[0x0101] = bytes(changed)
            with self.assertRaises(OracleFailure): self.check()
        for offset in (132, 190):
            changed = bytearray(valid)
            changed[offset] ^= 1
            self.after[0x0101] = bytes(changed)
            with self.assertRaises(OracleFailure): self.check()
        self.after[0x0101] = valid

    def test_exact_pc_letter_padding_sender_and_slot_swap_reject(self):
        valid = self.after[0x010B]
        for offset in (216, 234, 242, 246, 250, 288):
            changed = bytearray(valid)
            changed[offset] ^= 1
            self.after[0x010B] = bytes(changed)
            with self.assertRaises(OracleFailure): self.check()
        changed = bytearray(valid)
        changed[:36], changed[216:252] = valid[216:252], valid[:36]
        self.after[0x010B] = bytes(changed)
        with self.assertRaises(OracleFailure): self.check()
        self.after[0x010B] = valid

    def test_bag_pc_location_money_reject(self):
        for fid in (0x0106, 0x0301, 0x0100, 0x0102):
            old = self.after[fid]
            self.after[fid] = bytes([old[0] ^ 1]) + old[1:]
            with self.assertRaises(OracleFailure): self.check()
            self.after[fid] = old

    def test_parent_unoccupied_mail_and_first_attachment_required(self):
        old = self.before[0x010B]
        changed = bytearray(old)
        struct.pack_into("<H", changed, 6 * 36 + 32, 200)
        self.before[0x010B] = bytes(changed)
        with self.assertRaises(OracleFailure): self.check()
        self.before[0x010B] = old

    def test_source_backed_control_boundaries(self):
        controls = dict(author.CONTROLS)
        self.assertEqual(controls["mail-editor"], [(128, 8, 20), (1, 8, 60)])
        self.assertEqual(controls["pc-mail-sent-ready"], [(0, 1, 600)])
        self.assertEqual(controls["pc-mail-party-return"], [(2, 8, 180)])
        self.assertEqual(controls["pc-mail-start-return"], [(2, 8, 180)])
        self.assertEqual([a[0] for a in controls["save-confirm"]], [128] * 4 + [1])
        self.assertLess(sum(h + w for _, actions in author.CONTROLS for _, h, w in actions), 72000)

    def make_cache(self, root):
        inputs = {"unit": "cache"}
        ancestry = [{"path": "gen1", "sha256": "1" * 64}, {"path": "gen2", "sha256": "2" * 64},
                    {"path": "gen3", "sha256": self.source.sha256}]
        images = {}
        for stage, label in (("author", "pc-mail-saved"), ("author", "pc-mail-sent-ready"), ("cold", "cold-party")):
            folder = root / stage
            folder.mkdir(exist_ok=True)
            (folder / "game.sav").write_bytes(self.data)
            image = b"\x89PNG\r\n\x1a\nunit"
            (folder / (label + ".png")).write_bytes(image)
            images[f"{stage}/{label}.png"] = hashlib.sha256(image).hexdigest()
        (root / "pc-mail.sav").write_bytes(self.data)
        receipt = {"inputs": inputs, "cache_key": author.cache_key(inputs), "validated": self.check(),
                   "seed_lineage": ancestry + [{"path": str((root / "pc-mail.sav").resolve()), "sha256": self.saved.sha256}],
                   "cold_save_sha256": self.saved.sha256, "screenshots": images}
        (root / "receipt.json").write_text(json.dumps(receipt))
        return inputs, ancestry, receipt

    def test_cache_reuse_and_ancestry_tampering_reject(self):
        with tempfile.TemporaryDirectory() as temp, self.oracle(), mock.patch.object(author, "owned_scripted_emulator") as launch:
            root = Path(temp)
            inputs, ancestry, receipt = self.make_cache(root)
            self.assertEqual(author.cached_receipt(root, inputs, self.source, b"", self.recipe, ancestry), receipt)
            launch.assert_not_called()
            for chain in (receipt["seed_lineage"][:-1], list(reversed(receipt["seed_lineage"])),
                          [dict(receipt["seed_lineage"][0], path="substituted")] + receipt["seed_lineage"][1:]):
                changed = dict(receipt, seed_lineage=chain)
                (root / "receipt.json").write_text(json.dumps(changed))
                with self.assertRaises(OracleFailure):
                    author.cached_receipt(root, inputs, self.source, b"", self.recipe, ancestry)

    def test_cache_stale_inputs_runtime_and_screenshot_reject(self):
        with tempfile.TemporaryDirectory() as temp, self.oracle():
            root = Path(temp)
            inputs, ancestry, receipt = self.make_cache(root)
            with self.assertRaises(OracleFailure):
                author.cached_receipt(root, {"unit": "changed"}, self.source, b"", self.recipe, ancestry)
            for path in (root / "cold/game.sav", root / "author/pc-mail-saved.png"):
                old = path.read_bytes()
                path.write_bytes(b"tampered")
                with self.assertRaises(OracleFailure):
                    author.cached_receipt(root, inputs, self.source, b"", self.recipe, ancestry)
                path.write_bytes(old)
            (root / "receipt.json").unlink()
            with self.assertRaises(FileNotFoundError):
                author.cached_receipt(root, inputs, self.source, b"", self.recipe, ancestry)

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
        held = base / "held"
        held.mkdir()
        (held / "receipt.json").write_bytes(b"unit-parent-receipt")
        self.source.path = held / "first-mail.sav"
        parent = {"cache_key": "unit-parent", "inputs": {"signed_fixture": {"unit": True}}}
        ancestry = [{"path": "one", "sha256": "1" * 64}, {"path": "two", "sha256": "2" * 64},
                    {"path": str(self.source.path.resolve()), "sha256": self.source.sha256}]
        player = {"name": "a", "population_recipe": self.recipe, "authoring_menu_profile": "hoenn-debug-v1"}
        plan = {"release_dir": str(release), "players": [player]}
        launches, closed = [], []
        @contextmanager
        def driver(exe, folder, env):
            launches.append(folder.name)
            class Driver:
                def act(inner, label, mask, *, hold, wait):
                    if failure == "input" and label == "pc-mail-take":
                        raise OracleFailure("unit input failure")
                    if label == "pc-mail-saved":
                        (folder / "game.sav").write_bytes(self.data if failure != "semantic" else b"unit-broken")
                    if failure == "cold" and label == "cold-party":
                        (folder / "game.sav").write_bytes(b"unit-cold-changed")
                    image = folder / (label + ".png")
                    image.write_bytes(b"\x89PNG\r\n\x1a\nunit")
                    return image
            try:
                yield Driver()
            finally:
                closed.append(folder.name)
                if failure == "close" and folder.name == "author":
                    (folder / "game.sav").write_bytes(b"unit-close-changed")
        with ExitStack() as stack:
            stack.enter_context(self.oracle())
            for name in ("preflight", "check_c_space", "require_hash"):
                stack.enter_context(mock.patch.object(author.harness, name))
            stack.enter_context(mock.patch.object(author, "config_check"))
            stack.enter_context(mock.patch.dict("os.environ", {"SystemDrive": "Q:"}))
            prerequisite = stack.enter_context(mock.patch.object(author, "validate_parent",
                return_value=(b"unit-parent-bytes", self.source, parent, ancestry)))
            stack.enter_context(mock.patch.object(author, "owned_scripted_emulator", side_effect=driver))
            if failure == "semantic":
                stack.enter_context(mock.patch.object(author, "validate_pc_mail", side_effect=OracleFailure("unit semantic failure")))
            yield plan, held, base / "output", config, launches, closed, prerequisite

    def test_owned_author_cold_cache_zero_relaunch(self):
        with tempfile.TemporaryDirectory() as temp, self.fake_author_environment(temp) as args:
            plan, held, output, config, launches, closed, prerequisite = args
            first = author.author_player(plan, "a", held, output, config)
            second = author.author_player(plan, "a", held, output, config)
            self.assertEqual(first, second)
            self.assertEqual(launches, ["author", "cold"])
            self.assertEqual(closed, launches)
            self.assertEqual(len(first["seed_lineage"]), 4)
            self.assertEqual(prerequisite.call_count, 3)

    def test_failure_boundary_cleanup_and_no_later_launch(self):
        for failure in ("input", "semantic", "close", "cold"):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as temp:
                with self.fake_author_environment(temp, failure=failure) as args:
                    plan, held, output, config, launches, closed, _ = args
                    with self.assertRaises(OracleFailure):
                        author.author_player(plan, "a", held, output, config)
                    self.assertEqual(launches, ["author", "cold"] if failure == "cold" else ["author"])
                    self.assertEqual(closed, launches)
                    self.assertFalse(list(output.glob("*/receipt.json")))

    def test_parent_rejection_zero_launch_and_no_output(self):
        with tempfile.TemporaryDirectory() as temp, self.fake_author_environment(temp) as args:
            plan, held, output, config, launches, _, prerequisite = args
            prerequisite.side_effect = OracleFailure("dirty/malformed parent")
            with self.assertRaises(OracleFailure):
                author.author_player(plan, "a", held, output, config)
            self.assertFalse(output.exists())
            self.assertEqual(launches, [])

    def test_actual_parent_admission_current_bindings_generation_and_dirty_runtime(self):
        with tempfile.TemporaryDirectory() as temp, ExitStack() as stack:
            root = Path(temp)
            (root / "author").mkdir()
            (root / "author/game.sav").write_bytes(b"held-unit")
            rom, exe, config = root / "main.gba", root / "mgba.exe", root / "config.ini"
            for path in (rom, exe, config): path.write_bytes(path.name.encode())
            base = SimpleNamespace(generation=2, lineage=b"unit", sha256="b" * 64, path=root / "population.sav")
            first = SimpleNamespace(generation=1, lineage=b"unit", sha256="c" * 64, path=root / "harbor.sav")
            self.source.path = root / "first-mail.sav"
            player = {"source_save": str(base.path), "source_sha256": base.sha256,
                      "population_recipe": self.recipe, "authoring_menu_profile": "hoenn-debug-v1",
                      "seed_lineage": [{"path": str(s.path), "sha256": s.sha256} for s in (first, base)]}
            plan = {k: k for k in ("release_id", "envelope_sha256", "catalog_sha256", "server_catalog_sha256")}
            inputs = {"seed_sha256": base.sha256, "recipe": self.recipe, "menu_profile": "hoenn-debug-v1",
                      "signed_fixture": plan.copy(), "descriptor_sha256": hashlib.sha256(b"").hexdigest(),
                      "dependencies": {name: author.harness.digest(Path(author.__file__).with_name(name)) for name in author.PARENT_DEPS},
                      "rom_sha256": author.harness.digest(rom), "emulator_sha256": author.harness.digest(exe),
                      "config_sha256": author.harness.digest(config)}
            (root / "inputs.json").write_text(json.dumps(inputs))
            def seed(path, expected):
                parsed = {base.path: base, first.path: first, self.source.path: self.source}[path]
                if parsed.sha256 != expected: raise OracleFailure("unit pin mismatch")
                return b"held-unit", parsed
            stack.enter_context(mock.patch.object(author, "seed_bytes", side_effect=seed))
            stack.enter_context(mock.patch.object(author, "check_population"))
            location = stack.enter_context(mock.patch.object(author, "logical_field", return_value=self.before[0x0100]))
            stack.enter_context(mock.patch.object(author.held_mail, "cached_receipt", return_value={"first_mail": {"save_sha256": self.source.sha256}}))
            launch = stack.enter_context(mock.patch.object(author, "owned_scripted_emulator"))
            result = author.validate_parent(root, plan, player, b"", rom, exe, config)
            self.assertEqual(len(result[3]), 3)
            for key in ("signed_fixture", "dependencies", "seed_sha256"):
                changed = dict(inputs, **{key: "stale"})
                (root / "inputs.json").write_text(json.dumps(changed))
                with self.assertRaises(OracleFailure): author.validate_parent(root, plan, player, b"", rom, exe, config)
            (root / "inputs.json").write_text(json.dumps(inputs))
            for obj, bad in ((base, 1), (self.source, 2)):
                original = obj.generation
                obj.generation = bad
                with self.assertRaises(OracleFailure): author.validate_parent(root, plan, player, b"", rom, exe, config)
                obj.generation = original
            location.return_value = bytes(8)
            with self.assertRaises(OracleFailure): author.validate_parent(root, plan, player, b"", rom, exe, config)
            location.return_value = self.before[0x0100]
            (root / "author/game.sav").write_bytes(b"dirty")
            with self.assertRaises(OracleFailure): author.validate_parent(root, plan, player, b"", rom, exe, config)
            launch.assert_not_called()


if __name__ == "__main__":
    unittest.main()
