#include "global.h"
#include "item.h"
#include "load_save.h"
#include "test/test.h"

static const enum Item sCormoriaKeyItems[] =
{
    ITEM_APPOINTMENT_LETTER,
    ITEM_ARCHAEOLENS,
    ITEM_ARCHAEOLENS_2,
    ITEM_BACKSTAGE_PASS,
    ITEM_DETECTIVE_STUDENT_ID,
    ITEM_DIAMOND,
    ITEM_DRIFBLIM_TRAVELS_PASS,
    ITEM_FAKE_STUDENT_ID,
    ITEM_GACHA_TOKEN,
    ITEM_HEAL_PASS,
    ITEM_HISTORIAN_MEDAL,
    ITEM_LAB_WELCOMEPACKAGE,
    ITEM_NUZKIT,
    ITEM_ORPHANAGE_BOOK,
    ITEM_POCKET_BOY,
    ITEM_POCKET_DRIVE,
    ITEM_PURPLE_SCARF,
    ITEM_RANGER_CARD,
    ITEM_RANGER_CREST,
    ITEM_RANGER_PACKAGE,
    ITEM_RETRO_DRIVE,
    ITEM_SMUGGLER_EMBLEM,
    ITEM_STRANGE_ROCK,
    ITEM_SWAP_DRIVE,
    ITEM_TIME_SEED,
    ITEM_TIME_WATER,
    ITEM_TREKKING_BOOTS,
};

TEST("Cormoria key items coexist with Johto key items in the shared bag")
{
    u32 i;

    SetSaveBlocksPointers(0);
    SetBagItemsPointers();
    ClearBag();
    EXPECT(AddBagItem(ITEM_CLEAR_BELL, 1));
    EXPECT(AddBagItem(ITEM_GS_BALL, 1));
    for (i = 0; i < ARRAY_COUNT(sCormoriaKeyItems); i++)
    {
        EXPECT_EQ(GetItemPocket(sCormoriaKeyItems[i]), POCKET_KEY_ITEMS);
        EXPECT(AddBagItem(sCormoriaKeyItems[i], 1));
        EXPECT(CheckBagHasItem(sCormoriaKeyItems[i], 1));
    }
    EXPECT(CheckBagHasItem(ITEM_CLEAR_BELL, 1));
    EXPECT(CheckBagHasItem(ITEM_GS_BALL, 1));
}

TEST("Cormoria HM Splash resolves through the common machine table")
{
    EXPECT_EQ(GetItemPocket(ITEM_HM_SPLASH), POCKET_TM_HM);
    EXPECT_EQ(GetItemTMHMMoveId(ITEM_HM_SPLASH), MOVE_SPLASH);
    EXPECT_EQ(GetTMHMItemIdFromMoveId(MOVE_SPLASH), ITEM_HM_SPLASH);
    EXPECT_EQ(GetItemPocket(ITEM_ANCIENT_STONE), POCKET_ITEMS);
    EXPECT_EQ(GetItemPocket(ITEM_RARE_SHARD), POCKET_ITEMS);
}
