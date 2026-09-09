import hashlib
import json
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).parents[2]
DONOR = Path("C:/Users/Mayor/Documents/Caribbean/johto-hns")


class ScriptTextTests(unittest.TestCase):
    def test_three_donor_choices_supply_johto_species_variable(self):
        source = (DONOR / "data/maps/NewBarkTown_Lab/scripts.inc").read_text()
        self.assertEqual(len(re.findall(r"^\s*buffermoncategory STR_VAR_2, PLAYER_STARTER_SPECIES\s*$", source, re.M)), 3)
        self.assertRegex(source, r"\.set\s+PLAYER_STARTER_SPECIES,\s*VAR_TEMP_2")
        for species in ("CHIKORITA", "CYNDAQUIL", "TOTODILE"):
            self.assertRegex(source, re.compile(r"setvar PLAYER_STARTER_SPECIES, SPECIES_" + species + r"(?:(?!\bend\b).)*buffermoncategory STR_VAR_2, PLAYER_STARTER_SPECIES", re.S))
        donor = (DONOR / "src/scrcmd.c").read_text()
        native = donor.split("bool8 ScrCmd_buffermoncategory(", 1)[1].split("bool8 ScrCmd_bufferleadmonspeciesname", 1)[0]
        self.assertIn("VarSet(VAR_TEMP_2, species)", native)
        self.assertIn("GetPokedexCategoryName(SpeciesToNationalPokedexNum(species))", native)

    def test_macro_layout_and_effects_marker(self):
        macro = (ROOT / "asm/macros/johto_text.inc").read_text()
        self.assertRegex(macro, r"\.macro johto_buffermoncategory stringvar:req, species:req\s+callnative Script_JohtoBufferMonCategory, requests_effects=1\s+\.byte \\stringvar\s+\.2byte \\species\s+\.endm")

    def test_native_consumes_and_validates_before_bare_copy(self):
        source = (ROOT / "src/johto/script_text.c").read_text()
        self.assertIn("VarGet(ScriptReadHalfword(ctx))", source)
        self.assertIn("Script_RequestEffects(SCREFF_V1)", source)
        self.assertLess(source.index("ScriptReadHalfword"), source.index("if (destination"))
        self.assertIn("destination >= ARRAY_COUNT(buffers)", source)
        self.assertIn("species == SPECIES_NONE || species >= NUM_SPECIES", source)
        self.assertIn("buffers[destination][0] = EOS", source)
        self.assertIn("StringCopy(buffers[destination], GetSpeciesCategory(species))", source)
        for forbidden in ("GetStarterPokemon", "CopyMonCategoryText", "VarSet(", "FlagSet(", "gPlayerParty", "SCREFF_SAVE"):
            self.assertNotIn(forbidden, source)

    def test_provenance(self):
        data = json.loads((ROOT / "data/johto/script_text.json").read_text())
        self.assertFalse(data["campaign_ready"])
        self.assertEqual(data["donor_revision"], "751823abaf677020bcd72c45fe3e7cb2b8a576e4")
        self.assertEqual({x["path"] for x in data["source_hashes"]}, {"src/scrcmd.c", "data/maps/NewBarkTown_Lab/scripts.inc"})
        for entry in data["source_hashes"]:
            raw = (DONOR / entry["path"]).read_bytes().replace(b"\r\n", b"\n")
            self.assertEqual(hashlib.sha256(raw).hexdigest(), entry["sha256"])


if __name__ == "__main__":
    unittest.main()
