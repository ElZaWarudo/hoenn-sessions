import hashlib
import json
import re
import subprocess
import unittest
from pathlib import Path


ROOT = Path(__file__).parents[2]
DONOR = Path("C:/Users/Mayor/Documents/Caribbean/johto-hns")
CONTRACT_HASH = "sha256:60b66f702080efdf4492e32ae8607adf321340e26e521333e80f376bf1bb74f4"
ASSETS = ("cheri", "chesto", "pecha", "rawst", "aspear", "leppa", "oran", "persim", "lum", "sitrus", "dirt_pile", "sprout")
SPECIES = ASSETS[:10]
PALETTE_SLOTS = {
    "cheri": [3, 4, 4, 2, 2],
    "chesto": [3, 4, 2, 4, 4],
    "pecha": [3, 4, 4, 5, 5],
    "rawst": [3, 4, 4, 3, 3],
    "aspear": [3, 4, 3, 4, 4],
    "leppa": [3, 4, 3, 2, 2],
    "oran": [3, 4, 2, 3, 3],
    "persim": [3, 4, 2, 3, 3],
    "lum": [3, 4, 4, 5, 5],
    "sitrus": [3, 4, 4, 5, 5],
}
PALETTE_TAGS = {
    2: "OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_WHITE",
    3: "OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_PINK",
    4: "OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_BLUE",
    5: "OBJ_EVENT_PAL_TAG_JOHTO_SHARED_NPC_GREEN",
}


def normalized_sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


class BerryGraphicsTests(unittest.TestCase):
    def setUp(self):
        self.manifest = json.loads((ROOT / "data/johto/berry_graphics.json").read_text(encoding="utf-8"))
        self.header = (ROOT / "src/data/object_events/johto_berry_graphics.h").read_text(encoding="utf-8")
        self.movement = (ROOT / "src/event_object_movement.c").read_text(encoding="utf-8")

    def test_pinned_donor_assets_are_exact_and_dimensioned(self):
        self.assertEqual(self.manifest["contract_hash"], CONTRACT_HASH)
        self.assertEqual(self.manifest["donor_revision"], "751823abaf677020bcd72c45fe3e7cb2b8a576e4")
        for name in ASSETS:
            source = DONOR / "graphics/object_events/pics/berry_trees" / f"{name}.png"
            target = ROOT / "graphics/johto/berry_trees" / f"{name}.png"
            self.assertEqual(target.read_bytes(), source.read_bytes(), name)
            record = self.manifest["assets"][name]
            self.assertEqual(normalized_sha(target), record["sha256"], name)
            self.assertEqual(record["dimensions"], [96, 32] if name in SPECIES else [16, 16] if name == "dirt_pile" else [32, 16])

    def test_frame_packing_and_semantic_stage_mapping_are_independent(self):
        self.assertEqual(self.manifest["stage_mapping_host_to_donor"], [0, 1, 2, 2, 2, 3, 4])
        self.assertEqual(self.manifest["palette_slot_tags"], {str(k): v for k, v in PALETTE_TAGS.items()})
        for name in SPECIES:
            block = re.search(rf"sJohtoBerryPicTable_{name.upper()}\[\] = \{{(.*?)^\}};", self.header, re.M | re.S)
            self.assertIsNotNone(block, name)
            self.assertEqual(len(re.findall(r"overworld_frame\(", block.group(1))), 9)
            self.assertEqual(re.findall(rf"gJohtoBerryPic_{name.upper()}, 2, 4, (\d+)\)", block.group(1)), [str(i) for i in range(6)])
            palettes = re.search(rf"sJohtoBerryPaletteTags_{name.upper()}\[\] = \{{(.*?)^\}};", self.header, re.M | re.S)
            self.assertEqual(
                [x.strip() for x in palettes.group(1).split(",") if x.strip()],
                [PALETTE_TAGS[PALETTE_SLOTS[name][i]] for i in [0, 1, 2, 2, 2, 3, 4]],
            )

    def test_actual_renderer_is_plot_and_species_gated(self):
        self.assertIn('#include "data/object_events/johto_berry_graphics.h"', self.movement)
        self.assertIn("JohtoBerryGraphics_Apply(objectEvent, sprite, berryId, berryStage)", self.movement)
        self.assertRegex(self.header, r"plotId >= JOHTO_BERRY_PLOTS_FIRST")
        self.assertRegex(self.header, r"plotId <= JOHTO_BERRY_PLOTS_LAST")
        self.assertRegex(self.header, r"stage < ARRAY_COUNT\(sJohtoBerryPaletteTags_CHERI\)")
        self.assertIn("FindObjectEventPaletteIndexByTag(paletteTags[stage])", self.header)
        self.assertIn("sprite->images = sJohtoBerryPicTables[berryId]", self.header)
        self.assertIn("sprite->images = gBerries[berryId].berryTreePicTable", self.movement)

    def test_importer_checks_exact_assets_and_shared_roundtrip(self):
        importer = ROOT / "tools/johto/import_berry_graphics.py"
        result = subprocess.run(
            ["C:/Python313/python.exe", "-X", "utf8", str(importer), "--donor-root", str(DONOR), "--root", str(ROOT), "--check"],
            cwd=ROOT,
            text=True,
            capture_output=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("12 donor assets", result.stdout)
        shared = subprocess.run(
            ["C:/Python313/python.exe", "-X", "utf8", "tools/johto/import_shared_object_graphics.py", "--donor-root", str(DONOR), "--root", str(ROOT), "--check"],
            cwd=ROOT,
            text=True,
            capture_output=True,
        )
        self.assertEqual(shared.returncode, 0, shared.stdout + shared.stderr)
        self.assertIn("49 objects, 9 palettes, 58 assets", shared.stdout)


if __name__ == "__main__":
    unittest.main()
