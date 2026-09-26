import unittest
import re
from pathlib import Path

from tools.cormoria import quest_menu_provenance


ROOT = Path(__file__).resolve().parents[2]
QUEST_MENU = ROOT / "src/cormoria/quest_menu.c"
QUEST_COMMANDS = ROOT / "src/cormoria/quest_commands.c"
START_MENU = ROOT / "src/start_menu.c"


class CormoriaQuestMenuTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.source = QUEST_MENU.read_text(encoding="utf-8")
        cls.commands = QUEST_COMMANDS.read_text(encoding="utf-8")
        cls.start_menu = START_MENU.read_text(encoding="utf-8")

    def test_pinned_donor_provenance_is_complete(self):
        self.assertEqual(
            quest_menu_provenance.DONOR_REVISION,
            "f7997186345885bfa23a170e5f573851fc034b9b",
        )
        self.assertEqual(len(quest_menu_provenance.DONOR_SOURCE_SHA256), 3)
        self.assertEqual(len(quest_menu_provenance.DONOR_ASSET_SHA256), 3)
        for digest in (*quest_menu_provenance.DONOR_SOURCE_SHA256.values(),
                       *quest_menu_provenance.DONOR_ASSET_SHA256.values()):
            self.assertRegex(digest, r"^[0-9a-f]{64}$")
        self.assertIn(quest_menu_provenance.DONOR_REVISION, self.source)
        for digest in quest_menu_provenance.DONOR_SOURCE_SHA256.values():
            self.assertIn(digest, self.source)

    def test_full_donor_content_is_present(self):
        expected_names = (
            "Lab Assistant", "Find the Dreamstone!", "Dreamstone Mysteries",
            "Food Poisoning", "A Hiker's Treasure", "A Lost Skitty",
            "Historical Preservation", "Modern Matcha", "Cyndaquil's New Move",
            "Precious Pearls", "Love Is Sacrifice", "Malevolent Masterpiece",
            "I Can't Find My Wife!", "Career Crisis", "Pokémon Ranger Badge",
            "Pelluca's Leadership Tussle", "A Chef's Icy Troubles",
            "The Healers Need Help!", "Percy's Gone Missing!", "Mean Old Grandma",
        )
        expected_subquests = (
            "My First Day", "Lab Supplies", "Missing Supplies",
            "The First Dreamstone", "Mysterious Area", "Silversun Sighting",
            "Of Drama & Desire", "Knowledge of a Past Era",
            "Showdown at Mt. Mirroh!", "Stop Melea!", "No Way Out",
            "Reach Rivetshore City", "Board the S.S. Elegant", "Get Off the Ship!",
            "Explore the Island", "A Ranger's First Assignment",
            "Fieldwork: Mega Evolution", "The Final Test", "Help the Mayor!",
            "Save the Citizens!",
        )
        for name in (*expected_names, *expected_subquests):
            self.assertIn(f'_("{name}")', self.source)
        self.assertIn("CORMORIA_QUEST_FAVORITE", self.source)
        self.assertIn("CormoriaQuestState_GetSubquest", self.source)
        self.assertIn("completedDescription", self.source)
        self.assertIn("description", self.source)
        self.assertIn("sSubquestScroll", self.source)

    def test_script_open_uses_continuation_callback(self):
        self.assertIn("CormoriaQuestMenu_Init(CB2_ReturnToFieldContinueScript)", self.commands)
        self.assertIn("ScriptContext_Stop();", self.commands)
        self.assertIn("CormoriaQuestMenu_CopyQuestName", self.commands)

    def test_start_menu_action_is_cormoria_gated_and_has_capacity(self):
        self.assertIn("sCurrentStartMenuActions[12]", self.start_menu)
        self.assertIn("MENU_ACTION_QUESTS", self.start_menu)
        self.assertIn("#if ROM_WORLD == 2", self.start_menu)
        self.assertRegex(
            self.start_menu,
            r"if \(FlagGet\(Cormoria_FLAG_SYS_QUEST_MENU_GET\) == TRUE\)\s+"
            r"AddStartMenuAction\(MENU_ACTION_QUESTS\)",
        )
        self.assertIn("CormoriaQuestMenu_Init(CB2_ReturnToFieldWithOpenMenu)", self.start_menu)

    def test_menu_has_standalone_graphics_setup_and_safe_tile_budget(self):
        self.assertIn('#include "sprite.h"', self.source)
        self.assertIn("ResetBgsAndClearDma3BusyFlags(0);", self.source)
        self.assertIn("InitBgsFromTemplates(0, sBgTemplates", self.source)
        self.assertIn("ScheduleBgCopyTilemapToVram(0);", self.source)
        self.assertNotIn("AddWindow(&sWindowTemplates", self.source)
        templates = self.source.split("static const struct WindowTemplate sWindowTemplates[] =", 1)[1].split("DUMMY_WIN_TEMPLATE", 1)[0]
        windows = []
        for block in re.findall(r"\{([^{}]+)\}", templates):
            values = dict((key, int(value)) for key, value in re.findall(
                r"\.(tilemapTop|height|width|baseBlock)\s*=\s*(\d+)", block
            ))
            if values:
                windows.append(values)
        self.assertEqual(len(windows), 3)
        for window in windows:
            self.assertGreater(window["tilemapTop"], 0)
            self.assertLess(window["tilemapTop"] + window["height"], 20)
            self.assertLessEqual(window["baseBlock"] + window["width"] * window["height"], 0x214)
        for first, second in zip(windows, windows[1:]):
            self.assertLess(first["tilemapTop"] + first["height"], second["tilemapTop"])
            self.assertLessEqual(first["baseBlock"] + first["width"] * first["height"], second["baseBlock"])
        self.assertIn("ResetTasks();", self.source)
        self.assertIn("ResetSpriteData();", self.source)
        self.assertIn("PrintText(sWindowIds[0], FONT_NORMAL, buffer, 4, 0)", self.source)
        self.assertIn("CopyWindowToVram(sWindowIds[2], COPYWIN_FULL)", self.source)

    def test_state_failures_are_visible_and_navigation_wraps_safely(self):
        self.assertIn('sTextStateUnavailable[] = _("Quest data unavailable.")', self.source)
        self.assertIn("if (!CormoriaQuestState_Get(0, CORMORIA_QUEST_UNLOCKED, &value))", self.source)
        self.assertIn("if (!CormoriaQuestState_Set(questId, CORMORIA_QUEST_FAVORITE, !favorite))", self.source)
        self.assertIn("UpdateScroll(sQuestCount, &sScroll, sCursor)", self.source)
        self.assertIn("UpdateScroll(sSubquestCount, &sSubquestScroll, sCursor)", self.source)
        self.assertNotIn("PrintText(sWindowIds[1], sTextBack", self.source)

    def test_donor_subquest_table_uses_exact_group_count(self):
        self.assertIn("#define CORMORIA_SUBQUEST_COUNT 20", (ROOT / "include/cormoria/quest_state.h").read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()
