import hashlib
import json
import re
import unittest
from pathlib import Path


ROOT = Path(__file__).parents[2]
DONOR = Path(r"C:/Users/Mayor/Documents/Caribbean/johto-hns")
DONOR_REVISION = "751823abaf677020bcd72c45fe3e7cb2b8a576e4"
EXPECTED = [
    ("FLAG_DAILY_BUG_CONTEST_COMPLETED", "JOHTO_FLAG_DAILY_BUG_CONTEST_COMPLETED", "DAILY_FLAGS_START + 0x5", 24608),
    ("FLAG_DAILY_HAIRCUT1_RECEIVED", "JOHTO_FLAG_DAILY_HAIRCUT1_RECEIVED", "DAILY_FLAGS_START + 0x3", 24609),
    ("FLAG_DAILY_HAIRCUT2_RECEIVED", "JOHTO_FLAG_DAILY_HAIRCUT2_RECEIVED", "DAILY_FLAGS_START + 0x4", 24610),
    ("FLAG_DAILY_PICKED_LOTO_TICKET", "JOHTO_FLAG_DAILY_PICKED_LOTO_TICKET", "DAILY_FLAGS_START + 0xA", 24611),
]


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


class JohtoDailyEventsTests(unittest.TestCase):
    def test_selected_donor_daily_identities_are_exactly_the_relocated_four(self):
        symbols = json.loads((ROOT / "data/johto/content_symbols.json").read_text(encoding="utf-8"))
        by_symbol = {entry["symbol"]: entry for entry in symbols["identities"]["flags"]}
        actual = []
        for symbol, qualified, expression, runtime_id in EXPECTED:
            entry = by_symbol[symbol]
            expression = entry["definition"]["expression"].split("//")[0].strip().strip("()")
            actual.append((symbol, entry["qualified"], expression, entry["runtime_id"]))
        self.assertEqual(actual, EXPECTED)
        self.assertEqual(
            [entry["symbol"] for entry in symbols["identities"]["flags"] if "DAILY_" in entry["symbol"]],
            [item[0] for item in EXPECTED],
        )

    def test_source_clears_only_generated_qualified_flags_and_wires_host_hook(self):
        source = (ROOT / "src/johto/daily_events.c").read_text(encoding="utf-8")
        event_data = (ROOT / "src/event_data.c").read_text(encoding="utf-8")
        for _, qualified, _, _ in EXPECTED:
            self.assertIn(qualified, source)
        self.assertEqual(source.count("JohtoEvent_SetFlag"), 1)
        self.assertIn('#include "constants/johto_content.h"', source)
        self.assertIn('#include "johto/daily_events.h"', event_data)
        self.assertRegex(event_data, r"memset\(&gSaveBlock1Ptr->flags\[DAILY_FLAGS_START / 8\], 0, DAILY_FLAGS_SIZE\);\s+JohtoDailyEvents_ClearFlags\(\);")
        self.assertEqual(event_data.count("JohtoDailyEvents_ClearFlags();"), 1)
        self.assertNotRegex(source, r"0x[0-9A-Fa-f]+")

    def test_provenance_binds_pinned_donor_bytes_hashes_and_symbols(self):
        provenance = json.loads((ROOT / "data/johto/daily_events.json").read_text(encoding="utf-8"))
        self.assertFalse(provenance["campaign_ready"])
        self.assertFalse(provenance["engine_hooks_ready"])
        self.assertEqual(provenance["source_revision"], DONOR_REVISION)
        self.assertEqual(provenance["provenance"]["donor_revision"], DONOR_REVISION)
        for item in provenance["provenance"]["source_hashes"]:
            donor_path = DONOR / item["source"]
            self.assertEqual(item["sha256"], sha256(donor_path))
            self.assertEqual(item["bytes"], donor_path.stat().st_size)
        self.assertEqual(
            [item["symbol"] for item in provenance["selected_daily_flags"]],
            [item[0] for item in EXPECTED],
        )
        self.assertEqual(
            [item["source_expression"] for item in provenance["selected_daily_flags"]],
            [item[2] for item in EXPECTED],
        )
        self.assertEqual(
            [item["current_id"] for item in provenance["selected_daily_flags"]],
            [item[1] for item in EXPECTED],
        )

    def test_production_fixture_covers_isolation_and_idempotency(self):
        fixture = (ROOT / "test/johto/daily_events.c").read_text(encoding="utf-8")
        for token in (
            "ClearDailyFlags();",
            "JohtoDailyEvents_ClearFlags();",
            "FLAG_DAILY_CONTEST_LOBBY_RECEIVED_BERRY",
            "FLAG_DAILY_PICKED_LOTO_TICKET",
            "FLAG_SYS_POKEDEX_GET",
            "JOHTO_FLAG_COMPLETED_HOOH_PUZZLE",
            "JOHTO_VAR_BUG_CONTEST_STATE",
        ):
            self.assertIn(token, fixture)
        self.assertGreaterEqual(fixture.count("TEST("), 2)


if __name__ == "__main__":
    unittest.main()
