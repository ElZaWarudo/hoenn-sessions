"""Pinned roster, identity, and fail-closed conversion checks."""

import json
import os
import shutil
import tempfile
import unittest
from pathlib import Path

from tools.coop import generate_regional_identities as registry
from tools.cormoria import import_trainers


ROOT = import_trainers.ROOT
SOURCE_ROOT = Path(os.environ["CORMORIA_TRAINER_SOURCE"]) if os.environ.get("CORMORIA_TRAINER_SOURCE") else None


class CormoriaTrainerTests(unittest.TestCase):
    def test_exact_route1_allocation_and_online_identity(self):
        source = json.loads((ROOT / "data/coop/regional_identities.json").read_text())
        entries = [entry for entry in source["identities"]
                   if entry["kind"] == "trainer" and entry["id"].startswith("CORMORIA:")]
        self.assertEqual(len(entries), 194)
        self.assertNotIn("CORMORIA:TRAINER_NONE", {entry["id"] for entry in entries})
        route = next(entry for entry in entries if entry["id"] == "CORMORIA:TRAINER_ROUTE1_A")
        self.assertEqual(route["legacy_value"], 0x505E)
        self.assertEqual(route["legacy_symbol"], "Cormoria_TRAINER_ROUTE1_A")
        self.assertEqual(route["ordinal"], 856 + 0x5E - 1)
        self.assertEqual(len({entry["legacy_value"] for entry in entries}), 194)
        self.assertEqual(source["registry_version"], 3)
        registry.validate_registry(source)

    def test_namespace_validator_rejects_unqualified_cormoria_legacy_symbol(self):
        source = json.loads((ROOT / "data/coop/regional_identities.json").read_text())
        row = next(entry for entry in source["identities"]
                   if entry["id"] == "CORMORIA:TRAINER_ROUTE1_A")
        row["legacy_symbol"] = "TRAINER_ROUTE1_A"
        with self.assertRaisesRegex(registry.RegistryError, "Cormoria_TRAINER_"):
            registry.validate_registry(source)

    def test_unknown_donor_fields_and_music_rejected(self):
        body = "{ .trainerClass = TRAINER_CLASS_YOUNGSTER, .trainerPic = TRAINER_PIC_YOUNGSTER, .encounterMusic_gender = TRAINER_ENCOUNTER_MUSIC_MALE, .doubleBattle = FALSE, .partySize = 1, .party = (const struct TrainerMon[]){{.species = SPECIES_RATTATA}}, }"
        self.assertIn(".battleType = TRAINER_BATTLE_TYPE_SINGLES", import_trainers._transform(body, "TRAINER_X"))
        with self.assertRaises(import_trainers.ImportErrorStrict):
            import_trainers._transform(body.replace(".partySize", ".unknownField"), "TRAINER_X")
        with self.assertRaises(import_trainers.ImportErrorStrict):
            import_trainers._transform(body.replace("TRAINER_ENCOUNTER_MUSIC_MALE", "UNREVIEWED_MUSIC"), "TRAINER_X")


@unittest.skipUnless(SOURCE_ROOT and (SOURCE_ROOT / import_trainers.SOURCE).is_file(),
                     "set CORMORIA_TRAINER_SOURCE to the authenticated staged donor source root")
class CormoriaPinnedTrainerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.records = import_trainers.load_records(SOURCE_ROOT)

    def test_all_records_and_route1_easy_normal(self):
        self.assertEqual(len(self.records), 195)
        self.assertEqual(sum(len(row[2]) for row in self.records), 388)
        route = self.records[0x5E]
        self.assertEqual(route[1], "TRAINER_ROUTE1_A")
        self.assertEqual(set(route[2]), {"DIFFICULTY_EASY", "DIFFICULTY_NORMAL"})
        for difficulty, iv, ai in (
            ("DIFFICULTY_EASY", "TRAINER_PARTY_IVS(0, 0, 0, 0, 0, 0)", "AI_FLAG_BASIC_TRAINER"),
            ("DIFFICULTY_NORMAL", "TRAINER_PARTY_IVS(31, 31, 31, 31, 31, 31)", "AI_FLAG_SMART_TRAINER"),
        ):
            body = route[2][difficulty]
            self.assertIn('.trainerName = _("Jose")', body)
            self.assertIn("SPECIES_RATTATA_ALOLA", body)
            self.assertIn(iv, body)
            self.assertIn(ai, body)
        self.assertEqual(set(self.records[0x44][2]), {"DIFFICULTY_NORMAL"})
        self.assertEqual(set(next(row for row in self.records if row[1] == "TRAINER_ROUTE8_E")[2]),
                         {"DIFFICULTY_EASY"})

    def test_generated_header_is_deterministic_and_current(self):
        rendered = import_trainers.render(self.records)
        self.assertEqual(rendered, import_trainers.render(import_trainers.load_records(SOURCE_ROOT)))
        self.assertEqual((ROOT / "src/data/cormoria/trainers.h").read_text(encoding="utf-8"), rendered)
        self.assertNotIn("#line", rendered)
        self.assertEqual(import_trainers.run(SOURCE_ROOT, ROOT, check=True), 0)

    def test_boss_party_and_battle_metadata_are_retained(self):
        boss = next(row for row in self.records if row[1] == "TRAINER_CHAMPIONSHIP_E")
        easy = boss[2]["DIFFICULTY_EASY"]
        self.assertIn("TRAINER_CLASS_KOHLA_FINAL", easy)
        self.assertIn("MUGSHOT_COLOR_GREEN", easy)
        self.assertIn("SPECIES_SAMUROTT_HISUI", easy)
        self.assertIn("ITEM_LEFTOVERS", easy)
        self.assertIn("ABILITY_SHARPNESS", easy)
        self.assertIn("TRAINER_PARTY_EVS(252, 0, 116, 0, 0, 140)", easy)
        self.assertIn("MOVE_FLIP_TURN", easy)
        self.assertIn("MOVE_SACRED_SWORD", easy)
        self.assertTrue(any(".startingStatus = { .rainbowOpponent = TRUE }" in body
                            for _, _, records in self.records for body in records.values()))
        doubles = [row for row in self.records if any(
            "TRAINER_BATTLE_TYPE_DOUBLES" in body for body in row[2].values())]
        self.assertTrue(doubles)

    def test_source_tamper_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / import_trainers.SOURCE
            path.parent.mkdir(parents=True)
            shutil.copyfile(SOURCE_ROOT / import_trainers.SOURCE, path)
            raw = path.read_bytes()
            path.write_bytes(raw.replace(b'SPECIES_RATTATA_ALOLA', b'SPECIES_RATTATA_HISUI', 1))
            with self.assertRaisesRegex(import_trainers.ImportErrorStrict, "authenticated donor bytes"):
                import_trainers.load_records(Path(temporary))


if __name__ == "__main__":
    unittest.main()
