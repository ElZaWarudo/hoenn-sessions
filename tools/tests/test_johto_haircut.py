import hashlib
import json
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).parents[2]
DONOR = Path(r"C:/Users/Mayor/Documents/Caribbean/johto-hns")
DONOR_REVISION = "751823abaf677020bcd72c45fe3e7cb2b8a576e4"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


class JohtoHaircutTests(unittest.TestCase):
    def test_donor_brother_one_is_plus_99_at_all_friendship_tiers(self):
        donor_specials = (DONOR / "src/field_specials.c").read_text()
        donor_pokemon = (DONOR / "src/pokemon.c").read_text()
        self.assertIn("void HaircutBrother1(void)", donor_specials)
        self.assertIn("AdjustFriendship(&gPlayerParty[gSpecialVar_0x8004], FRIENDSHIP_EVENT_HAIRCUT1)", donor_specials)
        self.assertIn("[FRIENDSHIP_EVENT_HAIRCUT1]        = {99,  99,  99}", donor_pokemon)

    def test_service_uses_host_bonus_policy_and_safe_saturation(self):
        source = (ROOT / "src/johto/haircut.c").read_text()
        self.assertIn("bool8 JohtoHaircut_Apply(u16 partySlot)", source)
        self.assertRegex(source, r"partySlot >= PARTY_SIZE.*partySlot >= gPlayerPartyCount")
        self.assertIn("MON_DATA_SPECIES_OR_EGG", source)
        self.assertIn("SPECIES_NONE || species == SPECIES_EGG", source)
        self.assertIn("ShouldSkipFriendshipChange()", source)
        self.assertIn("CalculateFriendshipBonuses(mon, 99, holdEffect)", source)
        self.assertIn("friendship > MAX_FRIENDSHIP", source)
        self.assertIn("SetMonData(mon, MON_DATA_FRIENDSHIP", source)
        self.assertIn("ITEM_ENIGMA_BERRY_E_READER", source)
        self.assertIn("GetItemHoldEffect(heldItem)", source)

    def test_special_is_appended_effects_aware_and_result_explicit(self):
        specials = (ROOT / "data/specials.inc").read_text()
        self.assertRegex(
            specials,
            r"\n\s*def_special DoSlidingPuzzle, requests_effects=1\s*\n"
            r"\s*def_special HaircutBrother1, requests_effects=1\s*\n",
        )
        source = (ROOT / "src/johto/haircut.c").read_text()
        self.assertIn("Script_RequestEffects(SCREFF_V1 | SCREFF_SAVE)", source)
        self.assertIn("gSpecialVar_Result = JohtoHaircut_Apply(gSpecialVar_0x8004)", source)
        self.assertNotIn("gSpecialVar_Result = TRUE;", source)

    def test_provenance_binds_donor_and_scope(self):
        provenance = json.loads((ROOT / "data/johto/haircut.json").read_text())
        self.assertTrue(provenance["campaign_ready"])
        self.assertEqual(provenance["source_revision"], DONOR_REVISION)
        self.assertEqual(provenance["provenance"]["donor_revision"], DONOR_REVISION)
        for item in provenance["provenance"]["source_hashes"]:
            self.assertEqual(item["sha256"], sha256(DONOR / item["source"]))
            self.assertEqual(item["bytes"], (DONOR / item["source"]).stat().st_size)
        self.assertIn("emulator", " ".join(provenance["limitations"]).lower())

    def test_production_fixture_covers_service_boundaries(self):
        fixture = (ROOT / "test/johto/haircut.c").read_text()
        for token in ("0, 99, 100, 199, 200, 254, MAX_FRIENDSHIP", "ITEM_SOOTHE_BELL", "ITEM_LUXURY_BALL", "PARTY_NOTHING_CHOSEN", "BATTLE_TYPE_FRONTIER", "HaircutBrother1"):
            self.assertIn(token, fixture)
        self.assertGreaterEqual(fixture.count("TEST("), 4)

    def test_goldenrod_consumers_bind_payment_daily_gate_and_special_in_order(self):
        campaign = (ROOT / "data/johto/campaign_scripts.inc").read_text(encoding="utf-8")
        materialized = json.loads((ROOT / "data/maps/GoldenrodCity_UndergroundTunnel/map.json").read_text(encoding="utf-8"))
        scripts = {obj["script"] for obj in materialized["object_events"]}
        for suffix, price in (("1", "500"), ("2", "300")):
            brother = f"Johto_GoldenrodCity_UndergroundTunnel_GoldenrodUnderground_EventScript_Brother_Haircut{suffix}"
            service = f"Johto_GoldenrodCity_UndergroundTunnel_GoldenrodUnderground_EventScript_Haircut{suffix}"
            flag = f"JOHTO_FLAG_DAILY_HAIRCUT{suffix}_RECEIVED"
            self.assertIn(brother, scripts)
            entry = campaign.split(service + "::", 1)[1].split("\n\n", 1)[0]
            self.assertLess(entry.index(f"checkmoney {price}, 0"), entry.index(f"removemoney {price}"))
            self.assertLess(entry.index(f"removemoney {price}"), entry.index("special HaircutBrother1"))
            self.assertLess(entry.index("special HaircutBrother1"), entry.index(f"setflag {flag}"))


if __name__ == "__main__":
    unittest.main()
