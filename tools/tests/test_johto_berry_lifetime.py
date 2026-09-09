import hashlib
import json
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DONOR = ROOT.parent.parent.parent / "johto-hns"
if not DONOR.is_dir():
    DONOR = Path("C:/Users/Mayor/Documents/Caribbean/johto-hns")


def digest(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def lifetime_body(source):
    return source.split("void BerryTreeTimeUpdate(s32 minutes)\n", 1)[1].split("\nvoid PlantBerryTree(", 1)[0]


class BerryLifetimeTests(unittest.TestCase):
    def setUp(self):
        self.ledger = json.loads((ROOT / "data/johto/berry_lifetime.json").read_text())
        self.source = (ROOT / "src/berry.c").read_text()
        self.body = lifetime_body(self.source)

    def test_pinned_donor_source_and_policy(self):
        source = (DONOR / self.ledger["donor"]["path"]).read_text()
        self.assertEqual(digest(source), self.ledger["donor"]["normalized_sha256"])
        self.assertIn("/*case BERRY_STAGE_BERRIES:", source)
        self.assertIn("/*if (minutes >= GetStageDurationByBerryType(tree->berry) * 71)", source)
        self.assertIn("tree->stage != BERRY_STAGE_BERRIES", lifetime_body(source))

    def test_only_allocated_plots_have_lifetime_policy(self):
        header = (ROOT / "include/constants/johto_berry_plots.h").read_text()
        values = dict(re.findall(r"#define\s+(JOHTO_BERRY_PLOTS_\w+)\s+(\d+)", header))
        self.assertEqual((int(values["JOHTO_BERRY_PLOTS_FIRST"]), int(values["JOHTO_BERRY_PLOTS_LAST"])), (90, 109))
        self.assertEqual(self.ledger["runtime_plot_ids"], list(range(90, 110)))
        self.assertIn("i >= JOHTO_BERRY_PLOTS_FIRST && i <= JOHTO_BERRY_PLOTS_LAST", self.body)
        self.assertIn("if (!isJohtoPlot && (!OW_BERRY_IMMORTAL)", self.body)

    def test_nonpositive_ripe_and_gardening_guards(self):
        self.assertRegex(self.body, r"if \(isJohtoPlot && \(minutes <= 0 \|\| tree->stage == BERRY_STAGE_BERRIES\)\)\s+continue;")
        self.assertIn("while (!isJohtoPlot && time > 0", self.body)
        self.assertRegex(self.body, r"if \(tree->stage == BERRY_STAGE_BERRIES\)\s*\{[^{}]*if \(isJohtoPlot\)\s*break;")
        self.assertIn("!tree->stopGrowth", self.body)

    def test_global_growth_and_host_source_outside_clock_unchanged(self):
        outside = self.source.replace(self.body, "").replace('#include "constants/johto_berry_plots.h"\n', "")
        self.assertEqual(digest(outside), self.ledger["host_outside_clock_normalized_sha256"])

    def test_actual_engine_oracles_are_present(self):
        source = (ROOT / "test/johto/berry_lifetime.c").read_text()
        self.assertEqual(source.count('TEST("'), 4)
        for oracle in ("BerryTreeTimeUpdate(0x7FFFFFFF)", "BerryTreeTimeUpdate(-1)",
                       "BerryTreeTimeUpdate(duration - 1)", "BerryTreeTimeUpdate(1)",
                       "i == 89 || i == 110", "OW_BERRY_IMMORTAL ? 0 : 1",
                       "GetBerryInfo(BERRY_ID_ORAN)->minYield", "memcmp(gSaveBlock1Ptr->berryTrees"):
            self.assertIn(oracle, source)
        self.assertNotIn("JohtoBerry_TryHarvest", source)


if __name__ == "__main__":
    unittest.main()
