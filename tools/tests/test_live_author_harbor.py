"""Synthetic unit bytes and fake drivers only; never launch an emulator."""
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
from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools/coop"))
import live_author_harbor as author
from live_harness_oracles import OracleFailure


class HarborTests(unittest.TestCase):
    def setUp(self):
        self.data = b"synthetic-unit-save-never-uploaded"
        self.save = SimpleNamespace(generation=1, lineage=b"synthetic-lineage")
        self.fields = {0x0200: b"\xbb\xff" + bytes(6), 0x0205: bytes.fromhex("25917e69"),
                       0x0102: struct.pack("<I", 3000),
                       0x0100: struct.pack("<hh", 9, 12) + bytes((13, 10, 0, 0)) + bytes(556),
                       0x0101: bytes(4) + (bytes(85) + b"\xff" + bytes(14)) * 6,
                       0x0301: bytes(33600)}

    @contextmanager
    def oracle(self):
        with mock.patch.object(author, "read_flash_bytes", return_value=self.save), \
             mock.patch.object(author, "logical_field", side_effect=lambda save, descriptor, field: self.fields[field]):
            yield

    def test_semantic_generation_one_empty_rom_state_and_b_identity(self):
        with self.oracle():
            actual = author.validate_harbor(self.data, b"descriptor", "A", Path("unit.sav"))
            self.assertEqual((actual["x"], actual["y"], actual["warp_id"]), (9, 12, 0))
            self.assertEqual(actual["save_sha256"], hashlib.sha256(self.data).hexdigest())
            self.fields[0x0200] = b"\xbc\xff" + bytes(6)
            self.fields[0x0102] = struct.pack("<I", 999999)
            self.assertEqual(author.validate_harbor(self.data, b"descriptor", "B", Path("unit.sav"))["money"], 999999)

    def test_wrong_generation_identity_money_location_and_population_rejected(self):
        baseline = copy.deepcopy(self.fields)
        occupied = bytearray(33600)
        occupied[19] = 2  # Occupied species-zero contradicts box ABI.
        cases = [(0x0200, b"\xbc\xff" + bytes(6)), (0x0205, bytes(3)),
                 (0x0102, struct.pack("<I", 999999)),
                 (0x0100, baseline[0x0100][:6] + b"\xff" + baseline[0x0100][7:]),
                 (0x0101, bytes((1,)) + bytes(603)), (0x0301, bytes(occupied))]
        with self.oracle():
            for generation in (0, 2, 7):
                self.save.generation = generation
                with self.assertRaises(OracleFailure):
                    author.validate_harbor(self.data, b"d", "A", Path("unit.sav"))
            self.save.generation = 1
            for field, value in cases:
                with self.subTest(field=field):
                    self.fields = dict(baseline)
                    self.fields[field] = value
                    with self.assertRaises(OracleFailure):
                        author.validate_harbor(self.data, b"d", "A", Path("unit.sav"))

    def test_immutable_retention_validates_before_write_and_refuses_overwrite(self):
        with tempfile.TemporaryDirectory() as folder, self.oracle():
            target = Path(folder) / "harbor.sav"
            self.save.generation = 2
            with self.assertRaises(OracleFailure):
                author.retain_harbor(self.data, target, b"d", "A")
            self.assertFalse(target.exists())
            self.save.generation = 1
            author.retain_harbor(self.data, target, b"d", "A")
            self.assertEqual(target.read_bytes(), self.data)
            with self.assertRaises(FileExistsError):
                author.retain_harbor(self.data, target, b"d", "A")

    def test_controls_b_identity_before_first_save_and_budget(self):
        a, b = author.controls("A"), author.controls("B")
        self.assertEqual(dict(a)["name-confirm"][0], (1, 8, 20))
        self.assertEqual(dict(b)["name-confirm"][:2], [(16, 8, 20), (1, 8, 20)])
        labels = [label for label, _ in b]
        self.assertLess(labels.index("b-new-trainer"), labels.index("fresh-first-save"))
        self.assertLess(labels.index("b-max-money"), labels.index("fresh-first-save"))
        self.assertEqual(b[-2:], (("fresh-save-ready", [(0, 1, 300)]), ("fresh-first-save", [(1, 8, 600)])))
        for itinerary in (a, b):
            actions = [action for _, group in itinerary for action in group]
            self.assertLess(len(actions) + len(author.BOOT), 1000)
            self.assertLess(sum(hold + wait for _, hold, wait in actions)
                            + sum(hold + wait for _, _, hold, wait in author.BOOT)
                            + author.INTRO_LIMIT * 308 + 1202, 72000)
        with self.assertRaises(OracleFailure):
            author.controls("AA")

    def test_preflight_rejection_has_no_process_or_output_effects(self):
        with mock.patch.object(author.harness, "preflight", side_effect=OracleFailure("signed rejection")), \
             mock.patch.object(author, "owned_scripted_emulator") as launch, tempfile.TemporaryDirectory() as folder:
            output = Path(folder) / "unused"
            with self.assertRaises(OracleFailure):
                author.author_player({}, "A", output, Path("config"))
            launch.assert_not_called()
            self.assertFalse(output.exists())

    def test_orchestration_clean_author_cold_exact_cache_and_failure_cleanup(self):
        for failure in (None, "input", "gate", "closure", "cold", "source"):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as folder, self.oracle():
                root = Path(folder)
                release = root / "release"
                (release / "runtime").mkdir(parents=True)
                (release / "runtime/mgba.exe").write_bytes(b"unit-exe")
                rom = release / "game.gba"
                rom.write_bytes(b"unit-rom")
                (release / "release_catalog.json").write_text(json.dumps({"worlds": [{"world_id": 1,
                    "rom_path": "game.gba", "rom_sha256": hashlib.sha256(b"unit-rom").hexdigest()}]}))
                (release / "server-build-catalog.json").write_text(json.dumps({"shared_player_descriptor_hex": "abcd"}))
                config = root / "config.ini"
                config.write_bytes(b"unit-config")
                plan = {"release_dir": str(release), **{key: "unit" for key in (
                    "release_id", "envelope_sha256", "catalog_sha256", "server_catalog_sha256")}}
                opened, closed, buttons = [], [], []
                test = self

                @contextmanager
                def driver(exe, stage, env):
                    opened.append(stage.name)
                    test.assertEqual((stage / "game.sav").exists(), stage.name == "cold")
                    class Script:
                        def act(self, label, mask, hold, wait):
                            buttons.append(label)
                            if failure == "input" and label == "new-game-intro-0":
                                raise OracleFailure("input failure")
                            image = stage / (label + ".png")
                            canvas = Image.new("RGB", (240, 160), "magenta")
                            if label == "intro-gender-probe":
                                canvas.paste(author.load_templates()["gender"], author.TEMPLATES["gender"][0][:2])
                            if label == "intro-name-ready":
                                for key, template in author.load_templates().items():
                                    if key.startswith("name-"):
                                        canvas.paste(template, author.TEMPLATES[key][0][:2])
                            canvas.save(image)
                            if label == "fresh-first-save":
                                (stage / "game.sav").write_bytes(test.data)
                            if label == "cold-harbor":
                                if failure == "cold":
                                    (stage / "game.sav").write_bytes(b"changed")
                                if failure == "source":
                                    rom.write_bytes(b"changed")
                            return image
                    try:
                        yield Script()
                    finally:
                        closed.append(stage.name)
                        if failure == "closure" and stage.name == "author":
                            (stage / "game.sav").write_bytes(b"changed-on-close")

                def gate(act, templates):
                    if failure == "gate":
                        raise OracleFailure("checkpoint failure")
                    gender = act("intro-gender-probe", 0, 1, 300)
                    name = act("intro-name-ready", 0, 1, 300)
                    return {"gender": gender.name, "name": name.name}

                with mock.patch.object(author.harness, "preflight"), mock.patch.object(author.harness, "check_c_space"), \
                     mock.patch.object(author, "config_check"), mock.patch.object(author, "owned_scripted_emulator", side_effect=driver), \
                     mock.patch.object(author, "advance_intro", side_effect=gate), \
                     mock.patch.dict(author.os.environ, {"SystemDrive": "Q:"}):
                    output = root / "output"
                    if failure:
                        with self.assertRaises((OracleFailure, author.harness.HarnessFailure)):
                            author.author_player(plan, "A", output, config)
                        self.assertEqual(opened, closed)
                        self.assertFalse(list(output.glob("*/receipt.json")))
                        if failure == "gate":
                            self.assertEqual(opened, ["author"])
                            self.assertFalse(any(label.startswith("name-confirm") for label in buttons))
                            self.assertNotIn("fresh-first-save", buttons)
                        if failure != "source":
                            self.assertEqual(rom.read_bytes(), b"unit-rom")
                        continue
                    receipt = author.author_player(plan, "A", output, config)
                    self.assertEqual(opened, ["author", "cold"])
                    self.assertEqual(opened, closed)
                    self.assertEqual(author.author_player(plan, "A", output, config), receipt)
                    self.assertEqual(opened, ["author", "cold"])
                    cache = output / receipt["cache_key"]
                    for relative in ("harbor.sav", "cold/game.sav", "author/fresh-first-save.png"):
                        path = cache / relative
                        original = path.read_bytes()
                        path.write_bytes(b"tampered")
                        with self.assertRaises(OracleFailure):
                            author.author_player(plan, "A", output, config)
                        path.write_bytes(original)
                    (cache / "receipt.json").unlink()
                    with self.assertRaises(FileNotFoundError):
                        author.author_player(plan, "A", output, config)
                    self.assertEqual(opened, ["author", "cold"])

    def test_intro_gates_match_actual_rois_and_stop_before_naming_on_failure(self):
        templates = author.load_templates()
        for failure in (None, "gender", "name"):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as folder:
                calls = []
                def act(label, mask, hold, wait):
                    calls.append((label, mask, hold, wait))
                    image = Image.new("RGB", (240, 160), "magenta")
                    if label == "intro-advance-01" and failure != "gender":
                        image.paste(templates["gender"], author.TEMPLATES["gender"][0][:2])
                    if label == "intro-name-ready" and failure != "name":
                        for key in ("name-field", "name-keyboard"):
                            image.paste(templates[key], author.TEMPLATES[key][0][:2])
                    target = Path(folder) / (label + ".png")
                    image.save(target)
                    return target
                if failure:
                    with self.assertRaises(OracleFailure):
                        author.advance_intro(act, templates)
                    labels = [call[0] for call in calls]
                    self.assertNotIn("name-confirm", labels)
                    self.assertNotIn("fresh-first-save", labels)
                    if failure == "gender":
                        self.assertEqual(len(calls), author.INTRO_LIMIT + 1)
                        self.assertNotIn("intro-boy-selected", labels)
                else:
                    author.advance_intro(act, templates)
                    self.assertEqual([call[0] for call in calls], ["intro-gender-probe",
                        "intro-advance-00", "intro-advance-01", "intro-boy-selected", "intro-name-opening", "intro-name-ready"])
                    self.assertEqual(calls[-1], ("intro-name-ready", 0, 1, 300))

    def test_pinned_template_tamper_fails_before_any_launch(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            fixtures = root / "fixtures"
            fixtures.mkdir()
            for key in author.TEMPLATES:
                source = Path(author.__file__).with_name("fixtures") / f"harbor-{key}.png"
                (fixtures / source.name).write_bytes(source.read_bytes())
            (fixtures / "harbor-gender.png").write_bytes(b"stale template")
            with mock.patch.object(author, "__file__", str(root / "driver.py")), \
                 mock.patch.object(author, "owned_scripted_emulator") as launch:
                with self.assertRaises(author.harness.HarnessFailure):
                    author.load_templates()
                launch.assert_not_called()

    def test_name_gate_allows_animated_arrow_and_cursor_tint_but_keeps_blank_a_geometry(self):
        templates = author.load_templates()
        with tempfile.TemporaryDirectory() as folder:
            canvas = Image.new("RGB", (240, 160), "magenta")
            for key in ("name-field", "name-keyboard"):
                canvas.paste(templates[key], author.TEMPLATES[key][0][:2])
            path = Path(folder) / "name.png"
            outline = [(x, y) for y in range(80, 96) for x in range(32, 44)
                       if canvas.getpixel((x, y)) == (252, 160, 173)]
            for point in outline:
                canvas.putpixel(point, (252, 255, 255))
            canvas.save(path)
            self.assertTrue(author.matches_intro(path, templates, ("name-field", "name-keyboard")))
            valid = canvas.copy()
            canvas.putpixel((96, 54), (99, 99, 99))  # Typed first name character.
            canvas.save(path)
            self.assertFalse(author.matches_intro(path, templates, ("name-field", "name-keyboard")))
            canvas = valid.copy()
            for x, y in outline:
                canvas.putpixel((x, y), (123, 173, 198))
                canvas.putpixel((x + 12, y), (252, 255, 255))  # Selected B.
            canvas.save(path)
            self.assertFalse(author.matches_intro(path, templates, ("name-field", "name-keyboard")))


# Independent drawing of the naming_screen.c sprites as they appear in the
# blank name field (dark 57, shade 115, blank 255), in ROI coordinates.
ARROW = ("###", "####", "#####", "######", "#####+", "####+", "###+", "+++")
UNDERSCORE = ("######", "++++++")
SHADES = {"#": (57, 57, 57), "+": (115, 115, 115)}
LIVE_PHASE = Path(__file__).with_name("fixtures") / "harbor-name-field-live-x2-0-y2-1.png"
# ROI crop of the merged-ROM intro-name-ready.png (2026-10-03 v8 run 8cd9725a,
# author/intro-name-ready.png) that failed the old eight-column mask.
LIVE_PHASE_SHA256 = "622173cf3c130ea37ea9d61df5ab696ed09290d73aa69d0f88db6e024b9d5b42"


class NameFieldPhaseTests(unittest.TestCase):
    def setUp(self):
        self.templates = author.load_templates()
        self.folder = tempfile.TemporaryDirectory()
        self.addCleanup(self.folder.cleanup)

    def field(self, arrow_x2, underscore_y2, arrow=True, underscore=True):
        field = self.templates["name-field"].copy()
        field.paste((255, 255, 255), (0, 18, 10, 26))
        field.paste((255, 255, 255), (11, 29, 17, 33))
        if arrow:
            for row, line in enumerate(ARROW):
                for col, shade in enumerate(line):
                    x = 3 + arrow_x2 + col
                    if shade in SHADES and 0 <= x < field.size[0]:
                        field.putpixel((x, 18 + row), SHADES[shade])
        if underscore:
            for row, line in enumerate(UNDERSCORE):
                for col, shade in enumerate(line):
                    field.putpixel((11 + col, 28 + underscore_y2 + row), SHADES[shade])
        return field

    def check(self, field, keyboard=True):
        canvas = Image.new("RGB", (240, 160), "magenta")
        canvas.paste(field, author.TEMPLATES["name-field"][0][:2])
        if keyboard:
            canvas.paste(self.templates["name-keyboard"], author.TEMPLATES["name-keyboard"][0][:2])
        path = Path(self.folder.name) / "name.png"
        canvas.save(path)
        return author.matches_intro(path, self.templates, ("name-field", "name-keyboard"))

    def test_independent_drawing_reproduces_pinned_template_phase(self):
        self.assertEqual(self.field(-2, 2).tobytes(), self.templates["name-field"].tobytes())

    def test_every_reviewed_arrow_and_underscore_phase_is_accepted(self):
        for x2 in (0, -4, -2, -1):
            for y2 in (1, 2, 3):
                with self.subTest(x2=x2, y2=y2):
                    self.assertTrue(self.check(self.field(x2, y2)))
        self.assertEqual(sorted(author.name_field_variants(self.templates["name-field"])),
                         sorted((x2, y2) for x2 in (0, -4, -2, -1) for y2 in (1, 2, 3)))

    def test_live_merged_rom_phase_is_accepted(self):
        self.assertEqual(hashlib.sha256(LIVE_PHASE.read_bytes()).hexdigest(), LIVE_PHASE_SHA256)
        with Image.open(LIVE_PHASE) as image:
            live = image.convert("RGB")
        self.assertEqual(live.tobytes(), self.field(0, 1).tobytes())
        self.assertTrue(self.check(live))

    def test_off_phase_missing_or_extra_sprites_are_rejected(self):
        cases = {
            "arrow-x2-1": self.field(1, 2), "arrow-x2-3": self.field(-3, 2),
            "arrow-x2-5": self.field(-5, 2), "underscore-y2-0": self.field(-2, 0),
            "underscore-y2-4": self.field(-2, 4), "no-arrow": self.field(0, 1, arrow=False),
            "no-underscore": self.field(0, 1, underscore=False),
        }
        stray = self.field(0, 3)
        stray.putpixel((1, 25), (57, 57, 57))  # Pixel left behind inside the envelope.
        cases["stray-envelope-pixel"] = stray
        for label, field in cases.items():
            with self.subTest(label):
                self.assertFalse(self.check(field))

    def test_non_blank_name_wrong_screen_and_wrong_selection_are_rejected_in_every_phase(self):
        for x2 in (0, -4, -2, -1):
            for y2 in (1, 2, 3):
                with self.subTest(x2=x2, y2=y2):
                    typed = self.field(x2, y2)
                    for x, y in ((12, 22), (13, 21), (14, 20), (13, 23)):  # Glyph in slot 1.
                        typed.putpixel((x, y), (57, 57, 57))
                    self.assertFalse(self.check(typed))
                    second = self.field(x2, y2)
                    second.paste((255, 255, 255), (11, 28 + y2, 17, 30 + y2))
                    for row, line in enumerate(UNDERSCORE):  # Cursor on slot 2.
                        for col, shade in enumerate(line):
                            second.putpixel((19 + col, 28 + y2 + row), SHADES[shade])
                    self.assertFalse(self.check(second))
                    self.assertFalse(self.check(Image.new("RGB", (77, 35), "magenta")))
                    self.assertFalse(self.check(self.field(x2, y2), keyboard=False))
                    canvas = Image.new("RGB", (240, 160), "magenta")
                    canvas.paste(self.field(x2, y2), author.TEMPLATES["name-field"][0][:2])
                    keyboard = self.templates["name-keyboard"].copy()
                    for x, y in [(x, y) for y in range(3, 19) for x in range(8, 20)
                                 if keyboard.getpixel((x, y)) == (252, 160, 173)]:
                        keyboard.putpixel((x, y), (123, 173, 198))
                        keyboard.putpixel((x + 12, y), (252, 160, 173))  # Selected B.
                    canvas.paste(keyboard, author.TEMPLATES["name-keyboard"][0][:2])
                    path = Path(self.folder.name) / "selection.png"
                    canvas.save(path)
                    self.assertFalse(author.matches_intro(path, self.templates, ("name-field", "name-keyboard")))

    def test_template_with_content_in_animation_envelope_fails_closed(self):
        tampered = self.templates["name-field"].copy()
        tampered.putpixel((9, 20), (57, 57, 57))
        with self.assertRaises(OracleFailure):
            author.name_field_variants(tampered)
        tampered = self.templates["name-field"].copy()
        tampered.putpixel((14, 32), (57, 57, 57))
        with self.assertRaises(OracleFailure):
            author.name_field_variants(tampered)


if __name__ == "__main__":
    unittest.main()
