"""Regression checks for the authenticated Cormoria object-event graphics unit."""
from __future__ import annotations

import hashlib
import json
import struct
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
LEDGER_PATH = ROOT / "data/cormoria/object_graphics.json"


def png_dimensions(path: Path) -> tuple[int, int]:
    raw = path.read_bytes()
    if raw[:8] != b"\x89PNG\r\n\x1a\n" or raw[12:16] != b"IHDR":
        raise AssertionError(f"not a PNG with an IHDR: {path}")
    return struct.unpack(">II", raw[16:24])


class CormoriaObjectGraphicsTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.ledger = json.loads(LEDGER_PATH.read_text(encoding="utf-8"))
        cls.objects = cls.ledger["objects"]

    def test_provenance_and_assets_are_pinned(self):
        provenance = self.ledger["provenance"]
        self.assertEqual(provenance["revision"], "f7997186345885bfa23a170e5f573851fc034b9b")
        self.assertEqual(len(self.objects), 13)
        self.assertEqual(len({entry["token"] for entry in self.objects}), 13)

        assets = [*self.objects, *self.ledger["additional_assets"]]
        for entry in assets:
            path = ROOT / "graphics/object_events/cormoria" / entry["asset"]
            self.assertTrue(path.is_file(), path)
            self.assertEqual(path.stat().st_size, entry["size"], path)
            self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(), entry["sha256"], path)
            self.assertEqual(png_dimensions(path), tuple(map(int, entry["dimensions"].split("x"))), path)

    def test_ids_are_append_only_and_pointer_bindings_are_complete(self):
        constants = (ROOT / "include/constants/event_objects.h").read_text(encoding="utf-8")
        start = constants.index("/* CORMORIA_OBJECT_GRAPHICS_IDS")
        end = constants.index("NUM_OBJ_EVENT_GFX", start)
        section = constants[start:end]
        pointers = (ROOT / "src/data/object_events/cormoria_pointers.inc").read_text(encoding="utf-8")
        declarations = (ROOT / "src/data/object_events/cormoria_declarations.h").read_text(encoding="utf-8")
        info = (ROOT / "src/data/object_events/cormoria_info.h").read_text(encoding="utf-8")
        pics = (ROOT / "src/data/object_events/cormoria_pic_tables.h").read_text(encoding="utf-8")
        for entry in self.objects:
            token = entry["token"]
            self.assertEqual(section.count(token), 1, token)
            self.assertEqual(pointers.count(f"[{token}]"), 1, token)
            self.assertEqual(declarations.count(entry["info"]), 1, entry["info"])
            self.assertEqual(info.count(entry["info"]), 1, entry["info"])
            self.assertEqual(pics.count(entry["pic_table"]), 1, entry["pic_table"])

    def test_build_isolated_by_rom_world_and_rules_cover_every_frame(self):
        movement = (ROOT / "src/event_object_movement.c").read_text(encoding="utf-8")
        pointer_table = (ROOT / "src/data/object_events/object_event_graphics_info_pointers.h").read_text(encoding="utf-8")
        rules = (ROOT / "spritesheet_rules.mk").read_text(encoding="utf-8")
        constants = (ROOT / "include/constants/event_objects.h").read_text(encoding="utf-8")
        for marker in (
            '#include "data/object_events/cormoria_assets.h"',
            '#include "data/object_events/cormoria_pic_tables.h"',
            '#include "data/object_events/cormoria_info.h"',
            '#include "data/object_events/cormoria_palettes.inc"',
        ):
            self.assertIn("#if ROM_WORLD == 2\n" + marker, movement)
        self.assertIn('#if ROM_WORLD == 2\n#include "cormoria_declarations.h"', pointer_table)
        self.assertIn('#if ROM_WORLD == 2\n#include "cormoria_pointers.inc"', pointer_table)
        for entry in self.objects:
            asset = entry["asset"]
            stem = asset[:-4]
            self.assertIn(f"graphics/object_events/cormoria/{stem}.4bpp:", rules, asset)
            self.assertIn(f"graphics/object_events/cormoria/{stem}.4bpp: %.4bpp: %.png\n\t$(GFX) $< $@ -mwidth 2 -mheight 4", rules, asset)
        self.assertIn("OBJ_EVENT_PAL_TAG_CORMORIA_GUBUKING", constants)
        self.assertIn("OBJ_EVENT_PAL_TAG_CORMORIA_SHUBUBU", constants)

    def test_donor_animation_profiles_are_preserved(self):
        info = (ROOT / "src/data/object_events/cormoria_info.h").read_text(encoding="utf-8")
        pics = (ROOT / "src/data/object_events/cormoria_pic_tables.h").read_text(encoding="utf-8")
        tmhm_frames = pics.split("sCormoriaPicTable_TMHMBall[] = {", 1)[1].split("};", 1)[0]
        self.assertEqual(tmhm_frames.count("overworld_frame("), 6)
        self.assertTrue(tmhm_frames.rstrip().endswith("gObjectEventPic_CormoriaTMHMBall, 2, 4, 0),"))
        for token in ("Gubuking", "Shububu"):
            profile = info[info.index(f"gObjectEventGraphicsInfo_Cormoria{token}Normal"):]
            profile = profile[:profile.index("};") + 2]
            self.assertIn(".size = 512", profile)
            self.assertIn(".reflectionPaletteTag = OBJ_EVENT_PAL_TAG_BRIDGE_REFLECTION", profile)
            self.assertIn(".paletteSlot = PALSLOT_PLAYER", profile)
            self.assertIn(".anims = sAnimTable_BrendanMayNormal", profile)
        tmhm = info[info.index("gObjectEventGraphicsInfo_CormoriaTMHMBall"):]
        self.assertIn(".inanimate = TRUE", tmhm)
        self.assertIn(".tracks = TRACKS_NONE", tmhm)
        self.assertIn(".anims = sAnimTable_Following", tmhm)


if __name__ == "__main__":
    unittest.main()
