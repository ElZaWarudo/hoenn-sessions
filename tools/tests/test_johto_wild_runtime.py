import io
import json
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from tools.johto import register_wild
from tools.wild_encounters import wild_encounters_to_header


ROOT = Path(__file__).resolve().parents[2]
DONOR = Path(r"C:\Users\Mayor\Documents\Caribbean\johto-hns")


class JohtoWildRuntimeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.artifacts = register_wild.build_artifacts(DONOR)
        cls.runtime = json.loads(cls.artifacts[register_wild.RUNTIME_OUTPUT])

    def test_runtime_has_exact_tables_headers_and_fallbacks(self):
        selection = self.runtime["selection"]
        self.assertEqual(selection["runtime_header_count"], 93)
        self.assertEqual(selection["source_table_count"], 147)
        self.assertEqual(selection["night_fallback_count"], 39)
        self.assertTrue(selection["host_time_config_independent"])
        hosts = [
            (row["map_linkage"]["host"]["group"], row["map_linkage"]["host"]["index"])
            for row in self.runtime["headers"]
        ]
        self.assertEqual(len(hosts), len(set(hosts)))
        self.assertEqual({row["map"] for row in self.runtime["headers"]}, {row["map"] for row in self.runtime["tables"]})

    def test_source_vectors_and_slots_are_preserved(self):
        source = json.loads((ROOT / "data/johto/wild_encounters.json").read_text(encoding="utf-8"))
        group = source["wild_encounter_groups"][0]
        self.assertEqual(group["fields"][0]["encounter_rates"], [20, 20, 10, 10, 10, 10, 5, 5, 4, 4, 1, 1])
        self.assertEqual(group["fields"][1]["encounter_rates"], [60, 30, 5, 4, 1])
        self.assertEqual(group["fields"][2]["encounter_rates"], [60, 30, 5, 4, 1])
        self.assertEqual(group["fields"][3]["encounter_rates"], [70, 30, 60, 20, 20, 40, 40, 15, 4, 1])
        self.assertEqual(
            [len(field["encounter_rates"]) for field in group["fields"]],
            [12, 5, 5, 10],
        )
        self.assertEqual(sum(len(row["source_labels"]) for row in group["encounters"]), 149)
        self.assertTrue(any(row.get("land_mons", {}).get("encounter_rate") == 0 for row in group["encounters"]))

        by_map_time = {(row["map"], row["time_of_day"]): row for row in group["encounters"]}
        new_bark = by_map_time[("MAP_NEW_BARK_TOWN", "day")]
        self.assertEqual(new_bark["land_mons"]["encounter_rate"], 0)
        self.assertEqual(len(new_bark["land_mons"]["mons"]), 12)
        self.assertTrue(all(mon["species"] == "SPECIES_NONE" for mon in new_bark["land_mons"]["mons"]))
        self.assertEqual(new_bark["water_mons"]["encounter_rate"], 7)
        self.assertEqual(new_bark["water_mons"]["mons"][0], {
            "min_level": 25,
            "max_level": 29,
            "species": "SPECIES_TENTACOOL",
        })
        self.assertEqual(len(new_bark["water_mons"]["mons"]), 12)
        self.assertEqual(new_bark["rock_smash_mons"]["encounter_rate"], 60)
        self.assertEqual(new_bark["rock_smash_mons"]["mons"][0], {
            "min_level": 10,
            "max_level": 10,
            "species": "SPECIES_PINECO",
        })
        self.assertEqual(new_bark["fishing_mons"]["encounter_rate"], 30)
        self.assertEqual(new_bark["fishing_mons"]["mons"][0], {
            "min_level": 10,
            "max_level": 10,
            "species": "SPECIES_MAGIKARP",
        })
        self.assertEqual(len(new_bark["fishing_mons"]["mons"]), 10)

        fallbacks = source["absent_night_fallback"]
        self.assertEqual(fallbacks["count"], 39)
        fallback_maps = {item["map"] for item in fallbacks["maps"]}
        self.assertEqual(len(fallback_maps), 39)
        for map_name in fallback_maps:
            self.assertEqual(
                {row["time_of_day"] for row in group["encounters"] if row["map"] == map_name},
                {"day"},
            )

    def test_check_fails_closed_for_missing_and_stale_outputs(self):
        with tempfile.TemporaryDirectory() as directory:
            output_dir = Path(directory)
            paths = {
                register_wild.INFO_OUTPUT: output_dir / "wild_info.h",
                register_wild.HEADERS_OUTPUT: output_dir / "wild_headers.inc",
                register_wild.RUNTIME_OUTPUT: output_dir / "wild_runtime.json",
            }
            patched = {
                "INFO_OUTPUT": paths[register_wild.INFO_OUTPUT],
                "HEADERS_OUTPUT": paths[register_wild.HEADERS_OUTPUT],
                "RUNTIME_OUTPUT": paths[register_wild.RUNTIME_OUTPUT],
            }
            output_paths = tuple(paths.values())
            headers_path = patched["HEADERS_OUTPUT"]
            with mock.patch.multiple(register_wild, **patched):
                with self.assertRaises(register_wild.WildRuntimeError):
                    register_wild.run(DONOR, write=False, check=True)
                register_wild.run(DONOR, write=True, check=False)

                baseline = {path: path.read_bytes() for path in output_paths}
                for label, path in (
                    ("info", patched["INFO_OUTPUT"]),
                    ("headers", patched["HEADERS_OUTPUT"]),
                    ("runtime", patched["RUNTIME_OUTPUT"]),
                ):
                    mutated = bytearray(baseline[path])
                    mutated[0] ^= 1
                    path.write_bytes(mutated)
                    before_check = {candidate: candidate.read_bytes() for candidate in output_paths}
                    with self.assertRaisesRegex(register_wild.WildRuntimeError, "generated file drift"):
                        register_wild.run(DONOR, write=False, check=True)
                    self.assertEqual(
                        {candidate: candidate.read_bytes() for candidate in output_paths},
                        before_check,
                        f"stale {label} check mutated an output",
                    )
                    path.write_bytes(baseline[path])

                headers_path.unlink()
                with self.assertRaises(register_wild.WildRuntimeError):
                    register_wild.run(DONOR, write=False, check=True)

    def test_generator_adds_johto_includes_only_to_host_group(self):
        config = SimpleNamespace(
            times_of_day={"TIME_MORNING": "Morning", "TIME_DAY": "Day", "TIME_EVENING": "Evening", "TIME_NIGHT": "Night"},
            mon_types=list(register_wild.FIELD_ORDER),
            time_encounters=False,
            time_fallback="TIME_MORNING",
            use_firered_wild="FIRE_RED",
        )
        output = io.StringIO()
        assembler = wild_encounters_to_header.WildEncounterAssembler(output, {}, config)
        assembler.WritePokemonHeaders({"label": "gWildMonHeaders", "data": {}})
        host_text = output.getvalue()
        self.assertIn('#include "johto/wild_info.h"', host_text)
        self.assertIn('#include "johto/wild_headers.inc"', host_text)
        self.assertLess(host_text.index('#include "johto/wild_info.h"'), host_text.index("const struct WildPokemonHeader"))
        self.assertLess(host_text.index('#include "johto/wild_headers.inc"'), host_text.index("MAP_UNDEFINED"))

        output = io.StringIO()
        assembler = wild_encounters_to_header.WildEncounterAssembler(output, {}, config)
        assembler.WritePokemonHeaders({"label": "gOtherHeaders", "data": {}})
        self.assertNotIn("johto/wild_", output.getvalue())

    def test_runtime_provenance_and_world_boundary_are_truthful(self):
        self.assertTrue(self.runtime["runtime_ready"])
        self.assertEqual(self.runtime["world_status"], "unwired")
        self.assertIn("campaign traversal", self.runtime["limitation"])
        self.assertEqual(self.runtime["provenance"]["donor_revision"], register_wild.import_wild.DONOR_REVISION)


if __name__ == "__main__":
    unittest.main()
