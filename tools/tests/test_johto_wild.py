import copy
import json
import tempfile
import unittest
from pathlib import Path

from tools.johto import import_wild


ROOT = Path(import_wild.ROOT)
DONOR = Path(r"C:/Users/Mayor/Documents/Caribbean/johto-hns")


class JohtoWildImporterTests(unittest.TestCase):
    def parse_modified_source(self, change):
        source = json.loads((DONOR / import_wild.DONOR_SOURCE).read_text())
        entries = next(group["encounters"] for group in source["wild_encounter_groups"]
                       if group["label"] == "gWildMonHeaders")
        approved, _digest = import_wild._load_manifest(ROOT)
        change(entries, approved)
        with tempfile.TemporaryDirectory() as directory:
            donor = Path(directory)
            path = donor / import_wild.DONOR_SOURCE
            path.parent.mkdir(parents=True)
            path.write_text(json.dumps(source))
            return import_wild._parse_selected(
                donor, ROOT, approved, import_wild._species_symbols(ROOT)
            )

    def test_parser_rejects_distinct_labels_for_same_map_and_time(self):
        def change(entries, approved):
            entry = next(e for e in entries if e["base_label"] == "gRoute29_Night")
            entry["base_label"] = "gRoute29_UnexpectedDay"
        with self.assertRaisesRegex(import_wild.ImportErrorStrict, "duplicate source-time"):
            self.parse_modified_source(change)

    def test_parser_enforces_map_cardinality_with_same_header_totals(self):
        def change(entries, approved):
            represented = {e["map"] for e in entries}
            new_map = next(name for name in approved if name not in represented)
            entry = next(e for e in entries if e["base_label"] == "gRoute29_Night")
            entry["map"] = new_map
        with self.assertRaisesRegex(import_wild.ImportErrorStrict, "encounter maps total 94"):
            self.parse_modified_source(change)

    def test_parser_rejects_unknown_encounter_field(self):
        def change(entries, approved):
            entry = next(e for e in entries if e["base_label"] == "gRoute29")
            entry["unexpected_mons"] = {}
        with self.assertRaisesRegex(import_wild.ImportErrorStrict, "unsupported fields"):
            self.parse_modified_source(change)

    @classmethod
    def setUpClass(cls):
        cls.fragment = import_wild.build_fragment(DONOR, ROOT)
        cls.entries = cls.fragment["wild_encounter_groups"][0]["encounters"]

    def test_full_selected_corpus_and_exact_source_roundtrip(self):
        self.assertFalse(self.fragment["runtime_ready"])
        self.assertEqual(self.fragment["selection"]["approved_map_count"], 239)
        self.assertEqual(self.fragment["selection"]["maps_with_encounters"], 93)
        self.assertEqual(self.fragment["selection"]["source_header_count"], 149)
        self.assertEqual(self.fragment["selection"]["emitted_table_count"], 147)
        self.assertEqual(len(self.entries), 147)
        self.assertEqual(len({entry["map"] for entry in self.entries}), 93)

        source = json.loads((DONOR / import_wild.DONOR_SOURCE).read_text(encoding="utf-8"))
        manifest = json.loads(
            (ROOT / import_wild.MANIFEST_SOURCE).read_text(encoding="utf-8")
        )
        approved = {entry["source_map"]: entry for entry in manifest["maps"]}
        selected = [
            entry
            for group in source["wild_encounter_groups"]
            if group.get("for_maps")
            for entry in group["encounters"]
            if entry.get("map") in approved
        ]
        self.assertEqual(len(selected), 149)

        by_key = {
            (entry["map"], entry["time_of_day"], entry["base_label"]): entry
            for entry in self.entries
        }
        represented = set()
        for source_entry in selected:
            canonical, time = import_wild._normalize_label(source_entry["base_label"])
            if source_entry["map"] == "MAP_MT_SILVER_SNOW":
                canonical = {
                    "gMtSilver_SnowUnused": "gMtSilver_Snow",
                    "gMtSilver_SnowUnused_Night": "gMtSilver_Snow_Night",
                    "gMtSilver_SnowNight": "gMtSilver_Snow_Night",
                }.get(canonical, canonical)
            output_entry = by_key[(source_entry["map"], time, canonical)]
            represented.add(source_entry["base_label"])
            for field in import_wild.FIELD_ORDER:
                self.assertEqual(output_entry.get(field), source_entry.get(field))
            linkage = output_entry["map_linkage"]
            self.assertEqual(linkage["source_map"], source_entry["map"])
            self.assertEqual(linkage["host"]["map_id"], approved[source_entry["map"]]["proposed_map"]["map_id"])
            self.assertIn(source_entry["base_label"], output_entry["source_labels"])
        self.assertEqual(len(represented), 149)

    def test_verified_snow_duplicate_and_absent_night_metadata(self):
        snow = {
            (entry["time_of_day"], entry["base_label"]): entry
            for entry in self.entries
            if entry["map"] == "MAP_MT_SILVER_SNOW"
        }
        self.assertEqual(
            snow[("day", "gMtSilver_Snow")]["source_labels"],
            ["gMtSilver_Snow", "gMtSilver_SnowUnused"],
        )
        self.assertEqual(
            snow[("night", "gMtSilver_Snow_Night")]["source_labels"],
            ["gMtSilver_SnowNight", "gMtSilver_SnowUnused_Night"],
        )
        self.assertTrue(
            import_wild._duplicate_is_allowed(
                "MAP_MT_SILVER_SNOW",
                ["gMtSilver_Snow", "gMtSilver_SnowUnused"],
                "day",
            )
        )
        self.assertFalse(
            import_wild._duplicate_is_allowed(
                "MAP_ROUTE29", ["gRoute29", "gRoute29_Night"], "day"
            )
        )
        fallback = self.fragment["absent_night_fallback"]
        self.assertEqual(fallback["count"], 39)
        self.assertEqual(len(fallback["maps"]), 39)
        self.assertTrue(all(item["canonical_day_label"] for item in fallback["maps"]))

    def test_boundaries_and_unchanged_existing_default(self):
        self.assertEqual(import_wild.time_of_day_for_hour(5), "night")
        self.assertEqual(import_wild.time_of_day_for_hour(6), "day")
        self.assertEqual(import_wild.time_of_day_for_hour(17), "day")
        self.assertEqual(import_wild.time_of_day_for_hour(18), "night")
        self.assertEqual(
            self.fragment["time_windows"]["existing_region_default_selection"],
            "unchanged",
        )
        self.assertEqual(self.fragment["time_windows"]["day"], {"start": "06:00", "end": "17:59"})
        self.assertEqual(self.fragment["time_windows"]["night"], {"start": "18:00", "end": "05:59"})

    def test_invalid_slot_shapes_and_levels_fail_closed(self):
        sample = copy.deepcopy(self.entries[0]["land_mons"])
        with self.assertRaises(import_wild.ImportErrorStrict):
            import_wild._validate_mons({**sample, "unexpected": True}, "land_mons", {"SPECIES_SENTRET"})
        invalid_level = copy.deepcopy(sample)
        invalid_level["mons"][0]["min_level"] = 0
        with self.assertRaises(import_wild.ImportErrorStrict):
            import_wild._validate_mons(invalid_level, "land_mons", {"SPECIES_SENTRET"})
        invalid_species = copy.deepcopy(sample)
        invalid_species["mons"][0]["species"] = "SPECIES_NOT_IN_HOST"
        with self.assertRaises(import_wild.ImportErrorStrict):
            import_wild._validate_mons(invalid_species, "land_mons", {"SPECIES_SENTRET"})
        with self.assertRaises(import_wild.ImportErrorStrict):
            import_wild._validate_group_fields([{"type": "land_mons", "encounter_rates": [1]}])

    def test_check_is_deterministic_and_detects_drift(self):
        rendered = import_wild._render(self.fragment)
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "wild_encounters.json"
            output.write_text(rendered, encoding="utf-8")
            self.assertEqual(import_wild.run(DONOR, ROOT, output, check=True), 0)
            output.write_text(rendered + " ", encoding="utf-8")
            with self.assertRaises(import_wild.ImportErrorStrict):
                import_wild.run(DONOR, ROOT, output, check=True)


if __name__ == "__main__":
    unittest.main()
