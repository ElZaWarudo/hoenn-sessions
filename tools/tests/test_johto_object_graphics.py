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
        pointers = (ROOT / "src/data/object_events/object_event_graphics_info_pointers.h").read_text(encoding="utf-8") + (ROOT / "src/data/object_events/johto_pointers.inc").read_text(encoding="utf-8")
        info = (ROOT / "src/data/object_events/johto_info.h").read_text(encoding="utf-8")
        self.assertEqual(constants.count("OBJ_EVENT_GFX_JOHTO_"), 35)
        self.assertEqual(pointers.count("OBJ_EVENT_GFX_JOHTO_"), 35)
        self.assertEqual(info.count("const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_Johto_"), 35)
        self.assertEqual(info.count("static const struct SpriteFrameImage sJohtoPicTable_"), 35)
        self.assertEqual(info.count("sJohtoWhirlpool_"), 9 + 20)  # nine commands plus the 20 table entries
        self.assertIn("sJohtoAnimTable_Whirlpool", info)
        self.assertIn("* binary", (ROOT / "graphics/johto/object_events/.gitattributes").read_text())

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
