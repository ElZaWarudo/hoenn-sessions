import hashlib
import json
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DONOR = ROOT.parent.parent.parent / "johto-hns"
if not DONOR.is_dir():
    DONOR = Path("C:/Users/Mayor/Documents/Caribbean/johto-hns")


def blocks(source):
    result = {}
    current = None
    for raw in source.splitlines():
        line = raw.strip()
        if not line or line.startswith("@"):
            continue
        if re.fullmatch(r"\w+:{1,2}", line):
            current = line.rstrip(":")
            result[current] = []
        elif current:
            result[current].append(line)
    return result


class WhirlpoolTests(unittest.TestCase):
    def setUp(self):
        self.ledger = json.loads((ROOT / "data/johto/field_moves.json").read_text())
        self.script = blocks((ROOT / "data/scripts/johto_field_moves.inc").read_text())

    def test_pinned_source_and_explicit_party_correction(self):
        source = (DONOR / self.ledger["donor"]["path"]).read_text()
        self.assertEqual(hashlib.sha256(source.encode()).hexdigest(), self.ledger["donor"]["normalized_sha256"])
        self.assertIn("@checkpartymove MOVE_WHIRLPOOL", source)
        self.assertFalse(self.ledger["campaign_ready"])
        self.assertEqual(self.ledger["external_aliases"], {"EventScript_Whirlpool": "Johto_EventScript_Whirlpool"})

    def test_four_crossings_exactly_match_donor(self):
        donor = blocks((DONOR / self.ledger["donor"]["path"]).read_text())
        for direction, step in (("North", "up"), ("South", "down"), ("East", "right"), ("West", "left")):
            movement = "Movement_Whirlpool" + direction
            self.assertEqual(self.script["Johto_" + movement], ["slide_" + step] * 3 + ["step_end"])
            self.assertEqual(self.script["Johto_" + movement], donor[movement])
            crossing = "EventScript_WhirlpoolGo" + direction
            self.assertEqual(self.script["Johto_" + crossing], [
                "applymovement OBJ_EVENT_ID_PLAYER, Johto_" + movement,
                "waitmovement 0", "releaseall", "end"])

    def test_complete_gate_and_release_paths(self):
        entry = self.script["Johto_EventScript_Whirlpool"]
        self.assertEqual(entry[:5], ["lockall",
            "goto_if_unset JOHTO_FLAG_BADGE07_GET, Johto_EventScript_CantWhirlpool",
            "callnative Script_JohtoCheckWhirlpool, requests_effects=1",
            "goto_if_eq VAR_RESULT, PARTY_SIZE, Johto_EventScript_CantWhirlpool",
            "playse SE_M_WATERFALL"])
        self.assertEqual(entry[-2:], ["releaseall", "end"])
        for direction in ("North", "South", "East", "West"):
            self.assertIn("goto_if_eq VAR_FACING, DIR_" + direction.upper() + ", Johto_EventScript_WhirlpoolGo" + direction, entry)
        self.assertEqual(self.script["Johto_EventScript_CantWhirlpool"], [
            "msgbox Johto_Text_CantWhirlpool, MSGBOX_DEFAULT", "closemessage", "releaseall", "end"])
        donor = blocks((DONOR / self.ledger["donor"]["path"]).read_text())
        self.assertEqual(self.script["Johto_Text_CantWhirlpool"], donor["Text_CantWhirlpool"])

    def test_linkage_and_real_party_native(self):
        assembly = (ROOT / "data/event_scripts.s").read_text()
        self.assertEqual(assembly.count('.include "data/scripts/johto_field_moves.inc"'), 1)
        self.assertIn('#include "constants/johto_content.h"', assembly)
        for existing in ("data/maps/NewBarkTown/scripts.inc", "data/scripts/johto_berry_tree.inc"):
            self.assertIn('.include "' + existing + '"', assembly)
        source = (ROOT / "src/johto/field_moves.c").read_text()
        for required in ("count > PARTY_SIZE", "count = PARTY_SIZE", "MON_DATA_SPECIES", "SPECIES_NONE", "MON_DATA_IS_EGG", "MAX_MON_MOVES", "MON_DATA_MOVE1 + move", "MOVE_WHIRLPOOL", "Script_RequestEffects(SCREFF_V1)", "gPlayerPartyCount"):
            self.assertIn(required, source)
        self.assertNotIn("MON_DATA_HP", source)
        self.assertNotIn("FIELD_MOVE_DIVE", source)
        self.assertLess(source.index("Script_RequestEffects(SCREFF_V1)"), source.index("gSpecialVar_Result ="))


if __name__ == "__main__":
    unittest.main()
