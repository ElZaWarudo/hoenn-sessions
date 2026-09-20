"""Source-bound checks for the additive ordinary Johto object graphics import."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
DONOR = Path(r"C:/Users/Mayor/Documents/Caribbean/johto-hns")
DATA = ROOT / "data/johto/shared_object_graphics.json"
INFO = ROOT / "src/data/object_events/johto_shared_info.h"
IMPORTER = ROOT / "tools/johto/import_shared_object_graphics.py"
CONTRACT_HASH = "sha256:eb587dee56cf5828f754287e8a6c552145ea74055ced228b74a1b1324703c9d0"
TEXT_SUFFIXES = {".h", ".c", ".json", ".inc", ".mk"}


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def normalized(path: Path) -> bytes:
    data = path.read_bytes()
    return data.replace(b"\r\n", b"\n") if path.suffix in TEXT_SUFFIXES else data


def info_block(text: str, symbol: str) -> str:
    match = re.search(
        r"(?m)^const struct ObjectEventGraphicsInfo " + re.escape(symbol) + r"\s*=\s*\{([^}]*)\};",
        text,
    )
    if match is None:
        raise AssertionError(f"missing generated graphics info: {symbol}")
    return match.group(1)


def picture_block(text: str, table: str) -> str:
    match = re.search(
        r"(?ms)^static const struct SpriteFrameImage " + re.escape(table) + r"\[\]\s*=\s*\{(.*?)^\};",
        text,
    )
    if match is None:
        raise AssertionError(f"missing generated picture table: {table}")
    return match.group(1)


class JohtoSharedObjectGraphicsTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.payload = json.loads(DATA.read_text(encoding="utf-8"))
        cls.objects = cls.payload["objects"]
        cls.info_text = INFO.read_text(encoding="utf-8")
        cls.ids_text = (ROOT / "include/constants/event_objects.h").read_text(encoding="utf-8")

    def test_contract_scope_and_raw_assets(self) -> None:
        self.assertEqual(self.payload["contract_hash"], CONTRACT_HASH)
        self.assertEqual(self.payload["scope"], {"objects": 49, "pictures": 49, "palettes": 9})
        self.assertEqual(len(self.objects), 49)
        self.assertEqual(len({item["token"] for item in self.objects}), 49)
        self.assertEqual(len({item["public_id"] for item in self.objects}), 49)

        picture_paths = {item["picture"]["source_path"] for item in self.objects}
        palette_paths = {item["palette"]["source_path"] for item in self.objects}
        self.assertEqual(len(picture_paths), 49)
        self.assertEqual(len(palette_paths), 9)
        for item in self.objects:
            for kind in ("picture", "palette"):
                source = item[kind]
                donor = DONOR / source["source_path"]
                target = ROOT / "graphics/johto/shared/object_events" / Path(source["source_path"]).relative_to("graphics/object_events")
                self.assertTrue(donor.is_file(), donor)
                self.assertTrue(target.is_file(), target)
                self.assertEqual(source["bytes"], donor.stat().st_size)
                self.assertEqual(source["sha256"], digest(donor))
                self.assertEqual(target.read_bytes(), donor.read_bytes())

    def test_complete_frame_tables_and_animation_bindings(self) -> None:
        self.assertEqual(len(re.findall(r"^static const struct SpriteFrameImage sJohtoSharedPicTable_", self.info_text, re.M)), 49)
        for item in self.objects:
            block = picture_block(self.info_text, item["picture_table"])
            frames = re.findall(r"(?m)^\s+(?:overworld_frame|obj_frame_tiles)\s*\(", block)
            self.assertEqual(len(frames), item["frame_count"], item["token"])
            self.assertGreater(item["frame_count"], 0)
            dims = tuple(int(value) for value in item["logical_dimensions"].split("x"))
            width_tiles, height_tiles = dims[0] // 8, dims[1] // 8
            for line in block.splitlines():
                if "overworld_frame(" in line:
                    args = line.split("(", 1)[1].split(")", 1)[0].split(",")
                    self.assertEqual((int(args[1]), int(args[2])), (width_tiles, height_tiles), item["token"])
                    self.assertLess(int(args[3]), item["frame_count"], item["token"])
            fields = [field.strip() for field in info_block(self.info_text, item["info_symbol"]).split(",")]
            self.assertEqual(fields[1], item["palette_tag"])
            self.assertEqual(fields[4], str(dims[0]))
            self.assertEqual(fields[5], str(dims[1]))
            self.assertEqual(fields[14], item["picture_table"])
            animation_symbol = fields[13]
            self.assertRegex(self.info_text, r"(?m)^.*\b" + re.escape(animation_symbol) + r"\b.*$", item["token"])

    def test_additive_host_identity_and_namespaced_ids(self) -> None:
        self.assertIn("OBJ_EVENT_GFX_JOHTO_SILVER", self.ids_text)
        self.assertIn("OBJ_EVENT_GFX_JOHTO_SHINY_GYARADOS", self.ids_text)
        shared_ids = re.findall(r"^\s+(OBJ_EVENT_GFX_JOHTO_SHARED_[A-Z0-9_]+),$", self.ids_text, re.M)
        self.assertEqual(len(shared_ids), 49)
        self.assertEqual({item["public_id"] for item in self.objects}, set(shared_ids))
        self.assertIn("OBJ_EVENT_GFX_BERRY_TREE", self.ids_text)
        self.assertNotIn("OBJ_EVENT_GFX_JOHTO_SHARED_BERRY_TREE", self.ids_text)
        self.assertNotIn("OBJ_EVENT_GFX_JOHTO_SHARED_ITEM_BALL", self.ids_text)
        self.assertNotIn("OBJ_EVENT_GFX_JOHTO_SHARED_LIGHT_SPRITE", self.ids_text)

        tags = dict(re.findall(r"^#define\s+(OBJ_EVENT_PAL_TAG_JOHTO_SHARED_[A-Z0-9_]+)\s+0x([0-9A-Fa-f]+)$", self.ids_text, re.M))
        self.assertEqual(len(tags), 9)
        self.assertEqual(sorted(int(value, 16) for value in tags.values()), list(range(0x1240, 0x1249)))
        self.assertEqual({item["palette_tag"] for item in self.objects}, set(tags))

        movement = (ROOT / "src/event_object_movement.c").read_text(encoding="utf-8")
        pointers = (ROOT / "src/data/object_events/object_event_graphics_info_pointers.h").read_text(encoding="utf-8")
        rules = (ROOT / "spritesheet_rules.mk").read_text(encoding="utf-8")
        for marker, text in (
            ("johto_shared_assets.h", movement),
            ("johto_shared_info.h", movement),
            ("johto_shared_declarations.h", pointers),
            ("johto_shared_pointers.inc", pointers),
        ):
            self.assertEqual(text.count(marker), 1, marker)
        self.assertEqual(rules.count("# BEGIN JOHTO SHARED OBJECT FRAME RULES"), 1)
        self.assertEqual(rules.count("# END JOHTO SHARED OBJECT FRAME RULES"), 1)
        self.assertIn('#include "data/object_events/johto_assets.h"', movement)
        self.assertIn('#include "johto_declarations.h"', pointers)

    def test_importer_check_and_fail_closed_drift(self) -> None:
        checked = subprocess.run(
            ["C:/Python313/python.exe", "-X", "utf8", str(IMPORTER), "--donor-root", str(DONOR), "--check"],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(checked.returncode, 0, checked.stdout + checked.stderr)
        self.assertIn("Checked 68 outputs: 49 objects, 9 palettes, 58 assets", checked.stdout)

        spec = importlib.util.spec_from_file_location("johto_shared_importer", IMPORTER)
        self.assertIsNotNone(spec)
        module = importlib.util.module_from_spec(spec)
        assert spec.loader is not None
        spec.loader.exec_module(module)
        records = json.loads(json.dumps(self.payload["source_ledger"]))
        records[0]["donor_pictures"][0]["sha256"] = "0" * 64
        with self.assertRaises(SystemExit):
            module.preflight(ROOT, DONOR, records, self.payload)

    def test_default_mode_checks_without_writing(self) -> None:
        before = {p: p.read_bytes() for p in (DATA, INFO)}
        checked = subprocess.run(
            [sys.executable, str(IMPORTER), "--donor-root", str(DONOR)],
            cwd=ROOT, text=True, capture_output=True, check=False,
        )
        self.assertEqual(checked.returncode, 0, checked.stdout + checked.stderr)
        self.assertIn("Checked 68 outputs", checked.stdout)
        for path, data in before.items():
            self.assertEqual(path.read_bytes(), data)

    def test_ledger_uses_selected_root_and_fails_without_tracked_input(self) -> None:
        spec = importlib.util.spec_from_file_location("johto_shared_portable", IMPORTER)
        module = importlib.util.module_from_spec(spec)
        assert spec.loader is not None
        spec.loader.exec_module(module)
        self.assertFalse(hasattr(module, "CANONICAL_LEDGER"))
        with tempfile.TemporaryDirectory() as directory:
            selected_root = Path(directory)
            with self.assertRaisesRegex(SystemExit, "missing tracked source ledger"):
                module.load_records(selected_root)
            ledger = selected_root / "data/johto/shared_object_graphics.json"
            ledger.parent.mkdir(parents=True)
            ledger.write_bytes(DATA.read_bytes())
            records, payload = module.load_records(selected_root)
            self.assertEqual(records, self.payload["source_ledger"])
            self.assertEqual(payload, self.payload)


if __name__ == "__main__":
    unittest.main()
