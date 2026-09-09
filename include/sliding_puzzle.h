#ifndef GUARD_SLIDING_PUZZLE_H
#define GUARD_SLIDING_PUZZLE_H

#include "constants/sliding_puzzles.h"

enum
{
    ROTATE_NONE,
    ROTATE_ANTICLOCKWISE,
    ROTATE_CLOCKWISE,
};

enum
{
    ORIENTATION_0,
    ORIENTATION_90,
    ORIENTATION_180,
    ORIENTATION_270,
    ORIENTATION_MAX,
};

#define IMMOVABLE_TILE ORIENTATION_MAX

#define NUM_SLIDING_PUZZLE_COLS 6
#define NUM_SLIDING_PUZZLE_ROWS 4

#define FIRST_SLIDING_PUZZLE_COL  0
#define FINAL_SLIDING_PUZZLE_COL  (NUM_SLIDING_PUZZLE_COLS - 1)
#define FIRST_SLIDING_PUZZLE_ROW  0
#define FINAL_SLIDING_PUZZLE_ROW  (NUM_SLIDING_PUZZLE_ROWS - 1)

struct SlidingPuzzle
{
    u8 tiles[NUM_SLIDING_PUZZLE_ROWS][NUM_SLIDING_PUZZLE_COLS];
    u8 puzzleId;
    u8 cursorSpriteId;
    u8 heldTile;
    bool8 solved;
    bool8 failed;
};

#if TESTING
const u8 *SlidingPuzzle_TestLayout(u8 puzzleId);
u8 SlidingPuzzle_TestOrientation(u8 puzzleId, u8 row, u8 col);
bool32 SlidingPuzzle_TestCheckSolution(const u8 *tileIds, const u8 *orientations, bool32 holdingTile);
bool32 SlidingPuzzle_TestInitialLayoutSolved(u8 puzzleId);
bool32 SlidingPuzzle_TestManipulation(void);
bool32 SlidingPuzzle_TestValidPuzzleId(u16 puzzleId);
#endif

#endif // GUARD_SLIDING_PUZZLE_H
