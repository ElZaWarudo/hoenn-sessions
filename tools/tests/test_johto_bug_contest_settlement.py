import hashlib
import json
import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CONTRACT_HASH = "sha256:de5ee2940eeede3083c8b9a6b37f601e1f1c2cd5fc9b27420d640ae63d485104"
OWNED_FILES = {
    "src/johto/bug_contest.c",
    "include/johto/bug_contest.h",
    "test/johto/bug_contest_settlement.c",
    "tools/tests/test_johto_bug_contest_settlement.py",
    "data/johto/bug_contest_settlement.json",
}


def normalized_digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes().replace(b"\r\n", b"\n")).hexdigest()


class JohtoBugContestSettlementTests(unittest.TestCase):
    def setUp(self):
        self.ledger = json.loads((ROOT / "data/johto/bug_contest_settlement.json").read_text())
        self.source = (ROOT / "src/johto/bug_contest.c").read_text()
        self.header = (ROOT / "include/johto/bug_contest.h").read_text()
        self.fixtures = (ROOT / "test/johto/bug_contest_settlement.c").read_text()

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
            "Settlement rejects active and unjudged contests without mutation",
            "Settlement restores once and preserves post-settlement party PC mail and bag edits",
            "Prepared settlement keeps transfer and reward retries recoverable",
            "Prepared settlement abort preserves legitimate edits and releases the fence",
            "JohtoBugContest_PrepareSettlement()",
            "JohtoBugContest_IsSerializationBlocked()",
            "CopyMonToPC(&extra)",
            "FillContestPC();",
            "FillContestPocket(POCKET_ITEMS, ITEM_POTION);",
            "ClearAllMail();",
        ):
            self.assertIn(phrase, self.fixtures)

    def test_ledger_keeps_ui_and_campaign_scope_explicit(self):
        self.assertFalse(self.ledger["ui_ready"])
        self.assertFalse(self.ledger["campaign_ready"])
        self.assertEqual(self.ledger["context_lifetime"], "through Exit or allowed Abort")
        self.assertIn("selectedMon", self.ledger["frozen_fields"])
        self.assertIn("reward", self.ledger["frozen_fields"])
        self.assertIn("full mail", self.ledger["restored_once"])
        self.assertIn("PC, party, mail, and bag edits", self.ledger["post_settlement_policy"])
        self.assertNotIn("src/pokemon_storage_system.c", self.ledger["owned_files"])


if __name__ == "__main__":
    unittest.main()
