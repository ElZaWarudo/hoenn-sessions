#include "global.h"
#include "constants/characters.h"
#include "graphics.h"
#include "item.h"
#include "item_icon.h"
#include "item_use.h"
#include "pokemon.h"
#include "test/test.h"

TEST("Whirlpool appends HM09 without renumbering existing machines")
{
    u16 i;

    EXPECT_EQ(ITEM_TM01, 582);
    EXPECT_EQ(ITEM_TM100, 681);
    EXPECT_EQ(ITEM_HM01, 682);
    EXPECT_EQ(ITEM_HM08, 689);
    EXPECT_EQ(ITEM_EXP_SHARE_SMALL, 884);
    EXPECT_EQ(ITEM_HM09, 885);
    EXPECT_EQ(ITEMS_COUNT, 886);
    EXPECT_EQ(NUM_TECHNICAL_MACHINES, 50);
    EXPECT_EQ(NUM_HIDDEN_MACHINES, 9);
    EXPECT_EQ(NUM_ALL_MACHINES, 59);

    for (i = 1; i <= NUM_TECHNICAL_MACHINES; i++)
    {
        enum Item item = ITEM_TM01 + i - 1;
        enum Move move = GetTMHMMoveId(i);

        EXPECT_EQ(GetItemTMHMIndex(item), i);
        EXPECT_EQ(GetTMHMItemId(i), item);
        EXPECT_EQ(GetItemTMHMMoveId(item), move);
        EXPECT_EQ(GetTMHMItemIdFromMoveId(move), item);
    }

    for (i = 0; i < NUM_HIDDEN_MACHINES; i++)
    {
        enum Item item;

        if (i == 8)
            item = ITEM_HM09;
        else
            item = ITEM_HM01 + i;

        EXPECT_EQ(GetItemTMHMIndex(item), NUM_TECHNICAL_MACHINES + i + 1);
        EXPECT_EQ(GetTMHMItemId(NUM_TECHNICAL_MACHINES + i + 1), item);
        EXPECT_EQ(GetItemTMHMMoveId(item), GetTMHMMoveId(NUM_TECHNICAL_MACHINES + i + 1));
        EXPECT_EQ(GetTMHMItemIdFromMoveId(GetItemTMHMMoveId(item)), item);
    }
}

TEST("Whirlpool has nonconsumable HM metadata and Water artwork")
{
    enum Item item = ITEM_HM_WHIRLPOOL;

    EXPECT(GetItemName(item)[0] != EOS);
    EXPECT(GetItemDescription(item)[0] != EOS);
    EXPECT_EQ(GetItemPrice(item), 0);
    EXPECT_EQ(GetItemImportance(item), 1);
    EXPECT_EQ(GetItemPocket(item), POCKET_TM_HM);
    EXPECT_EQ(GetItemType(item), ITEM_USE_PARTY_MENU);
    EXPECT_EQ(GetItemFieldFunc(item), ItemUseOutOfBattle_TMHM);
    EXPECT_EQ(GetItemIconPic(item), gItemIcon_HM);
    EXPECT_EQ(GetItemIconPalette(item), gItemIconPalette_WaterTMHM);
    EXPECT_EQ((u8)gItemsInfo[item].importance, 1);
}

TEST("Whirlpool participates in teachability and HM APIs")
{
    enum Item item = ITEM_HM_WHIRLPOOL;
    enum TMHMIndex index = NUM_TECHNICAL_MACHINES + 8 + 1;

    EXPECT_EQ(GetItemTMHMIndex(item), index);
    EXPECT_EQ(GetItemTMHMMoveId(item), MOVE_WHIRLPOOL);
    EXPECT_EQ(GetTMHMItemId(index), item);
    EXPECT_EQ(GetTMHMMoveId(index), MOVE_WHIRLPOOL);
    EXPECT_EQ(GetTMHMItemIdFromMoveId(MOVE_WHIRLPOOL), item);
    EXPECT(IsMoveHM(MOVE_WHIRLPOOL));
    EXPECT(IsMoveHM(MOVE_DIVE));
    EXPECT(IsMoveHM(MOVE_SURF));
    EXPECT(!IsMoveHM(MOVE_SPLASH));

    EXPECT(CanLearnTeachableMove(SPECIES_TOTODILE, MOVE_WHIRLPOOL));
    EXPECT(CanLearnTeachableMove(SPECIES_GYARADOS, MOVE_WHIRLPOOL));
    EXPECT(!CanLearnTeachableMove(SPECIES_MAGIKARP, MOVE_WHIRLPOOL));
    EXPECT(!CanLearnTeachableMove(SPECIES_CHIKORITA, MOVE_WHIRLPOOL));
}
