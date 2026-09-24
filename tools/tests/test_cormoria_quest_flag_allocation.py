"""Keep donor quest state out of the campaign's ordinary event-ID allocation."""

import json
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
LEDGER = ROOT / "data/cormoria/symbol_ledger.json"
QUEST_FIRST_ID = 0x8000 + 0xF00
QUEST_LAST_ID = QUEST_FIRST_ID + 20 * 5 + 21 - 1


class CormoriaQuestFlagAllocationTests(unittest.TestCase):
    def test_campaign_flags_do_not_occupy_reserved_quest_window(self) -> None:
        ledger = json.loads(LEDGER.read_text(encoding="utf-8"))
        collisions = [row for row in ledger["flags"]
                      if QUEST_FIRST_ID <= row["target_id"] <= QUEST_LAST_ID]
        self.assertEqual(collisions, [], "campaign flag allocation overlaps quest state")


if __name__ == "__main__":
    unittest.main()
