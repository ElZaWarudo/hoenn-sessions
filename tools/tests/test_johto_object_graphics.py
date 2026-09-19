import hashlib
import json
import os
import importlib.util
import tempfile
from pathlib import Path
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]
DONOR = Path(os.environ.get("JOHTO_DONOR_ROOT", r"C:/Users/Mayor/Documents/Caribbean/johto-hns"))
SPEC = importlib.util.spec_from_file_location("johto_graphics", ROOT / "tools/johto/import_object_graphics.py")
importer = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(importer)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


class JohtoObjectGraphicsTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.manifest = json.loads((ROOT / "data/johto/object_graphics.json").read_text(encoding="utf-8"))

    def test_pinned_scope_and_raw_asset_hashes(self):
        self.assertEqual(self.manifest["scope"]["types"], 35)
        self.assertEqual(self.manifest["scope"]["palettes"], 27)
        objects = self.manifest["objects"]
        self.assertEqual(len(objects), 35)
        self.assertEqual(len({o["pic_source_path"] for o in objects}), 35)
        self.assertEqual(len({o["palette_source_path"] for o in objects}), 27)
        pics = [a for a in self.manifest["assets"] if a["kind"] == "pic"]
        pals = [a for a in self.manifest["assets"] if a["kind"] == "palette"]
        self.assertEqual(len(pics), 35)
        self.assertEqual(len(pals), 27)
        for asset in self.manifest["assets"]:
            src = DONOR / asset["source_path"]
            dst = ROOT / asset["dest_path"]
            self.assertTrue(src.is_file(), asset["source_path"])
            self.assertTrue(dst.is_file(), asset["dest_path"])
            self.assertEqual(digest(src), asset["sha256"])
            self.assertEqual(digest(dst), asset["sha256"])
            self.assertEqual(dst.read_bytes(), src.read_bytes())

    def test_negative_drift_and_missing_fixtures_are_detectable(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            first, second = root / "first.h", root / "second.bin"
            outputs = {first: b"expected\n", second: b"original"}
            with self.assertRaisesRegex(SystemExit, "missing generated output"):
                importer.check_outputs(root, outputs)
            first.write_bytes(b"expected\r\n")
            second.write_bytes(b"drift")
            with self.assertRaisesRegex(SystemExit, "stale or drifted"):
                importer.check_outputs(root, outputs, write=True)
            self.assertEqual(first.read_bytes(), b"expected\r\n")
            self.assertEqual(second.read_bytes(), b"drift")
            second.write_bytes(b"original")
            importer.check_outputs(root, outputs)

    def test_definition_drift_and_wrong_donor_revision_reject(self):
        with patch.object(importer.subprocess, "check_output", return_value="wrong"):
            with self.assertRaisesRegex(SystemExit, "revision drift"):
                importer.preflight(DONOR)
        with patch.dict(importer.SOURCE_HASHES, {next(iter(importer.SOURCE_HASHES)): "0" * 64}):
            with self.assertRaisesRegex(SystemExit, "definition drift"):
                importer.preflight(DONOR)

    def test_generated_outputs_match_complete_plan(self):
        importer.preflight(DONOR)
        importer.check_outputs(ROOT, importer.build_plan(ROOT, DONOR))
        for record in self.manifest["objects"]:
            self.assertEqual(record["public_id"], record["token"].replace("OBJ_EVENT_GFX_", "OBJ_EVENT_GFX_JOHTO_"))

    def test_sprite_sheet_rules_preserve_contiguous_frames(self):
        # Source dimensions, independently fixed for each distinct frame shape.
        rules = (ROOT / "spritesheet_rules.mk").read_text(encoding="utf-8")
        cases = {
            "people/special/silver": (2, 4),
            "pokemon/wingull_old": (4, 4),
            "misc/whirlpool": (8, 8),
            "misc/cable_car": (8, 8),
            "misc/submarine_shadow": (11, 4),
        }
        for path, (width, height) in cases.items():
            self.assertIn(
                f"graphics/johto/object_events/pics/{path}.4bpp: %.4bpp: %.png\n"
                f"\t$(GFX) $< $@ -mwidth {width} -mheight {height}\n", rules)
        self.assertEqual(rules.count("graphics/johto/object_events/pics/"), 35)
        outputs = importer.build_plan(ROOT, DONOR)
        broken = dict(outputs)
        path = ROOT / "spritesheet_rules.mk"
        broken[path] = outputs[path].replace(b"-mwidth 2 -mheight 4", b"-mwidth 1 -mheight 1", 1)
        with self.assertRaisesRegex(SystemExit, "stale or drifted"):
            importer.check_outputs(ROOT, broken)

    def test_source_defined_runtime_namespaces_and_counts(self):
        constants = (ROOT / "include/constants/event_objects.h").read_text(encoding="utf-8")
        pointers = "".join((ROOT / path).read_text(encoding="utf-8") for path in (
            "src/data/object_events/object_event_graphics_info_pointers.h",
            "src/data/object_events/johto_pointers.inc",
            "src/data/object_events/johto_shared_pointers.inc",
        ))
        info = (ROOT / "src/data/object_events/johto_info.h").read_text(encoding="utf-8")
        self.assertEqual(constants.count("OBJ_EVENT_GFX_JOHTO_"), 84)
        self.assertEqual(constants.count("OBJ_EVENT_GFX_JOHTO_SHARED_"), 49)
        self.assertEqual(pointers.count("OBJ_EVENT_GFX_JOHTO_"), 84)
        self.assertEqual(pointers.count("OBJ_EVENT_GFX_JOHTO_SHARED_"), 49)
        self.assertEqual(info.count("const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_Johto_"), 35)
        self.assertEqual(info.count("static const struct SpriteFrameImage sJohtoPicTable_"), 35)
        self.assertEqual(info.count("sJohtoWhirlpool_"), 9 + 20)  # nine commands plus the 20 table entries
        self.assertIn("sJohtoAnimTable_Whirlpool", info)
        self.assertIn("* binary", (ROOT / "graphics/johto/object_events/.gitattributes").read_text())

    def test_base_integration_preserves_shared_and_berry_blocks(self):
        paths = [
            "include/constants/event_objects.h",
            "src/event_object_movement.c",
            "src/data/object_events/object_event_graphics_info_pointers.h",
            "spritesheet_rules.mk",
        ]
        originals = {path: (ROOT / path).read_text(encoding="utf-8") for path in paths}
        outputs = importer.integration_outputs(ROOT)

        constants = outputs[ROOT / paths[0]].decode()
        self.assertEqual(constants.count("OBJ_EVENT_GFX_JOHTO_SHARED_"), 49)
        self.assertLess(constants.index("/* JOHTO_OBJECT_GRAPHICS_IDS */"), constants.index("/* JOHTO_SHARED_OBJECT_GRAPHICS_IDS */"))
        self.assertLess(constants.index("/* JOHTO_SHARED_OBJECT_GRAPHICS_IDS */"), constants.index("NUM_OBJ_EVENT_GFX"))
        for start, end in (
            ("/* JOHTO_SHARED_OBJECT_GRAPHICS_IDS */", "    NUM_OBJ_EVENT_GFX,"),
            ("/* JOHTO_SHARED_OBJECT_GRAPHICS_PALETTE_TAGS */", "#define OBJ_EVENT_PAL_TAG_NONE"),
        ):
            original = originals[paths[0]]
            self.assertEqual(
                constants[constants.index(start):constants.index(end)],
                original[original.index(start):original.index(end)],
            )

        movement = outputs[ROOT / paths[1]].decode()
        self.assertIn('#include "data/object_events/johto_shared_assets.h"', movement)
        self.assertIn('#include "data/object_events/johto_shared_info.h"', movement)
        self.assertIn('#include "data/object_events/johto_shared_palettes.inc"', movement)

        pointers = outputs[ROOT / paths[2]].decode()
        self.assertIn('#include "johto_shared_declarations.h"', pointers)
        self.assertIn('#include "johto_shared_pointers.inc"', pointers)

        rules = outputs[ROOT / paths[3]].decode()
        self.assertIn("# BEGIN JOHTO BERRY GRAPHICS RULES", rules)
        self.assertIn("# BEGIN JOHTO SHARED OBJECT FRAME RULES", rules)
        self.assertLess(rules.index("# BEGIN JOHTO OBJECT FRAME RULES"), rules.index("# BEGIN JOHTO BERRY GRAPHICS RULES"))
        self.assertLess(rules.index("# BEGIN JOHTO BERRY GRAPHICS RULES"), rules.index("# BEGIN JOHTO SHARED OBJECT FRAME RULES"))
        original_rules = originals[paths[3]]
        downstream = "# BEGIN JOHTO BERRY GRAPHICS RULES"
        self.assertEqual(rules[rules.index(downstream):], original_rules[original_rules.index(downstream):])

        prefix_markers = {
            paths[0]: "    /* JOHTO_OBJECT_GRAPHICS_IDS */",
            paths[1]: '#include "data/object_events/johto_assets.h"',
            paths[3]: "# BEGIN JOHTO OBJECT FRAME RULES",
        }
        for path, marker in prefix_markers.items():
            original = originals[path]
            generated = outputs[ROOT / path].decode()
            self.assertEqual(generated[:generated.index(marker)], original[:original.index(marker)])

    def test_shared_includes_must_remain_in_their_local_groups(self):
        paths = [
            "include/constants/event_objects.h",
            "src/event_object_movement.c",
            "src/data/object_events/object_event_graphics_info_pointers.h",
            "spritesheet_rules.mk",
        ]
        cases = [
            (
                "src/event_object_movement.c",
                '#include "data/object_events/johto_shared_assets.h"\n',
                '// movement type callbacks\n',
            ),
            (
                "src/event_object_movement.c",
                '#include "data/object_events/johto_shared_info.h"\n',
                '#include "data/object_events/object_event_graphics_info_followers.h"\n',
            ),
            (
                "src/event_object_movement.c",
                '#include "data/object_events/johto_shared_palettes.inc"\n',
                'static const u16 sReflectionPaletteTags_Brendan[] = {\n',
            ),
            (
                "src/data/object_events/object_event_graphics_info_pointers.h",
                '#include "johto_shared_declarations.h"\n',
                'extern const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_BrendanNormal;\n',
            ),
            (
                "src/data/object_events/object_event_graphics_info_pointers.h",
                '#include "johto_shared_pointers.inc"\n',
                'const struct ObjectEventGraphicsInfo *const gMauvilleOldManGraphicsInfoPointers[] = {\n',
            ),
        ]

        for path, shared_include, destination in cases:
            with self.subTest(shared_include=shared_include.strip()):
                with tempfile.TemporaryDirectory() as folder:
                    root = Path(folder)
                    for integration_path in paths:
                        target = root / integration_path
                        target.parent.mkdir(parents=True, exist_ok=True)
                        target.write_bytes((ROOT / integration_path).read_bytes())
                    target = root / path
                    text = target.read_text(encoding="utf-8").replace(shared_include, "", 1)
                    text = text.replace(destination, destination + shared_include, 1)
                    target.write_text(text, encoding="utf-8")
                    with self.assertRaisesRegex(SystemExit, "integration .* drift"):
                        importer.integration_outputs(root)

    def test_deterministic_manifest_order_and_names(self):
        objects = self.manifest["objects"]
        self.assertEqual([o["token"] for o in objects], [
            "OBJ_EVENT_GFX_SILVER", "OBJ_EVENT_GFX_SUPER_NERD", "OBJ_EVENT_GFX_KIMONO_GIRL",
            "OBJ_EVENT_GFX_SLOWPOKE_NO_TAIL", "OBJ_EVENT_GFX_KURT", "OBJ_EVENT_GFX_BATTLE_GIRL",
            "OBJ_EVENT_GFX_SAGE", "OBJ_EVENT_GFX_ATTENDANT", "OBJ_EVENT_GFX_EUSINE",
            "OBJ_EVENT_GFX_ENGINEER", "OBJ_EVENT_GFX_FIREBREATHER", "OBJ_EVENT_GFX_JUGGLER",
            "OBJ_EVENT_GFX_LEGENDARY_SHADOW", "OBJ_EVENT_GFX_WHIRLPOOL", "OBJ_EVENT_GFX_ARCHER",
            "OBJ_EVENT_GFX_SCIENTIST_M", "OBJ_EVENT_GFX_PROF_ELM", "OBJ_EVENT_GFX_SCIENTIST_F",
            "OBJ_EVENT_GFX_NURSE_CHANSEY", "OBJ_EVENT_GFX_FALKNER", "OBJ_EVENT_GFX_BUGSY",
            "OBJ_EVENT_GFX_BURGLAR", "OBJ_EVENT_GFX_WHITNEY", "OBJ_EVENT_GFX_ATTENDANT_M",
            "OBJ_EVENT_GFX_TRAIN_FRONT", "OBJ_EVENT_GFX_PROTON", "OBJ_EVENT_GFX_ARIANA",
            "OBJ_EVENT_GFX_PETREL", "OBJ_EVENT_GFX_MORTY", "OBJ_EVENT_GFX_JASMINE",
            "OBJ_EVENT_GFX_CHUCK", "OBJ_EVENT_GFX_PRYCE", "OBJ_EVENT_GFX_CLAIR",
            "OBJ_EVENT_GFX_JANINE", "OBJ_EVENT_GFX_SHINY_GYARADOS"])
        self.assertEqual([o["palette_tag"] for o in objects if o["token"] == "OBJ_EVENT_GFX_WHIRLPOOL"], ["OBJ_EVENT_PAL_TAG_JOHTO_WHIRLPOOL"])


if __name__ == "__main__":
    unittest.main()
