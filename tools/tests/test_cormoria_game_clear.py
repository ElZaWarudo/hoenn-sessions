"""Exercise GameClear's regional first/repeat classification in both ROM worlds."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SOURCE = (ROOT / "src/post_battle_event_funcs.c").read_text(encoding="utf-8")


class GameClearFlagTests(unittest.TestCase):
    def test_first_and_repeat_clear_ignore_other_regions_flag(self) -> None:
        compiler = shutil.which("gcc") or shutil.which("clang")
        if compiler is None:
            self.skipTest("host C compiler unavailable")

        # Compile the actual flag-selection and classification statements with
        # small FlagGet/FlagSet stubs, for both compile-time ROM variants.
        selector = SOURCE.split('#if ROM_WORLD == 2\n#include "cormoria/heal_locations.h"', 1)[1]
        selector = "#if ROM_WORLD == 2\n" + selector.split("\nint GameClear(void)", 1)[0]
        selector = "\n".join(line for line in selector.splitlines() if not line.startswith("#include"))
        body = SOURCE.split("    if (FlagGet(REGION_GAME_CLEAR_FLAG) == TRUE)", 1)[1]
        body = "    if (FlagGet(REGION_GAME_CLEAR_FLAG) == TRUE)" + body.split(
            "\n    if (GetGameStat(GAME_STAT_FIRST_HOF_PLAY_TIME)", 1
        )[0]
        harness = f"""
#define TRUE 1
#define FALSE 0
#define FLAG_SYS_GAME_CLEAR 4
#define Cormoria_FLAG_SYS_GAME_CLEAR 17
{selector}
static int flags[32];
static int gHasHallOfFameRecords;
static int gHasHallOfFameRecordsFrlg;
static int FlagGet(int flag) {{ return flags[flag]; }}
static void FlagSet(int flag) {{ flags[flag] = TRUE; }}
static void classify(void) {{
{body}
}}
int main(void) {{
    int own = REGION_GAME_CLEAR_FLAG;
    int other = own == FLAG_SYS_GAME_CLEAR ? Cormoria_FLAG_SYS_GAME_CLEAR : FLAG_SYS_GAME_CLEAR;
    for (int otherSet = 0; otherSet < 2; otherSet++) {{
        flags[own] = FALSE;
        flags[other] = otherSet;
        classify();
        if (gHasHallOfFameRecords || gHasHallOfFameRecordsFrlg || !flags[own] || flags[other] != otherSet)
            return 1;
        classify();
        if (!gHasHallOfFameRecords || !gHasHallOfFameRecordsFrlg || flags[other] != otherSet)
            return 2;
    }}
    return 0;
}}
"""
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "game_clear.c"
            executable = Path(directory) / "game_clear.exe"
            source.write_text(harness, encoding="utf-8")
            for world in (1, 2):
                with self.subTest(world=world):
                    subprocess.run(
                        [compiler, "-std=c99", "-Wall", "-Werror", f"-DROM_WORLD={world}",
                         str(source), "-o", str(executable)],
                        check=True, capture_output=True, text=True,
                    )
                    subprocess.run([str(executable)], check=True, capture_output=True, text=True)

    def test_championship_script_does_not_preempt_first_clear(self) -> None:
        script = (ROOT / "data/cormoria/maps/Championship_R5/scripts.inc").read_text(encoding="utf-8")
        before_game_clear = script.split("special GameClear", 1)[0]
        self.assertNotIn("setflag Cormoria_FLAG_SYS_GAME_CLEAR", before_game_clear)


if __name__ == "__main__":
    unittest.main()
