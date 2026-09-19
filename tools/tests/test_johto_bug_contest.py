import hashlib
import json
import re
import unittest
from pathlib import Path


ROOT = Path(__file__).parents[2]
DONOR = Path(r"C:/Users/Mayor/Documents/Caribbean/johto-hns")


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


class JohtoBugContestSourceTests(unittest.TestCase):
    def setUp(self):
        self.source = (ROOT / "src/johto/bug_contest.c").read_text()
        self.header = (ROOT / "include/johto/bug_contest.h").read_text()

    def test_public_lifecycle_api_is_declared(self):
        for name in (
            "Begin", "IsActive", "IsEnding", "CheckTime", "RequestEnd",
            "Judge", "TransferSelected", "ClaimReward", "Exit", "Abort",
        ):
            self.assertIn(f"JohtoBugContest_{name}", self.header)
        self.assertIn("JOHTO_BUG_CONTEST_TIME_LIMIT_FRAMES (60u * 60u * 8u)", self.header)
        self.assertIn("EWRAM_DATA struct JohtoBugContestState *sBugContest", self.source)

    def test_donor_scoring_and_rewards_are_bound(self):
        donor = (DONOR / "src/bug_contest.c").read_text()
        for token in ("maxHP < 41", "maxHP <= 46", "maxHP <= 47", "Random() % 100"):
            self.assertIn(token, donor)
        for token in (
            "ITEM_MOON_STONE", "ITEM_SUN_STONE", "ITEM_LEAF_STONE",
            "ITEM_FIRE_STONE", "ITEM_THUNDER_STONE", "ITEM_WATER_STONE",
            "ITEM_ORAN_BERRY", "ITEM_CHERI_BERRY", "ITEM_PERSIM_BERRY",
            "ITEM_PECHA_BERRY", "ITEM_RAWST_BERRY", "ITEM_ASPEAR_BERRY",
            "ITEM_CHESTO_BERRY",
        ):
            self.assertIn(token, self.source)
        self.assertIn("u32 maxHp", self.source)

    def test_transaction_and_retry_guards_are_source_bound(self):
        for token in (
            "CheckBagHasSpace(ITEM_SAFARI_BALL, 30)",
            "AddBagItem(ITEM_SAFARI_BALL, state->loanedSafariBalls)",
            "GetFirstFreeBoxSpot(box)",
            "CopyMonToPC(&sBugContest->selectedMon)",
            "if (sBugContest->transferred)",
            "if (!sBugContest->transferred)",
            "if (sBugContest->rewardClaimed)",
            "ReturnLoanedSafariBalls",
            "originalMail[MAIL_COUNT]",
        ):
            self.assertIn(token, self.source)
        self.assertNotIn("SavePlayerParty", self.source)
        self.assertNotIn("LoadPlayerParty", self.source)
        self.assertNotIn("FLAG_ADVENTURE_STARTED", self.source)

    def test_raw_source_and_provenance(self):
        donor_file = DONOR / "src/bug_contest.c"
        provenance = json.loads((ROOT / "data/johto/bug_contest.json").read_text())
        self.assertTrue(provenance["campaign_ready"])
        self.assertTrue(provenance["engine_hooks_ready"])
        self.assertEqual(provenance["provenance"]["donor_revision"], "751823abaf677020bcd72c45fe3e7cb2b8a576e4")
        self.assertEqual(provenance["provenance"]["donor_source_sha256"], digest(donor_file))
        normalized = (ROOT / "src/johto/bug_contest.c").read_bytes().replace(b"\r\n", b"\n")
        self.assertEqual(provenance["provenance"]["target_source_sha256"], hashlib.sha256(normalized).hexdigest())
        for source in provenance["provenance"]["source_scripts"]:
            self.assertTrue((DONOR / source).is_file(), source)

    def test_public_selection_and_species_guards(self):
        self.assertIn("slot == 0 || slot >= gPlayerPartyCount || slot >= PARTY_SIZE", self.source)
        self.assertIn("!JohtoBugContest_IsContestSpecies(species)", self.source)
        self.assertIn("GetMonData(&gPlayerParty[slot], MON_DATA_IS_EGG)", self.source)
        self.assertIn("selectedDisplayIndex", self.source)
        self.assertRegex(self.source, re.compile(r"case SPECIES_CATERPIE:.*case SPECIES_PINSIR:", re.S))


if __name__ == "__main__":
    unittest.main()
