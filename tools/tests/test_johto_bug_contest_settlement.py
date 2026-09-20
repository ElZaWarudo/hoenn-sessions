import hashlib
import json
import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CONTRACT_HASH = "sha256:7f07de4ea3c528133997c0e6d84d7d26e7d3f98870ff46a57793baaa61e14cc8"
OWNED_FILES = {
    "src/johto/bug_contest.c",
    "include/johto/bug_contest.h",
    "test/johto/bug_contest.c",
    "test/johto/bug_contest_settlement.c",
    "tools/tests/test_johto_bug_contest_settlement.py",
    "data/johto/bug_contest.json",
    "data/johto/bug_contest_settlement.json",
}


def normalized_digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes().replace(b"\r\n", b"\n")).hexdigest()


class JohtoBugContestSettlementTests(unittest.TestCase):
    def setUp(self):
        self.ledger = json.loads((ROOT / "data/johto/bug_contest_settlement.json").read_text())
        self.source = (ROOT / "src/johto/bug_contest.c").read_text()
        self.header = (ROOT / "include/johto/bug_contest.h").read_text()
        self.fixtures = (ROOT / "test/johto/bug_contest.c").read_text()
        self.settlement_fixtures = (ROOT / "test/johto/bug_contest_settlement.c").read_text()

    def test_contract_and_source_hashes_are_pinned(self):
        self.assertEqual(self.ledger["contract_hash"], CONTRACT_HASH)
        self.assertEqual(set(self.ledger["owned_files"]), OWNED_FILES)
        hashable_files = OWNED_FILES - {"data/johto/bug_contest_settlement.json"}
        self.assertEqual(set(self.ledger["source_hashes"]), hashable_files)
        for relative_path in hashable_files:
            self.assertEqual(
                self.ledger["source_hashes"][relative_path],
                normalized_digest(ROOT / relative_path),
                relative_path,
            )

    def test_one_time_settlement_boundary_is_declared_and_guarded(self):
        self.assertIn("JohtoBugContest_PrepareSettlement", self.header)
        self.assertRegex(self.source, r"bool8 settlementPrepared;")
        self.assertRegex(
            self.source,
            re.compile(r"JohtoBugContest_PrepareSettlement\(void\).*?"
            r"settlementPrepared.*?RestoreOriginalParty\(\);.*?"
            r"ReturnLoanedSafariBalls\(\);.*?settlementPrepared = TRUE", re.S),
        )
        self.assertIn("if (!sBugContest->settlementPrepared)", self.source)
        self.assertIn("even after recovery restores the live party", self.source)

    def test_recovery_fixtures_cover_retries_and_fence(self):
        for phrase in (
            "Contest settlement rejects wrong order without mutation",
            "Contest full PC and reward bag failures retain recoverable state",
            "Contest reward can be explicitly forfeited without implicit bag mutation",
            "Contest transfer is exactly once and preserves held item and checksum",
            "JohtoBugContest_PrepareSettlement()",
            "JohtoBugContest_IsSerializationBlocked()",
            "FillContestPC();",
            "FillContestPocket(POCKET_ITEMS, ITEM_POTION);",
            "JohtoBugContest_ForfeitReward()",
        ):
            self.assertIn(phrase, self.fixtures)
        for phrase in (
            "Settlement rejects active and unjudged contests without mutation",
            "Settlement restores once and preserves post-settlement party PC mail and bag edits",
            "Prepared settlement keeps transfer and reward retries recoverable",
            "Prepared settlement abort preserves legitimate edits and releases the fence",
        ):
            self.assertIn(phrase, self.settlement_fixtures)

    def test_recovery_status_and_exit_contract_is_declared(self):
        self.assertIn("JOHTO_BUG_CONTEST_NOT_PREPARED", self.header)
        self.assertIn("JOHTO_BUG_CONTEST_REWARD_FORFEITED", self.header)
        self.assertRegex(self.source, r"bool8 rewardForfeited;")
        self.assertIn("(!sBugContest->rewardClaimed && !sBugContest->rewardForfeited)", self.source)

    def test_ledger_records_complete_ui_and_campaign_binding(self):
        self.assertTrue(self.ledger["ui_ready"])
        self.assertTrue(self.ledger["campaign_ready"])
        self.assertIn("retry", self.ledger["campaign_binding"])
        self.assertIn("explicit reward forfeit", self.ledger["campaign_binding"])
        self.assertEqual(self.ledger["context_lifetime"], "through Exit; Abort remains allowed only before transfer")
        self.assertIn("selectedMon", self.ledger["frozen_fields"])
        self.assertIn("reward", self.ledger["frozen_fields"])
        self.assertIn("full mail", self.ledger["restored_once"])
        self.assertIn("PC, party, mail, and bag edits", self.ledger["post_settlement_policy"])
        self.assertIn("explicit ForfeitReward", self.ledger["reward_policy"])
        self.assertIn("ending, judged and prepared", self.ledger["transfer_policy"])
        self.assertNotIn("src/pokemon_storage_system.c", self.ledger["owned_files"])


if __name__ == "__main__":
    unittest.main()
