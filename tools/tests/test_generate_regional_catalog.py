#!/usr/bin/env python3
"""Focused tests for the complete co-op regional map catalog."""

import importlib.util
import sys
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


if __name__ == "__main__":
    unittest.main()
