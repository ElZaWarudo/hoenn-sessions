"""Cormoria plot IDs stay local and retain donor map/script semantics."""

from __future__ import annotations

import unittest
from pathlib import Path

from tools.cormoria import berry_plots, register_maps, register_scripts

ROOT = Path(__file__).resolve().parents[2]


class CormoriaBerryPlotTests(unittest.TestCase):
    def test_all_donor_ids_have_distinct_append_only_slots(self) -> None:
        bindings = berry_plots.bindings(ROOT)
        self.assertEqual(len(bindings), 29)
        self.assertEqual(bindings["BERRY_TREE_FENNILAHL_ORAN"],
                         "Cormoria_BERRY_TREE_FENNILAHL_ORAN")
        self.assertEqual(bindings["BERRY_TREE_HOYA_C"],
                         "Cormoria_BERRY_TREE_HOYA_C")

        header = (ROOT / "include/constants/berry.h").read_text(encoding="utf-8")
        self.assertIn("#define BERRY_TREES_COUNT 192", header)
        self.assertIn("#define CORMORIA_BERRY_PLOTS_FIRST 125", header)
        self.assertIn("#define CORMORIA_BERRY_PLOTS_LAST 153", header)

    def test_only_berry_map_objects_translate_and_donor_alias_remains(self) -> None:
        bindings = berry_plots.bindings(ROOT)
        mapped = {"name": "Cormoria_Route3", "object_events": [
            {"movement_type": "MOVEMENT_TYPE_BERRY_TREE_GROWTH",
             "trainer_sight_or_berry_tree_id": "BERRY_TREE_FENNILAHL_ORAN"},
            {"movement_type": "MOVEMENT_TYPE_BERRY_TREE_GROWTH",
             "trainer_sight_or_berry_tree_id": "BERRY_TREE_FENNILAHL_ORAN"},
            {"movement_type": "MOVEMENT_TYPE_FACE_DOWN",
             "trainer_sight_or_berry_tree_id": "90"},
        ]}
        found = register_maps._adapt_berry_plots(mapped, bindings)
        self.assertEqual(found, ["BERRY_TREE_FENNILAHL_ORAN"] * 2)
        self.assertEqual(mapped["object_events"][0]["trainer_sight_or_berry_tree_id"],
                         mapped["object_events"][1]["trainer_sight_or_berry_tree_id"])
        self.assertEqual(mapped["object_events"][2]["trainer_sight_or_berry_tree_id"], "90")
        mapped["object_events"][0]["trainer_sight_or_berry_tree_id"] = "BERRY_TREE_UNKNOWN"
        with self.assertRaises(register_maps.MapRegistrationError):
            register_maps._adapt_berry_plots(mapped, bindings)

    def test_new_game_script_seed_ids_translate_without_changing_dialogue(self) -> None:
        source = ('setberrytree BERRY_TREE_ROUTE2_ORAN, ITEM_TO_BERRY(ITEM_ORAN_BERRY), BERRY_STAGE_BERRIES\n'
                  'msgbox "BERRY_TREE_ROUTE2_ORAN"\n')
        rendered = register_scripts._rename(source, berry_plots.bindings(ROOT))
        self.assertIn("setberrytree Cormoria_BERRY_TREE_ROUTE2_ORAN", rendered)
        self.assertIn('"BERRY_TREE_ROUTE2_ORAN"', rendered)


if __name__ == "__main__":
    unittest.main()
