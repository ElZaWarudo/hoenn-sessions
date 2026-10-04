"""Cormoria section allocation and authenticated source checks."""

from __future__ import annotations

import copy
import json
import unittest
from pathlib import Path

from tools.cormoria import import_world, register_sections


ROOT = Path(__file__).resolve().parents[2]


class CormoriaSectionsTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.region, _, _ = import_world.load_manifests(ROOT)
        cls.host = json.loads((ROOT / register_sections.HOST_SECTIONS).read_text(encoding="utf-8"))["map_sections"]
        cls.donor = []
        for offset, entry in enumerate(cls.region["sections"]):
            installed = cls.host[register_sections.HOST_COUNT + offset]
            cls.donor.append({
                "map_section": entry["source_symbol"],
                "name": register_sections.DISPLAY_NAME_CORRECTIONS.get(
                    entry["target_symbol"], (installed["name"],))[0],
                **{key: installed[key] for key in ("x", "y", "width", "height")},
            })

    def test_all_51_sections_append_without_renumbering_host(self) -> None:
        result = register_sections.append_sections(self.host, self.donor, self.region["sections"])
        self.assertEqual(result[:250], self.host[:250])
        self.assertEqual(len(result), 301)
        self.assertEqual([entry["id"] for entry in result[250:]],
                         [entry["target_symbol"] for entry in self.region["sections"]])
        self.assertEqual(result[288]["name"], "Ivy River")
        self.assertEqual(result[300]["name"], "Champion Corridor")

    def test_later_region_tail_is_preserved(self) -> None:
        later = {"id": "MAPSEC_FUTURE_ISLAND", "name": "Future Island",
                 "x": 1, "y": 2, "width": 3, "height": 4}
        result = register_sections.append_sections(
            self.host + [later], self.donor, self.region["sections"])
        self.assertEqual(result[-1], later)
        with self.assertRaisesRegex(register_sections.SectionRegistrationError, "reuses"):
            register_sections.append_sections(
                self.host + [{**later, "id": self.host[250]["id"]}],
                self.donor, self.region["sections"])

    def test_rejects_shifted_id_and_duplicate_donor(self) -> None:
        shifted = copy.deepcopy(self.region["sections"])
        shifted[0]["target_id"] = 251
        with self.assertRaises(register_sections.SectionRegistrationError):
            register_sections.append_sections(self.host, self.donor, shifted)
        with self.assertRaises(register_sections.SectionRegistrationError):
            register_sections.append_sections(self.host, self.donor + [self.donor[0]],
                                              self.region["sections"])

    def test_rejects_host_baseline_drift_and_bad_geometry(self) -> None:
        host = copy.deepcopy(self.host)
        host[-1]["id"] = "MAPSEC_UNEXPECTED"
        with self.assertRaises(register_sections.SectionRegistrationError):
            register_sections.append_sections(host, self.donor, self.region["sections"])
        host = copy.deepcopy(self.host)
        host[0]["id"], host[1]["id"] = host[1]["id"], host[0]["id"]
        with self.assertRaises(register_sections.SectionRegistrationError):
            register_sections.append_sections(host, self.donor, self.region["sections"])
        donor = copy.deepcopy(self.donor)
        donor[0]["width"] = 0
        with self.assertRaises(register_sections.SectionRegistrationError):
            register_sections.append_sections(self.host, donor, self.region["sections"])


if __name__ == "__main__":
    unittest.main()
