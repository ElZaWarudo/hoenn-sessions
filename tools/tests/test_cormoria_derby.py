import pathlib
import re
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
SOURCE = ROOT / "src" / "cormoria" / "derby.c"
HEADER = ROOT / "include" / "cormoria" / "derby.h"
STATE_HEADER = ROOT / "include" / "cormoria" / "derby_state.h"
CAMPAIGN_IDS = ROOT / "include" / "constants" / "cormoria_event_ids.h"
SPECIALS = ROOT / "data" / "specials.inc"
ASSET_DIR = ROOT / "graphics" / "cormoria" / "derby"


class CormoriaDerbySourceTests(unittest.TestCase):
    """Guard the pinned Dreamstone Derby port and its Cormoria boundary."""

    def setUp(self):
        self.source = SOURCE.read_text(encoding="utf-8")
        self.header = HEADER.read_text(encoding="utf-8")
        self.specials = SPECIALS.read_text(encoding="utf-8")

    def test_is_cormoria_only_and_exports_both_derby_specials(self):
        self.assertIn('#include "cormoria/derby.h"', self.source)
        self.assertIn("#if ROM_WORLD == 2", self.source)
        self.assertIn("#endif // ROM_WORLD == 2", self.source)
        self.assertIn("void StartDerby(void);", self.header)
        self.assertIn("void GetNewDerby(void);", self.header)
        self.assertNotIn('#include "derby.h"', self.source)
        self.assertNotIn("graphics/derby/", self.source)
        cormoria_block = self.specials[self.specials.index("#if ROM_WORLD == 2"):]
        self.assertIn("def_special StartDerby", cormoria_block)
        self.assertIn("def_special GetNewDerby", cormoria_block)

    def test_real_coin_and_script_settlement_is_preserved(self):
        self.assertIn("if ((sDerby->Bet + 10) <= GetCoins())", self.source)
        self.assertIn("SetCoins((GetCoins() - sDerby->Bet));", self.source)
        self.assertIn("VarSet(GAME_CORNER_VAR_WINNINGS, sDerby->PotentialWin);", self.source)
        self.assertIn("VarSet(GAME_CORNER_VAR_WINNINGS, 0);", self.source)
        self.assertIn("CB2_ReturnToFieldContinueScriptPlayMapMusic", self.source)
        self.assertIn("gSpecialVar_Result = 0;", self.source)

    def test_authentic_derby_graphics_are_materialized(self):
        expected = {
            "betslip_bg.4bpp.lz",
            "betslip_bg.bin.lz",
            "bet_bg.gbapal",
            "betslip_bg_2.4bpp.lz",
            "race_bg.4bpp.lz",
            "race_bg.bin.lz",
            "racetrack_bg.gbapal",
            "betmenu_interface.gbapal",
            "credit.4bpp.lz",
            "creditred.4bpp.lz",
            "digits.4bpp.lz",
            "digits_2.4bpp.lz",
            "selection.4bpp.lz",
        }
        present = {path.name for path in ASSET_DIR.iterdir() if path.is_file()}
        self.assertTrue(expected <= present)
        for subdir, minimum in (("condition", 5), ("countdown", 4), ("payout", 7), ("pokemon_ui", 6), ("species_name", 4)):
            self.assertGreaterEqual(len(list((ASSET_DIR / subdir).glob("*.4bpp.lz"))), minimum)

    def test_every_cormoria_asset_reference_exists(self):
        references = re.findall(r'INCBIN_\w+\("([^"]+)"\)', self.source)
        cormoria_references = [path for path in references if path.startswith("graphics/cormoria/derby/")]
        self.assertGreater(len(cormoria_references), 50)
        self.assertEqual(len(cormoria_references), len(set(cormoria_references)))
        for reference in cormoria_references:
            self.assertTrue((ROOT / reference).is_file(), reference)

    def test_roster_uses_unoccupied_world_local_event_ids(self):
        state = STATE_HEADER.read_text(encoding="utf-8")
        campaign = CAMPAIGN_IDS.read_text(encoding="utf-8")
        variable_ordinals = [int(value) for value in re.findall(
            r"#define DERBY_VAR_\w+\s+\(WORLD_EVENT_VAR_START \+ (\d+)\)", state)]
        flag_ordinals = [int(value) for value in re.findall(
            r"#define DERBY_FLAG_\w+\s+\(WORLD_EVENT_FLAG_START \+ (\d+)\)", state)]
        campaign_vars = [int(value, 16) - 0x9100 for value in re.findall(
            r"#define Cormoria_VAR_\w+ (0x[0-9A-F]+)", campaign)]
        campaign_flags = [int(value, 16) - 0x8000 for value in re.findall(
            r"#define Cormoria_FLAG_\w+ (0x[0-9A-F]+)", campaign)]
        self.assertEqual(sorted(variable_ordinals), list(range(43, 55)))
        self.assertEqual(sorted(flag_ordinals), [484, 485])
        self.assertTrue(set(variable_ordinals).isdisjoint(campaign_vars))
        self.assertTrue(set(flag_ordinals).isdisjoint(set(campaign_flags) | {483}))
        self.assertIn('#include "cormoria/derby_state.h"', self.source)


if __name__ == "__main__":
    unittest.main()
