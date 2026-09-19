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
        self.assertEqual(selection["approved_map_count"], 407)
        self.assertEqual(selection["runtime_header_count"], 145)
        self.assertEqual(selection["source_header_count"], 236)
        self.assertEqual(selection["source_table_count"], 234)
        self.assertEqual(selection["night_fallback_count"], 56)
        self.assertTrue(selection["host_time_config_independent"])
        hosts = [
            (row["map_linkage"]["host"]["group"], row["map_linkage"]["host"]["index"])
            for row in self.runtime["headers"]
        ]
        self.assertEqual(len(hosts), len(set(hosts)))
        self.assertEqual({group for group, _index in hosts}, {75, 76, 77, 78})
        self.assertEqual({row["map"] for row in self.runtime["headers"]}, {row["map"] for row in self.runtime["tables"]})
        later = [row for row in self.runtime["headers"] if row["map_linkage"]["host"]["group"] in (77, 78)]
        self.assertTrue(later)
        self.assertTrue(all(row["map_linkage"]["host"]["map_id"].startswith("MAP_KANTO_LATER_") for row in later))

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
        self.assertEqual(sum(len(row["source_labels"]) for row in group["encounters"]), 236)
        self.assertEqual(
            sum(
                len(row["source_labels"])
                for row in group["encounters"]
                if "water_mons" in row and len(row["water_mons"]["mons"]) == 10
            ),
            16,
        )
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
        self.assertEqual(fallbacks["count"], 56)
        fallback_maps = {item["map"] for item in fallbacks["maps"]}
        self.assertEqual(len(fallback_maps), 56)
        for map_name in fallback_maps:
            self.assertEqual(
                {row["time_of_day"] for row in group["encounters"] if row["map"] == map_name},
                {"day"},
            )

    def test_every_water_slot_has_explicit_nonzero_runtime_reachability(self):
        source = json.loads((ROOT / "data/johto/wild_encounters.json").read_text(encoding="utf-8"))
        source_rows = {
            (row["map"], row["time_of_day"]): row
            for row in source["wild_encounter_groups"][0]["encounters"]
        }
        water_tables = [row for row in self.runtime["tables"] if "water_selection" in row]
        self.assertEqual(len(water_tables), sum("water_mons" in row for row in source_rows.values()))
        self.assertEqual({row["water_selection"]["slot_count"] for row in water_tables}, {5, 10, 12})
        for row in water_tables:
            accepted = source_rows[(row["map"], row["time_of_day"])]["water_mons"]["mons"]
            selection = row["water_selection"]
            self.assertEqual(selection["slot_count"], len(accepted))
            self.assertEqual(len(selection["weights"]), len(accepted))
            self.assertTrue(all(weight > 0 for weight in selection["weights"]))
            self.assertEqual(
                selection["mode"],
                "weighted" if len(accepted) == 5 else "uniform_explicit_slots",
            )

        info_text = self.artifacts[register_wild.INFO_OUTPUT]
        self.assertIn("struct JohtoWildWaterSlotCount", info_text)
        self.assertIn("JohtoWild_GetWaterSlotCount", info_text)
        self.assertEqual(
            info_text.count("{ &sJohtoWild_") ,
            sum(row["water_selection"]["slot_count"] != 5 for row in water_tables),
        )

    def test_dynamic_water_consumers_cover_all_accepted_widths(self):
        source = json.loads((ROOT / "data/johto/wild_encounters.json").read_text(encoding="utf-8"))
        water_rows = [
            row["water_mons"]["mons"]
            for row in source["wild_encounter_groups"][0]["encounters"]
            if "water_mons" in row
        ]
        self.assertEqual({len(mons) for mons in water_rows}, {5, 10, 12})
        self.assertLessEqual(
            max(len({mon["species"] for mon in mons if mon["species"] != "SPECIES_NONE"}) for mons in water_rows),
            5,
            "the five-column DexNav water row must fit every distinct accepted species",
        )

        dexnav = (ROOT / "src/dexnav.c").read_text(encoding="utf-8")
        pokedex_area = (ROOT / "src/pokedex_area_screen.c").read_text(encoding="utf-8")
        match_call = (ROOT / "src/match_call.c").read_text(encoding="utf-8")
        constants = (ROOT / "include/constants/wild_encounter.h").read_text(encoding="utf-8")
        self.assertIn("enum Species waterSpecies[WATER_WILD_COUNT_MAX]", dexnav)
        self.assertGreaterEqual(dexnav.count("GetWaterWildMonCount(waterMonsInfo)"), 3)
        self.assertNotIn("GetWaterEncounterSlot", match_call)
        self.assertIn("ChooseWildMonIndex_WaterWithRoll(waterMonsInfo, Random(), FALSE)", match_call)
        self.assertIn("GetWaterWildMonCount(info->waterMonsInfo)", pokedex_area)
        self.assertIn("#define WATER_WILD_COUNT_MAX 12", constants)

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
        self.assertEqual(self.runtime["world_status"], "authoritative_407_map_world")
        self.assertEqual(self.runtime["world_identity"], "johto_and_later_kanto")
        self.assertEqual(self.runtime["provenance"]["donor_revision"], register_wild.import_wild.DONOR_REVISION)


if __name__ == "__main__":
    unittest.main()
