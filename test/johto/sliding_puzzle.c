#include "global.h"
#include "sliding_puzzle.h"
#include "test/test.h"

static const u8 sSolvedTiles[NUM_SLIDING_PUZZLE_ROWS * NUM_SLIDING_PUZZLE_COLS] =
{
    0, 1, 2, 3, 4, 0,
    0, 5, 6, 7, 8, 0,
    0, 9, 10, 11, 12, 0,
    0, 13, 14, 15, 16, 0,
};

static const u8 sSolvedOrientations[NUM_SLIDING_PUZZLE_ROWS * NUM_SLIDING_PUZZLE_COLS] =
{
    [0 ... (NUM_SLIDING_PUZZLE_ROWS * NUM_SLIDING_PUZZLE_COLS - 1)] = ORIENTATION_0,
};

TEST("Ruins of Alph keeps all four donor boards and completed layout")
{
    static const u8 firstRows[4][NUM_SLIDING_PUZZLE_COLS] =
    {
        {6, 1, 0, 3, 4, 0},
        {9, 0, 2, 3, 4, 11},
        {14, 0, 0, 3, 4, 0},
        {15, 0, 14, 3, 4, 1},
    };
    u8 i, j;
    for (i = 0; i < 4; i++)
    {
        const u8 *layout = SlidingPuzzle_TestLayout(i);
        EXPECT(layout != NULL);
        for (j = 0; j < NUM_SLIDING_PUZZLE_COLS; j++)
            EXPECT_EQ(layout[j], firstRows[i][j]);
        EXPECT(!SlidingPuzzle_TestInitialLayoutSolved(i));
    }
    EXPECT(SlidingPuzzle_TestLayout(SLIDING_PUZZLE_SOLVED) != NULL);
    EXPECT_EQ(SlidingPuzzle_TestLayout(SLIDING_PUZZLE_SOLVED)[1], 1);
    EXPECT_EQ(SlidingPuzzle_TestLayout(SLIDING_PUZZLE_SOLVED)[23], 0);
}

TEST("Sliding puzzle production solution checker rejects blanks and rotation errors")
{
    u8 tiles[ARRAY_COUNT(sSolvedTiles)];
    u8 orientations[ARRAY_COUNT(sSolvedOrientations)];
    memcpy(tiles, sSolvedTiles, sizeof(tiles));
    memcpy(orientations, sSolvedOrientations, sizeof(orientations));

    EXPECT(SlidingPuzzle_TestCheckSolution(tiles, orientations, FALSE));
    EXPECT(!SlidingPuzzle_TestCheckSolution(tiles, orientations, TRUE));
    orientations[7] = ORIENTATION_90;
    EXPECT(!SlidingPuzzle_TestCheckSolution(tiles, orientations, FALSE));
    orientations[7] = ORIENTATION_0;
    tiles[0] = 1;
    EXPECT(!SlidingPuzzle_TestCheckSolution(tiles, orientations, FALSE));
    tiles[0] = 0;
    EXPECT(!SlidingPuzzle_TestCheckSolution(NULL, orientations, FALSE));
    EXPECT_EQ(SlidingPuzzle_TestOrientation(SLIDING_PUZZLE_KABUTO, 0, 0), ORIENTATION_90);
    EXPECT_EQ(SlidingPuzzle_TestOrientation(SLIDING_PUZZLE_KABUTO, 0, 2), ORIENTATION_0);
    EXPECT_EQ(SlidingPuzzle_TestOrientation(SLIDING_PUZZLE_COUNT, 0, 0), IMMOVABLE_TILE);
    EXPECT(SlidingPuzzle_TestValidPuzzleId(0));
    EXPECT(!SlidingPuzzle_TestValidPuzzleId(SLIDING_PUZZLE_SOLVED));
    EXPECT(!SlidingPuzzle_TestValidPuzzleId(0xFFFF));
    EXPECT(SlidingPuzzle_TestManipulation());
}
