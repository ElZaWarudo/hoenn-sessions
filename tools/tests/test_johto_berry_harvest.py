import hashlib
import json
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).parents[2]
DONOR = Path("C:/Users/Mayor/Documents/Caribbean/johto-hns")


class BerryHarvestTests(unittest.TestCase):
    def setUp(self):
        self.source = (ROOT / "src/johto/berry_plots.c").read_text(encoding="utf-8")
        self.script = (ROOT / "data/scripts/johto_berry_tree.inc").read_text(encoding="utf-8")

    def test_donor_automatic_regrowth_and_provenance(self):
        data = json.loads((ROOT / "data/johto/berry_harvest.json").read_text(encoding="utf-8"))
        self.assertTrue(data["campaign_ready"])
        self.assertEqual(data["donor_revision"], "751823abaf677020bcd72c45fe3e7cb2b8a576e4")
        self.assertEqual(data["script_alias"], {"BerryTreeScript": "Johto_BerryTreeScript"})
        self.assertEqual({x["path"] for x in data["source_hashes"]}, {"src/berry.c", "data/scripts/berry_tree.inc"})
        for x in data["source_hashes"]:
            raw = (DONOR / x["path"]).read_bytes().replace(b"\r\n", b"\n")
            self.assertEqual(hashlib.sha256(raw).hexdigest(), x["sha256"])
        donor = (DONOR / "src/berry.c").read_text(encoding="utf-8")
        body = donor.split("void ObjectEventInteractionPlantBerryTree(void)", 1)[1].split("void ObjectEventInteractionPickBerryTree", 1)[0]
        self.assertIn("BERRY_STAGE_SPROUTED", body)
        self.assertIn("TRUE", body)
        self.assertNotIn("RemoveBagItem", body)

    def test_transaction_commits_only_after_exact_award(self):
        body = self.source.split("u8 JohtoBerryPlots_TryHarvest", 1)[1].split("void Script_JohtoHarvestBerryTree", 1)[0]
        self.assertLess(body.index("if (!IsJohtoPlot(plot))"), body.index("GetBerryTreeInfo(plot)"))
        self.assertIn("tree->stage != BERRY_STAGE_BERRIES || item == ITEM_NONE || count == 0", body)
        self.assertLess(body.index("AddBagItem(item, count)"), body.index("RemoveBerryTree(plot)"))
        self.assertLess(body.index("RemoveBerryTree(plot)"), body.index("PlantBerryTree(plot, berry, BERRY_STAGE_SPROUTED, TRUE)"))
        self.assertIn("VarGet(VAR_DAILY_PICKED_BERRIES) + count", body)
        self.assertNotIn("RemoveBagItem", self.source)
        self.assertNotIn("gSpecialVar_ItemId", self.source)

    def test_native_identity_effects_and_captured_values(self):
        native = self.source.split("void Script_JohtoHarvestBerryTree", 1)[1]
        self.assertLess(native.index("Script_RequestEffects(SCREFF_V1 | SCREFF_SAVE)"), native.index("gSpecialVar_Result ="))
        for text in ("gSelectedObjectEvent >= OBJECT_EVENTS_COUNT", "!object->active", "MOVEMENT_TYPE_BERRY_TREE_GROWTH", "object->spriteId >= MAX_SPRITES", "if (!IsJohtoPlot(plot))"):
            self.assertIn(text, native)
        self.assertLess(native.index("count = tree->berryYield"), native.index("JohtoBerryPlots_TryHarvest(plot)"))
        self.assertRegex(native, r"if \(gSpecialVar_Result == JOHTO_BERRY_HARVEST_SUCCESS\)\s+SetBerryTreeJustPicked\(object->localId, object->mapNum, object->mapGroup\);")

    def test_linked_script_has_distinct_complete_branches(self):
        event = (ROOT / "data/event_scripts.s").read_text(encoding="utf-8")
        self.assertEqual(event.count('.include "data/scripts/johto_berry_tree.inc"'), 1)
        self.assertEqual(event.count('#include "constants/johto_berry_plots.h"'), 1)
        self.assertIn('.include "data/johto/campaign_scripts.inc"', event)
        self.assertIn("callnative Script_JohtoHarvestBerryTree, requests_effects=1", self.script)
        self.assertIn("JOHTO_BERRY_HARVEST_SUCCESS, Johto_BerryTree_Picked", self.script)
        self.assertIn("JOHTO_BERRY_HARVEST_BAG_FULL, Johto_BerryTree_Full", self.script)
        self.assertEqual(len(re.findall(r"^    release$", self.script, re.M)), 3)
        self.assertEqual(len(re.findall(r"^    end$", self.script, re.M)), 3)
        self.assertIn("playfanfare MUS_OBTAIN_BERRY", self.script)
        self.assertNotIn("ObjectEventInteractionGetBerryTreeData", self.script)
        self.assertNotIn("PlantBerryTree", self.script)
        self.assertIn("These BERRIES aren't ripe yet.$", self.script)

    def test_existing_twenty_allocations_and_seeds_stay_exact(self):
        plots = json.loads((ROOT / "data/johto/berry_plots.json").read_text(encoding="utf-8"))["plots"]
        self.assertEqual([p["runtime_id"] for p in plots[:20]], list(range(90, 110)))
        self.assertEqual([p["runtime_id"] for p in plots], list(range(90, 125)))
        constants = dict(re.findall(r"#define (JOHTO_BERRY_TREE_\w+) (\d+)", (ROOT / "include/constants/johto_berry_plots.h").read_text(encoding="utf-8")))
        self.assertEqual(constants, {p["runtime_constant"]: str(p["runtime_id"]) for p in plots})
        table = re.findall(r"\{(JOHTO_BERRY_TREE_\w+), (BERRY_ID_\w+)\}", self.source)
        self.assertEqual(table, [(p["runtime_constant"], p["berry"]) for p in plots])
        self.assertIn("PlantBerryTree(sInitialPlots[i].id, sInitialPlots[i].berry, BERRY_STAGE_BERRIES, FALSE)", self.source)
        for plot in plots:
            obj = json.loads((ROOT / "data/maps" / plot["map"] / "map.json").read_text(encoding="utf-8"))["object_events"][plot["object_index"]]
            self.assertEqual(obj["trainer_sight_or_berry_tree_id"], plot["runtime_constant"])
            self.assertEqual(obj["script"], "Johto_BerryTreeScript")


if __name__ == "__main__":
    unittest.main()
