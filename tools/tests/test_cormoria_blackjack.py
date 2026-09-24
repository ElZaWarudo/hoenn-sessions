import pathlib
import re
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
SOURCE = ROOT / "src" / "cormoria" / "blackjack.c"
HEADER = ROOT / "include" / "cormoria" / "blackjack.h"
SPECIALS = ROOT / "data" / "specials.inc"
ASSET_DIR = ROOT / "graphics" / "cormoria" / "blackjack"


class CormoriaBlackjackSourceTests(unittest.TestCase):
    """Guard the pinned Dreamstone Blackjack port and its Cormoria boundary."""

    def setUp(self):
        self.source = SOURCE.read_text(encoding="utf-8")
        self.header = HEADER.read_text(encoding="utf-8")
        self.specials = SPECIALS.read_text(encoding="utf-8")

    def test_is_cormoria_only_and_exports_the_script_special(self):
        self.assertIn('#include "cormoria/blackjack.h"', self.source)
        self.assertIn("#if ROM_WORLD == 2", self.source)
        self.assertIn("#endif // ROM_WORLD == 2", self.source)
        self.assertIn("void StartBlackJack(void)", self.header)
        cormoria_block = self.specials[self.specials.index("#if ROM_WORLD == 2"):]
        self.assertIn("def_special StartBlackJack", cormoria_block)
        self.assertNotIn("graphics/blackjack/", self.source)

    def test_authentic_gameplay_keeps_coin_settlement(self):
        self.assertIn("RemoveCoins(10);", self.source)
        self.assertIn("AddCoins(10);", self.source)
        self.assertIn("SetCoins(GetCoins() - sBlackJack->betBlackJack);", self.source)
        self.assertIn("winnings = (sBlackJack->betBlackJack * 3) / 2;", self.source)
        self.assertIn("winnings = sBlackJack->betBlackJack * 2;", self.source)
        self.assertIn("AddCoins(sBlackJack->betBlackJack);", self.source)
        self.assertIn("FREE_AND_SET_NULL(sBlackJack);", self.source)

    def test_script_callback_distinguishes_completion_from_cancel(self):
        start = self.source.index("void StartBlackJack(void)")
        start_end = self.source.index("static void FadeToBJScreen", start)
        self.assertIn("gSpecialVar_Result = 0;", self.source[start:start_end])
        complete = self.source.index("static void HandleInput_BJComplete")
        complete_end = self.source.index("static void HandleInput(void)", complete)
        self.assertIn("gSpecialVar_Result = 1;", self.source[complete:complete_end])
        cancel = self.source.index("static void HandleInput(void)\n{")
        cancel_end = self.source.index("static void UpdateCardVisibility", cancel)
        self.assertIn("sBlackJack->state = BJ_STATE_START_EXIT;", self.source[cancel:cancel_end])

    def test_authentic_blackjack_graphics_are_complete(self):
        expected = {
            "background_tiles.4bpp.lz",
            "background_tiles.bin.lz",
            "background.gbapal",
            "cards.gbapal",
            "cursor.4bpp.lz",
            "digits.4bpp.lz",
            "facedown.4bpp.lz",
            "option_1.4bpp.lz",
            "option_2.4bpp.lz",
            "option_3.4bpp.lz",
            "popup.4bpp.lz",
        }
        present = {path.name for path in ASSET_DIR.iterdir() if path.is_file()}
        self.assertTrue(expected <= present)
        for suit in ("hearts", "diamonds", "clubs", "spades"):
            suit_dir = ASSET_DIR / "cards" / suit
            self.assertEqual(len(list(suit_dir.glob("*.4bpp.lz"))), 13)

    def test_every_runtime_asset_reference_is_materialized(self):
        references = re.findall(r'INCBIN_\w+\("([^"]+)"\)', self.source)
        self.assertGreater(len(references), 60)
        self.assertEqual(len(references), len(set(references)))
        for reference in references:
            self.assertTrue((ROOT / reference).is_file(), reference)


if __name__ == "__main__":
    unittest.main()
