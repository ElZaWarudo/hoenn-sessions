#!/usr/bin/env python3
"""Focused tests for the complete co-op regional map catalog."""

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = ROOT / "tools" / "coop" / "generate_regional_catalog.py"
SPEC = importlib.util.spec_from_file_location("generate_regional_catalog", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
catalog = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = catalog
SPEC.loader.exec_module(catalog)


class RegionalCatalogTests(unittest.TestCase):
    def test_catalog_covers_all_maps_and_later_kanto(self):
        entries = catalog.build_entries()

        self.assertEqual(len(entries), catalog.EXPECTED_MAP_COUNT)
        self.assertEqual(len({(group, number) for _, _, group, number in entries}), len(entries))
        self.assertTrue(
            any(region == "Johto" and key == "NEW_BARK_TOWN" for region, key, _, _ in entries)
        )
        later_kanto = [entry for entry in entries if entry[1].startswith("KANTO_LATER_")]
        self.assertEqual(len(later_kanto), 168)
        self.assertTrue(all(region == "Kanto" for region, _, _, _ in later_kanto))

    def test_geographic_kanto_routes_use_engine_region_authority(self):
        sections, sevii, special_area = catalog.source_sections()

        for section in sorted(catalog.KANTO_ENGINE_JOHTO_SECTIONS):
            with self.subTest(section=section):
                self.assertEqual(
                    catalog.protocol_region(
                        "REGION_KANTO", section, sections, sevii, special_area
                    ),
                    "Kanto",
                )
                with self.assertRaisesRegex(catalog.CatalogError, "requires REGION_KANTO"):
                    catalog.protocol_region(
                        "REGION_JOHTO", section, sections, sevii, special_area
                    )

    def test_catalog_carries_layout_bounds_and_vanilla_escape_endpoints(self):
        records = catalog._build_catalog_records()
        targets, ranges = catalog.build_escape_targets(records)
        granite = next(record for record in records if record.map_key == "GRANITE_CAVE_1F")

        self.assertEqual((granite.width, granite.height), (42, 15))
        self.assertTrue(granite.allow_escaping)
        start, length = ranges[granite.map_key]
        self.assertIn((0, 21, 48, 17), targets[start : start + length])

        littleroot = next(record for record in records if record.map_key == "LITTLEROOT_TOWN")
        self.assertEqual((littleroot.width, littleroot.height), (20, 20))
        self.assertFalse(littleroot.allow_escaping)
        self.assertEqual(ranges[littleroot.map_key][1], 0)

    def test_escape_endpoints_follow_scripts_and_indoor_chains(self):
        records = catalog._build_catalog_records()
        targets, ranges = catalog.build_escape_targets(records)
        by_coordinates = {(record.group, record.number): record for record in records}

        def endpoints(map_key):
            start, length = ranges[map_key]
            return {
                (by_coordinates[(group, number)].map_key, x, y)
                for group, number, x, y in targets[start : start + length]
            }

        # Deeper floors inherit the entrance endpoint set by UpdateEscapeWarp.
        self.assertEqual(endpoints("GRANITE_CAVE_B2F"), {("ROUTE106", 48, 17)})
        # A map-local setescapewarp supplies the endpoint for outdoor-typed maps.
        self.assertEqual(
            endpoints("SIX_ISLAND_PATTERN_BUSH"),
            {("SIX_ISLAND_GREEN_PATH", 45, 10), ("SIX_ISLAND_GREEN_PATH", 64, 10)},
        )
        self.assertIn(("SOOTOPOLIS_CITY", 31, 17), endpoints("CAVE_OF_ORIGIN_B1F"))
        # The shared abnormal-weather script seeds the Marine Cave dive entry.
        self.assertIn(("ROUTE105", 11, 29), endpoints("MARINE_CAVE_END"))
        # Every endpoint lies inside its target layout.
        for group, number, x, y in targets:
            target = by_coordinates[(group, number)]
            self.assertLess(x, target.width)
            self.assertLess(y, target.height)

    def test_setescapewarp_parser_rejects_warp_id_form(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "scripts.inc"
            path.write_text("Label::\n\tsetescapewarp MAP_ROUTE101, 255, 3, 4\n", encoding="utf-8")
            self.assertEqual(catalog.parse_setescapewarps(path), [("MAP_ROUTE101", 3, 4)])
            path.write_text("Label::\n\tsetescapewarp MAP_ROUTE101, 2, 3, 4\n", encoding="utf-8")
            with self.assertRaises(catalog.CatalogError):
                catalog.parse_setescapewarps(path)


if __name__ == "__main__":
    unittest.main()
