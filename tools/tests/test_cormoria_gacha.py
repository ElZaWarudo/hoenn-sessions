import pathlib
import re
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
SOURCE = ROOT / "src" / "cormoria" / "gacha.c"
SPECIALS = ROOT / "data" / "specials.inc"
ASSET_DIR = ROOT / "graphics" / "cormoria" / "gacha"


class CormoriaGachaSourceTests(unittest.TestCase):
    """Guard the pinned Dreamstone Gacha port's world and reward boundaries."""

    def setUp(self):
        self.source = SOURCE.read_text(encoding="utf-8")
        self.specials = SPECIALS.read_text(encoding="utf-8")

    def test_is_cormoria_only_and_uses_qualified_progression_flags(self):
        self.assertIn("#if ROM_WORLD == 2", self.source)
        self.assertIn("#endif // ROM_WORLD == 2", self.source)
        self.assertIn('#include "constants/cormoria_event_ids.h"', self.source)
        self.assertNotRegex(self.source, r"FlagGet\(FLAG_BADGE0[1-8]_GET\)")
        self.assertNotIn("FlagGet(FLAG_IS_CHAMPION)", self.source)

    def test_reward_commit_is_after_capacity_and_mon_commit(self):
        preflight = self.source.index("IsPlayerPartyAndPokemonStorageFull")
        give = self.source.index("GiveCapturedMonToPlayer")
        spend = self.source.index("RemoveBagItem(ITEM_GACHA_TOKEN, 1)")
        caught = self.source.index("FLAG_SET_CAUGHT")
        self.assertLess(preflight, give)
        self.assertLess(give, spend)
        self.assertLess(spend, caught)
        self.assertNotIn("AddBagItem(ITEM_GACHA_TOKEN", self.source)
        self.assertIn("gSpecialVar_Result = 0", self.source)
        self.assertIn("gSpecialVar_Result = 1", self.source)

    def test_donor_early_return_is_not_present_in_pull_selection(self):
        start = self.source.index("void DeterminePokemonRarityAndNewStatus")
        end = self.source.index("static void AButton", start)
        pull = self.source[start:end]
        self.assertIn("RARITY_COMMON_ODDS", pull)
        self.assertIn("PickGachaSpecies", pull)
        self.assertNotRegex(
            pull,
            re.compile(r"CalculatedSpecies\s*=.*?;\s*\n\s*return;", re.S),
        )

    def test_special_is_profile_gated_with_other_cormoria_games(self):
        cormoria_block = self.specials[self.specials.index("#if ROM_WORLD == 2"):]
        self.assertIn("def_special StartGacha", cormoria_block)
        self.assertIn("def_special StartSnake", cormoria_block)
        self.assertIn("def_special Special_ViewVoltorbFlip", cormoria_block)

    def test_authentic_graphics_and_shared_trade_platform_are_present(self):
        expected = {
            "bg_middle.4bpp.lz",
            "bg_middle.bin.lz",
            "knob.4bpp.lz",
            "lottery_japan.4bpp.lz",
            "pressA.4bpp.lz",
            "numbers.4bpp.lz",
            "input_numbers.4bpp.lz",
        }
        present = {p.name for p in ASSET_DIR.iterdir()}
        self.assertTrue(expected <= present)
        self.assertIn('INCBIN_U16("graphics/trade/platform.bin")', self.source)


if __name__ == "__main__":
    unittest.main()
