import hashlib
import json
import re
import struct
import unittest
from pathlib import Path


ROOT = Path(__file__).parents[2]
DONOR = Path(r"C:/Users/Mayor/Documents/Caribbean/johto-hns")
ASSET_NAMES = (
    "bg.pal",
    "bg.png",
    "cursor.png",
    "cursor_tiles.pal",
    "map.bin",
    "puzzles/aerodactyl/tiles.png",
    "puzzles/ho_oh/tiles.png",
    "puzzles/kabuto/tiles.png",
    "puzzles/omanyte/tiles.png",
)
PUZZLES = ("kabuto", "omanyte", "aerodactyl", "ho_oh")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def png_size(path: Path):
    data = path.read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise AssertionError(f"not a PNG: {path}")
    return struct.unpack(">II", data[16:24])


def puzzle_tables(source: str):
    constants = {"__": 0, "ORIENTATION_0": 0, "ORIENTATION_90": 1,
                 "ORIENTATION_180": 2, "ORIENTATION_270": 3, "IMMOVABLE_TILE": 4}
    result = {}
    for table in ("sPuzzleLayouts", "sTileOrientations"):
        body = re.search(r"static const u8 " + table + r"[^=]*=\s*\{(.*?)\n\};", source, re.S).group(1)
        boards = {}
        for name, cells in re.findall(r"\[SLIDING_PUZZLE_(\w+)\]\s*=\s*\{(.*?)\n    \}", body, re.S):
            rows = []
            for row in re.findall(r"\{([^{}]+)\}", cells):
                tokens = [token.strip() for token in row.split(",") if token.strip()]
                rows.append([constants[token] if token in constants else int(token) for token in tokens])
            if len(rows) != 4 or any(len(row) != 6 for row in rows):
                raise ValueError(f"Invalid board dimensions: {table}/{name}")
            boards[name] = rows
        if set(boards) != {"KABUTO", "OMANYTE", "AERODACTYL", "HO_OH", "SOLVED"}:
            raise ValueError(f"Missing or unexpected boards in {table}")
        result[table] = boards
    return result


class JohtoSlidingPuzzleTests(unittest.TestCase):
    def test_source_uses_raw_input_and_safe_lifecycle(self):
        source = (ROOT / "src/sliding_puzzle.c").read_text()
        self.assertIn("void DoSlidingPuzzle(void)", source)
        self.assertIn("Script_RequestEffects(SCREFF_V1 | SCREFF_HARDWARE)", source)
        self.assertIn("gMain.newKeysRaw", source)
        self.assertNotIn("optionsButtonMode", source)
        self.assertNotIn("JOY_NEW", source)
        self.assertIn("!IsValidPuzzleId(gSpecialVar_0x8004)", source)
        self.assertIn("return puzzleId < SLIDING_PUZZLE_SOLVED;", source)
        self.assertIn("CheckSolutionAgainst", source)
        self.assertGreaterEqual(source.count("SPRITE_NONE"), 10)
        self.assertIn("if (sSlidingPuzzle->cursorSpriteId != SPRITE_NONE)", source)

    def test_four_boards_and_solved_layout_are_exact(self):
        data = (ROOT / "src/data/johto/sliding_puzzles.h").read_text()
        expected = puzzle_tables((DONOR / "src/data/sliding_puzzles.h").read_text())
        self.assertEqual(puzzle_tables(data), expected)
        changed_lower_row = data.replace("{ 2,13,14,15,16, 9}", "{ 9,13,14,15,16, 2}", 1)
        self.assertNotEqual(changed_lower_row, data)
        with self.assertRaises(AssertionError):
            self.assertEqual(puzzle_tables(changed_lower_row), expected)
        changed_orientation = data.replace("ORIENTATION_270", "ORIENTATION_90", 1)
        with self.assertRaises(AssertionError):
            self.assertEqual(puzzle_tables(changed_orientation), expected)
        self.assertIn("0x2000, GFXTAG_TILES", data)

    def test_assets_match_pinned_source_and_geometry(self):
        for name in ASSET_NAMES:
            target = ROOT / "graphics/johto/sliding_puzzle" / name
            donor = DONOR / "graphics/sliding_puzzle" / name
            self.assertTrue(target.is_file(), name)
            self.assertTrue(donor.is_file(), name)
            self.assertEqual(sha256(target), sha256(donor), name)
        self.assertEqual(png_size(ROOT / "graphics/johto/sliding_puzzle/bg.png"), (128, 80))
        self.assertEqual(png_size(ROOT / "graphics/johto/sliding_puzzle/cursor.png"), (32, 32))
        for puzzle in PUZZLES:
            self.assertEqual(
                png_size(ROOT / "graphics/johto/sliding_puzzle/puzzles" / puzzle / "tiles.png"),
                (128, 128),
            )
        self.assertEqual((ROOT / "graphics/johto/sliding_puzzle/bg.pal").stat().st_size, 167)
        self.assertEqual((ROOT / "graphics/johto/sliding_puzzle/cursor_tiles.pal").stat().st_size, 172)

    def test_graphics_rules_pack_sixteen_32px_frames(self):
        rules = (ROOT / "graphics_file_rules.mk").read_text()
        for puzzle in PUZZLES:
            pattern = rf"graphics/johto/sliding_puzzle/puzzles/{puzzle}/tiles\.4bpp: %\.4bpp: %\.png\n\s*\$\(GFX\) \$< \$@ -mwidth 4 -mheight 4"
            self.assertRegex(rules, pattern)
        self.assertNotIn("-mwidth4", rules)
        self.assertNotIn("-mheight4", rules)
        self.assertIn("*.png binary", (ROOT / "graphics/johto/sliding_puzzle/.gitattributes").read_text())

    def test_special_and_test_hooks_are_registered(self):
        specials = (ROOT / "data/specials.inc").read_text()
        self.assertRegex(specials, r"def_special DoSlidingPuzzle, requests_effects=1")
        header = (ROOT / "include/sliding_puzzle.h").read_text()
        self.assertIn("SlidingPuzzle_TestLayout", header)
        self.assertIn("SlidingPuzzle_TestCheckSolution", header)
        source = (ROOT / "src/sliding_puzzle.c").read_text()
        self.assertIn("SlidingPuzzle_TestCheckSolution", source)

    def test_provenance_binds_all_owned_source_art(self):
        provenance = json.loads((ROOT / "data/johto/sliding_puzzle.json").read_text())
        self.assertFalse(provenance["campaign_ready"])
        self.assertEqual(provenance["source_revision"], "751823abaf677020bcd72c45fe3e7cb2b8a576e4")
        self.assertEqual(set(provenance["assets"]), set(ASSET_NAMES))
        for name in ASSET_NAMES:
            self.assertEqual(provenance["assets"][name], sha256(ROOT / "graphics/johto/sliding_puzzle" / name))

    def test_runtime_solver_oracle_is_production_bound(self):
        source = (ROOT / "src/sliding_puzzle.c").read_text()
        self.assertLess(source.index("static bool32 CheckSolutionAgainst"), source.index("bool32 SlidingPuzzle_TestCheckSolution"))
        self.assertIn("return CheckSolutionAgainst(&puzzle, sprites);", source)
        self.assertIn("tileId == __", source)
        self.assertIn("spriteId == SPRITE_NONE || spriteId >= MAX_SPRITES", source)


if __name__ == "__main__":
    unittest.main()
