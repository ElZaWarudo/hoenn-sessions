"""Authenticated Cormoria wild encounter import checks."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

from tools.cormoria import import_wild_encounters


ROOT = import_wild_encounters.ROOT
SOURCE_ROOT = Path(os.environ["CORMORIA_WILD_SOURCE"]) if os.environ.get("CORMORIA_WILD_SOURCE") else None


class CormoriaWildEncounterTests(unittest.TestCase):
    def test_merge_preserves_host_entries(self):
        host = {
            "wild_encounter_groups": [{
                "label": "gWildMonHeaders",
                "encounters": [{"map": "MAP_ROUTE101", "base_label": "gRoute101"}],
            }],
        }
        imported = [{"map": "MAP_CORMORIA_ROUTE1", "base_label": "gRoute1"}]
        merged = import_wild_encounters.merge(host, imported)
        self.assertEqual(merged["wild_encounter_groups"][0]["encounters"], host["wild_encounter_groups"][0]["encounters"] + imported)
        self.assertEqual(host["wild_encounter_groups"][0]["encounters"][0]["map"], "MAP_ROUTE101")


@unittest.skipUnless(SOURCE_ROOT and (SOURCE_ROOT / import_wild_encounters.SOURCE).is_file(),
                     "set CORMORIA_WILD_SOURCE to the authenticated staged donor source root")
class CormoriaPinnedWildEncounterTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.donor, cls.maps = import_wild_encounters._load_donor(SOURCE_ROOT)
        cls.records = import_wild_encounters._cormoria_entries(cls.donor, cls.maps)

    def test_exact_entry_count_and_namespaced_maps(self):
        self.assertEqual(len(self.records), 41)
        self.assertTrue(all(row["map"].startswith("MAP_CORMORIA_") for row in self.records))
        self.assertEqual(self.records[0]["map"], "MAP_CORMORIA_ROUTE1")
        self.assertEqual(self.records[-1]["map"], "MAP_CORMORIA_LILY_POND")
        self.assertEqual(sum(row["map"] == "MAP_CORMORIA_RANGER_INSTITUTE_BIOME" for row in self.records), 3)

    def test_species_levels_and_area_rates_are_verbatim(self):
        donor_entries = [
            entry for entry in self.donor["wild_encounter_groups"][0]["encounters"]
            if entry.get("map") in self.maps
        ]
        self.assertEqual(len(donor_entries), len(self.records))
        for donor, imported in zip(donor_entries, self.records):
            self.assertEqual(imported["map"], self.maps[donor["map"]])
            source_without_map = {key: value for key, value in donor.items() if key != "map"}
            imported_without_map = {key: value for key, value in imported.items() if key != "map"}
            self.assertEqual(imported_without_map, source_without_map)

    def test_generated_table_keeps_all_host_entries(self):
        current = json.loads((ROOT / import_wild_encounters.SOURCE).read_text(encoding="utf-8"))
        baseline = json.loads(subprocess.check_output(
            ["git", "show", f"HEAD:{import_wild_encounters.SOURCE}"], cwd=ROOT,
        ))
        targets = {row["map"] for row in self.records}
        retained = [row for row in current["wild_encounter_groups"][0]["encounters"]
                    if row.get("map") not in targets]
        self.assertEqual(retained, baseline["wild_encounter_groups"][0]["encounters"])
        self.assertEqual(current["wild_encounter_groups"][0]["fields"], baseline["wild_encounter_groups"][0]["fields"])

    def test_generated_table_is_current(self):
        self.assertEqual(import_wild_encounters.run(SOURCE_ROOT, ROOT, check=True), 0)

    def test_source_tamper_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            temporary_root = Path(temporary)
            destination = temporary_root / import_wild_encounters.SOURCE
            destination.parent.mkdir(parents=True)
            shutil.copyfile(SOURCE_ROOT / import_wild_encounters.SOURCE, destination)
            raw = destination.read_bytes()
            destination.write_bytes(raw.replace(b"SPECIES_RATTATA_ALOLA", b"SPECIES_RATTATA_HISUI", 1))
            with self.assertRaisesRegex(import_wild_encounters.ImportErrorStrict, "authenticated donor bytes"):
                import_wild_encounters._load_donor(temporary_root)


if __name__ == "__main__":
    unittest.main()
