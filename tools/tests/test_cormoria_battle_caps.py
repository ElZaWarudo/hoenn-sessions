"""Compile the real battle-cap selector against small host-side stubs."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "src" / "battle_caps.c"


def _extract_function(source: str) -> str:
    start = source.index("u32 GetBadgeBattleLevelCap(void)")
    opening = source.index("{", start)
    depth = 0
    for index in range(opening, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[start : index + 1]
    raise AssertionError("GetBadgeBattleLevelCap has no closing brace")


def _harness(function: str) -> str:
    return f"""
#include <stdint.h>
#include "constants/cormoria_event_ids.h"

typedef uint8_t u8;
typedef uint16_t u16;
typedef uint32_t u32;
typedef int bool32;

#define TRUE 1
#define FALSE 0
#define MAX_LEVEL 100
#define NUM_BADGES 8
#define KANTO_ERA_LATER 2
#define VAR_MAP_SCENE_PALLET_TOWN_OAK 7

#define TRAINER_ROXANNE_1 100
#define TRAINER_BRAWLY_1 101
#define TRAINER_WATTSON_1 102
#define TRAINER_FLANNERY_1 103
#define TRAINER_NORMAN_1 104
#define TRAINER_WINONA_1 105
#define TRAINER_TATE_AND_LIZA_1 106
#define TRAINER_JUAN_1 107

#define FLAG_DEFEATED_BROCK 200
#define FLAG_DEFEATED_MISTY 201
#define FLAG_DEFEATED_LT_SURGE 202
#define FLAG_DEFEATED_ERIKA 203
#define FLAG_DEFEATED_KOGA 204
#define FLAG_DEFEATED_SABRINA 205
#define FLAG_DEFEATED_BLAINE 206
#define FLAG_DEFEATED_LEADER_GIOVANNI 207
#define FLAG_KANTO_MASTERY_CHAMPION 208
#define FLAG_IS_CHAMPION 209

#define JOHTO_FLAG_BADGE01_GET 300
#define JOHTO_FLAG_BADGE02_GET 301
#define JOHTO_FLAG_BADGE03_GET 302
#define JOHTO_FLAG_BADGE04_GET 303
#define JOHTO_FLAG_BADGE05_GET 304
#define JOHTO_FLAG_BADGE06_GET 305
#define JOHTO_FLAG_BADGE07_GET 306
#define JOHTO_FLAG_BADGE08_GET 307
#define JOHTO_FLAG_IS_CHAMPION 308

enum Region {{ REGION_KANTO = 1, REGION_JOHTO, REGION_HOENN, REGION_CORMORIA = 11 }};
struct SaveLocation {{ u8 mapGroup; u8 mapNum; }};
struct SaveBlock1 {{ struct SaveLocation location; }};
struct MapHeader {{ u16 regionMapSectionId; }};
static struct SaveBlock1 saveBlock1;
static struct SaveBlock1 *gSaveBlock1Ptr = &saveBlock1;
static struct MapHeader gMapHeader;
static enum Region currentRegion;
static int kantoEra;
static unsigned char flags[0x9000];
static unsigned char trainers[256];
static unsigned int vars[16];

static enum Region GetCurrentRegion(void) {{ return currentRegion; }}
static int GetKantoEraByMap(u8 mapGroup, u8 mapNum, u16 sectionId)
{{
    (void)mapGroup;
    (void)mapNum;
    (void)sectionId;
    return kantoEra;
}}
static int FlagGet(u16 flag) {{ return flags[flag]; }}
static int HasTrainerBeenFought(u16 trainer) {{ return trainers[trainer - 100]; }}
static unsigned int VarGet(unsigned int var) {{ return vars[var]; }}

{function}

static void clear_state(void)
{{
    for (unsigned int i = 0; i < sizeof(flags); i++)
        flags[i] = 0;
    for (unsigned int i = 0; i < sizeof(trainers); i++)
        trainers[i] = 0;
    for (unsigned int i = 0; i < sizeof(vars) / sizeof(vars[0]); i++)
        vars[i] = 0;
    kantoEra = 0;
    gMapHeader.regionMapSectionId = 0;
    currentRegion = REGION_HOENN;
}}

static int test_hoenn(void)
{{
    static const unsigned int caps[] = {{15, 19, 24, 29, 31, 33, 42, 46, 58}};
    clear_state();
    currentRegion = REGION_HOENN;
    for (unsigned int i = 0; i <= NUM_BADGES; i++)
    {{
        if (GetBadgeBattleLevelCap() != caps[i])
            return 1;
        if (i < NUM_BADGES)
            trainers[i] = 1;
    }}
    flags[FLAG_IS_CHAMPION] = 1;
    if (GetBadgeBattleLevelCap() != MAX_LEVEL)
        return 2;
    return 0;
}}

#if ROM_WORLD == 2
static int test_cormoria(void)
{{
    static const unsigned int caps[] = {{15, 19, 24, 29, 31, 33, 42, 46, 58}};
    static const u16 badges[] = {{
        Cormoria_FLAG_BADGE01_GET, Cormoria_FLAG_BADGE02_GET,
        Cormoria_FLAG_BADGE03_GET, Cormoria_FLAG_BADGE04_GET,
        Cormoria_FLAG_BADGE05_GET, Cormoria_FLAG_BADGE06_GET,
        Cormoria_FLAG_BADGE07_GET, Cormoria_FLAG_BADGE08_GET,
    }};
    clear_state();
    currentRegion = REGION_CORMORIA;
    flags[FLAG_IS_CHAMPION] = 1;
    if (GetBadgeBattleLevelCap() != caps[0])
        return 3;
    for (unsigned int i = 0; i < NUM_BADGES; i++)
    {{
        flags[badges[i]] = 1;
        if (GetBadgeBattleLevelCap() != caps[i + 1])
            return 4;
    }}
    if (GetBadgeBattleLevelCap() != caps[NUM_BADGES])
        return 5;
    flags[Cormoria_FLAG_IS_CHAMPION] = 1;
    if (GetBadgeBattleLevelCap() != MAX_LEVEL)
        return 6;
    return 0;
}}
#endif

int main(void)
{{
    int result = test_hoenn();
#if ROM_WORLD == 2
    if (result == 0)
        result = test_cormoria();
#endif
    return result;
}}
"""


class CormoriaBattleCapTests(unittest.TestCase):
    def test_real_selector_uses_independent_hoenn_and_cormoria_progression(self) -> None:
        compiler = shutil.which("gcc") or shutil.which("clang")
        if compiler is None:
            self.skipTest("host C compiler unavailable")

        function = _extract_function(SOURCE.read_text(encoding="utf-8"))
        with tempfile.TemporaryDirectory() as directory:
            directory_path = Path(directory)
            harness_path = directory_path / "battle_caps.c"
            executable = directory_path / "battle_caps.exe"
            harness_path.write_text(_harness(function), encoding="utf-8")
            for world in (1, 2):
                with self.subTest(rom_world=world):
                    subprocess.run(
                        [
                            compiler,
                            "-std=c99",
                            "-Wall",
                            "-Werror",
                            f"-DROM_WORLD={world}",
                            "-I",
                            str(ROOT / "include"),
                            str(harness_path),
                            "-o",
                            str(executable),
                        ],
                        check=True,
                        capture_output=True,
                        text=True,
                    )
                    subprocess.run(
                        [str(executable)],
                        check=True,
                        capture_output=True,
                        text=True,
                    )


if __name__ == "__main__":
    unittest.main()
