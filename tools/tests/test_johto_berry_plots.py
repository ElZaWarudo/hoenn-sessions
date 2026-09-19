import hashlib
import json
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).parents[2]
DONOR = ROOT.parent.parent.parent / "johto-hns"
# Worktrees need the same pinned read-only donor as the canonical checkout.
if not DONOR.is_dir():
    DONOR = Path("C:/Users/Mayor/Documents/Caribbean/johto-hns")


def read_json(path):
    return json.loads(path.read_text(encoding="utf-8"))


class BerryPlotTests(unittest.TestCase):
    def setUp(self):
        self.ledger = read_json(ROOT / "data/johto/berry_plots.json")
        self.plots = self.ledger["plots"]

    def test_all_selected_donor_growth_objects_have_unique_identities(self):
        manifest = read_json(ROOT / "data/johto/region_manifest.json")
        actual = set()
        for entry in manifest["maps"]:
            name = entry["source_name"]
            data = read_json(DONOR / "data/maps" / name / "map.json")
            for index, obj in enumerate(data.get("object_events", [])):
                if obj.get("movement_type") == "MOVEMENT_TYPE_BERRY_TREE_GROWTH":
                    actual.add((name, index, obj["trainer_sight_or_berry_tree_id"]))
        expected = {(x["map"], x["object_index"], x["source_symbol"]) for x in self.plots}
        self.assertEqual(actual, expected)
        self.assertEqual(len(expected), 35)
        self.assertEqual(len({x["runtime_constant"] for x in self.plots}), 35)
        self.assertEqual([x["runtime_id"] for x in self.plots], list(range(90, 125)))
        self.assertEqual(
            [(x["map"], x["object_index"], x["runtime_id"]) for x in self.plots[:20]],
            [
                ("VioletCity", 26, 90),
                ("AzaleaTown", 16, 91),
                ("Route29", 15, 92),
                ("Route30", 20, 93),
                ("Route30", 22, 94),
                ("Route31", 16, 95),
                ("Route33", 10, 96),
                ("Route35", 21, 97),
                ("Route36", 20, 98),
                ("Route37", 13, 99),
                ("Route38", 17, 100),
                ("Route39", 15, 101),
                ("Route42", 15, 102),
                ("Route43", 14, 103),
                ("Route43", 15, 104),
                ("Route44", 25, 105),
                ("Route45", 20, 106),
                ("Route46", 12, 107),
                ("Route47", 31, 108),
                ("Route26", 15, 109),
            ],
        )

    def test_all_407_materialized_maps_normalize_growth_objects_by_donor_identity(self):
        manifest = read_json(ROOT / "data/johto/region_manifest.json")
        bindings = {
            (x["map"], x["object_index"]): x["runtime_constant"] for x in self.plots
        }
        actual = {}
        for entry in manifest["maps"]:
            name = entry["source_name"]
            donor = read_json(DONOR / "data/maps" / name / "map.json")
            materialized = read_json(ROOT / "data/maps" / name / "map.json")
            self.assertGreaterEqual(len(materialized.get("object_events", [])), len(donor.get("object_events", [])))
            for index, (source_obj, target_obj) in enumerate(
                zip(donor.get("object_events", []), materialized.get("object_events", []))
            ):
                key = (name, index)
                if source_obj.get("movement_type") == "MOVEMENT_TYPE_BERRY_TREE_GROWTH":
                    self.assertIn(key, bindings)
                    self.assertEqual(target_obj["trainer_sight_or_berry_tree_id"], bindings[key])
                    self.assertEqual(target_obj["script"], "Johto_BerryTreeScript")
                    actual[key] = target_obj["trainer_sight_or_berry_tree_id"]
        self.assertEqual(actual, bindings)

    def test_no_host_growth_object_collides(self):
        header = (ROOT / "include/constants/berry.h").read_text()
        constants = {name: int(value) for name, value in re.findall(r"#define\s+(BERRY_TREE_\w+)\s+(\d+)\b", header)}
        johto_constants = {x["runtime_constant"]: x["runtime_id"] for x in self.plots}
        occupied = set()
        materialized_johto = set()
        for path in (ROOT / "data/maps").glob("*/map.json"):
            for obj in read_json(path).get("object_events", []):
                if obj.get("movement_type") == "MOVEMENT_TYPE_BERRY_TREE_GROWTH":
                    value = str(obj["trainer_sight_or_berry_tree_id"])
                    if value in johto_constants:
                        materialized_johto.add(johto_constants[value])
                    else:
                        occupied.add(int(value) if value.isdigit() else constants[value])
        self.assertTrue(occupied)
        self.assertTrue(occupied.isdisjoint(x["runtime_id"] for x in self.plots))
        self.assertEqual(materialized_johto, {x["runtime_id"] for x in self.plots})
        count = int(re.search(r"#define BERRY_TREES_COUNT (\d+)", header).group(1))
        self.assertTrue(all(0 <= x["runtime_id"] < count for x in self.plots))

    def test_donor_seeding_constants_and_runtime_table_agree(self):
        donor = (DONOR / "data/scripts/new_game.inc").read_text()
        header = (ROOT / "include/constants/johto_berry_plots.h").read_text()
        source = (ROOT / "src/johto/berry_plots.c").read_text()
        table = re.findall(r"\{(JOHTO_BERRY_TREE_\w+), (BERRY_ID_\w+)\}", source)
        self.assertEqual(table, [(x["runtime_constant"], x["berry"]) for x in self.plots])
        constants = dict(re.findall(r"#define (JOHTO_BERRY_TREE_\w+) (\d+)", header))
        self.assertEqual(constants, {x["runtime_constant"]: str(x["runtime_id"]) for x in self.plots})
        for x in self.plots:
            berry = x["berry"].removeprefix("BERRY_ID_")
            pattern = r"setberrytree\s+" + re.escape(x["source_symbol"]) + r",\s*ITEM_TO_BERRY\(ITEM_" + berry + r"_BERRY\),\s*BERRY_STAGE_BERRIES"
            self.assertRegex(donor, pattern)
            self.assertEqual(x["stage"], "BERRY_STAGE_BERRIES")
        self.assertIn("PlantBerryTree(sInitialPlots[i].id, sInitialPlots[i].berry, BERRY_STAGE_BERRIES, FALSE)", source)
        self.assertIn("JOHTO_BERRY_PLOTS_LAST < BERRY_TREES_COUNT", source)

    def test_initialization_is_exclusive_to_new_game(self):
        calls = []
        for path in (ROOT / "src").rglob("*.c"):
            text = path.read_text(encoding="utf-8")
            if "JohtoBerryPlots_InitializeNewGame();" in text:
                calls.append(path.relative_to(ROOT).as_posix())
        self.assertEqual(calls, ["src/new_game.c"])
        source = (ROOT / "src/new_game.c").read_text()
        self.assertRegex(source, r"ClearBerryTrees\(\);\s+JohtoBerryPlots_InitializeNewGame\(\);")
        self.assertGreater(source.index("JohtoBerryPlots_InitializeNewGame();"), source.index("void NewGameInitData(void)"))

    def test_normalized_provenance_covers_every_source(self):
        self.assertTrue(self.ledger["campaign_ready"])
        self.assertEqual(self.ledger["source_revision"], "751823abaf677020bcd72c45fe3e7cb2b8a576e4")
        expected = {"data/scripts/new_game.inc"} | {"data/maps/" + x["map"] + "/map.json" for x in self.plots}
        self.assertEqual({x["path"] for x in self.ledger["source_hashes"]}, expected)
        for x in self.ledger["source_hashes"]:
            data = (DONOR / x["path"]).read_bytes().replace(b"\r\n", b"\n")
            self.assertEqual(hashlib.sha256(data).hexdigest(), x["sha256"])


if __name__ == "__main__":
    unittest.main()
