"""Independent checks for append-only Johto region section registration."""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.johto import register_sections


ROOT = Path(register_sections.ROOT)
DONOR = Path(r"C:/Users/Mayor/Documents/Caribbean/johto-hns")


class JohtoSectionRegistrationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.generated = register_sections.build_sections(DONOR, ROOT)
        cls.records = cls.generated["map_sections"]
        cls.manifest = json.loads(
            (ROOT / "data/johto/region_manifest.json").read_text(encoding="utf-8")
        )
        cls.donor = json.loads(
            (DONOR / "src/data/region_map/region_map_sections.json").read_text(
                encoding="utf-8"
            )
        )

    def test_count_order_and_immutable_baseline(self):
        self.assertEqual(len(self.records), 250)
        baseline = json.loads(
            (ROOT / "data/johto/host_identity_baseline.json").read_text(encoding="utf-8")
        )
        expected = [entry["id"] for entry in baseline["section_constants"]]
        self.assertEqual([entry["id"] for entry in self.records[:210]], expected)
        tail = self.manifest["sections"]["allocation_order"]
        self.assertEqual(
            tuple(tail),
            register_sections.EXPECTED_SECTION_ALLOCATION_ORDER,
        )
        self.assertEqual(len(tail), 84)
        tail = [symbol for symbol in tail if symbol.startswith("MAPSEC_JOHTO_")]
        self.assertEqual(len(tail), 40)
        self.assertEqual([entry["id"] for entry in self.records[210:]], tail)
        self.assertEqual(
            [index for index, _ in enumerate(self.records[210:], 210)],
            list(range(210, 250)),
        )

    def test_donor_names_and_geometry_are_exact(self):
        donor_records = {entry["map_section"]: entry for entry in self.donor["map_sections"]}
        self.assertEqual(
            self.records[209],
            {
                "id": "MAPSEC_NEW_BARK_TOWN",
                "name": donor_records["MAPSEC_NEW_BARK_TOWN"]["name"],
                "x": 19,
                "y": 10,
                "width": 1,
                "height": 1,
            },
        )
        for record in self.records[210:]:
            donor_id = "MAPSEC_" + record["id"].removeprefix("MAPSEC_JOHTO_")
            source = donor_records[donor_id]
            expected = {
                "id": record["id"],
                "name": source["name"],
                "x": source["x"],
                "y": source["y"],
                "width": source["width"],
                "height": source["height"],
            }
            self.assertEqual(record, expected)

    def test_manifest_allocation_rejects_duplicate_unknown_reordered_and_nonstring(self):
        for mutation in ("duplicate", "unknown", "reordered", "nonstring", "kanto_reordered"):
            with self.subTest(mutation=mutation):
                manifest = json.loads(json.dumps(self.manifest))
                order = manifest["sections"]["allocation_order"]
                positions = [i for i, symbol in enumerate(order)
                             if symbol.startswith("MAPSEC_JOHTO_")]
                a, b = positions[:2]
                if mutation == "duplicate":
                    order[b] = order[a]
                elif mutation == "unknown":
                    order[a] = "MAPSEC_JOHTO_UNKNOWN"
                elif mutation == "reordered":
                    order[a], order[b] = order[b], order[a]
                elif mutation == "kanto_reordered":
                    order[-1], order[-2] = order[-2], order[-1]
                else:
                    order[a] = None
                # Keep entry ordinals aligned with tampered allocation: the
                # closed identity list must reject even self-consistent drift.
                for i in positions:
                    for entry in manifest["sections"]["entries"]:
                        if entry["target_symbol"] == order[i]:
                            entry["target_id"] = 210 + positions.index(i)
                with self.assertRaisesRegex(
                    register_sections.SectionRegistrationError,
                    "region manifest section allocation order drifted",
                ):
                    register_sections._tail_symbols(manifest)

    def test_deterministic_output_and_pin_rejection(self):
        self.assertEqual(self.generated, register_sections.build_sections(DONOR, ROOT))
        with mock.patch.object(register_sections, "_git", return_value="wrong"):
            with self.assertRaisesRegex(register_sections.SectionRegistrationError, "pin mismatch"):
                register_sections.build_sections(DONOR, ROOT)

    def test_unsupported_donor_fields_and_geometry_fail_closed(self):
        source = {
            "MAPSEC_TEST": {
                "map_section": "MAPSEC_TEST",
                "name": "TEST",
                "x": 0,
                "y": 0,
                "width": 1,
                "height": 1,
                "unexpected": True,
            }
        }
        with self.assertRaisesRegex(register_sections.SectionRegistrationError, "unsupported donor"):
            register_sections._record_for_donor(source, "MAPSEC_TEST")
        source["MAPSEC_TEST"].pop("unexpected")
        source["MAPSEC_TEST"]["width"] = 0
        with self.assertRaisesRegex(register_sections.SectionRegistrationError, "geometry"):
            register_sections._record_for_donor(source, "MAPSEC_TEST")

    def test_check_rejects_stale_source(self):
        rendered = register_sections.render(self.generated)
        with tempfile.TemporaryDirectory(dir=ROOT) as directory:
            stale = Path(directory) / "region_map_sections.json"
            stale.write_text(rendered + " ", encoding="utf-8")
            with mock.patch.object(register_sections, "HOST_SECTIONS", stale):
                with self.assertRaisesRegex(register_sections.SectionRegistrationError, "drift"):
                    register_sections.run(DONOR, ROOT, check=True)


if __name__ == "__main__":
    unittest.main()
