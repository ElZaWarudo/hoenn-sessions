#include "global.h"
#include "graphics.h"
#include "constants/characters.h"
#include "item.h"
#include "item_use.h"
#include "load_save.h"
#include "string_util.h"
#include "test/test.h"

static const enum Item sJohtoQuestItems[] =
{
    ITEM_CLEAR_BELL,
    ITEM_GS_BALL,
    ITEM_MYSTERY_EGG,
    ITEM_PASS,
    ITEM_RAINBOW_WING,
    ITEM_RED_SCALE,
    ITEM_SECRET_POTION,
    ITEM_SILVER_WING,
    ITEM_TIDAL_BELL,
};

static void ResetItemFixture(void)
{
    SetSaveBlocksPointers(0);
    SetBagItemsPointers();
    ClearBag();
}

TEST("Johto passive quest items expose key item metadata")
{
    u32 i;

    for (i = 0; i < ARRAY_COUNT(sJohtoQuestItems); i++)
    {
        enum Item item = sJohtoQuestItems[i];

        EXPECT_EQ(GetItemPocket(item), POCKET_KEY_ITEMS);
        EXPECT_EQ(GetItemImportance(item), 1);
        EXPECT_EQ(GetItemType(item), ITEM_USE_BAG_MENU);
        EXPECT_EQ(GetItemFieldFunc(item), ItemUseOutOfBattle_CannotUse);
        EXPECT_EQ(GetItemBattleUsage(item), 0);
        EXPECT_EQ(GetItemHoldEffect(item), HOLD_EFFECT_NONE);
        EXPECT(GetItemEffect(item) == NULL);
        EXPECT(GetItemName(item)[0] != EOS);
        EXPECT(GetItemDescription(item)[0] != EOS);
        EXPECT(gItemsInfo[item].iconPic != NULL);
        EXPECT(gItemsInfo[item].iconPalette != NULL);
    }

    EXPECT_EQ(GetItemPocket(ITEM_GS_BALL), POCKET_KEY_ITEMS);
    EXPECT_EQ(GetItemBattleUsage(ITEM_GS_BALL), 0);
    EXPECT_EQ(GetItemHoldEffect(ITEM_GS_BALL), HOLD_EFFECT_NONE);
}

TEST("Johto quest item bag identity remains distinct through add and remove")
{
    u32 i;

    ResetItemFixture();
    for (i = 0; i < ARRAY_COUNT(sJohtoQuestItems); i++)
    {
        enum Item item = sJohtoQuestItems[i];

        EXPECT(AddBagItem(item, 1));
        EXPECT(CheckBagHasItem(item, 1));
        EXPECT_EQ(CountTotalItemQuantityInBag(item), 1);
    }

    for (i = ARRAY_COUNT(sJohtoQuestItems); i-- > 0;)
    {
        enum Item item = sJohtoQuestItems[i];

        EXPECT_EQ(GetBagItemId(POCKET_KEY_ITEMS, i), item);
        EXPECT(RemoveBagItem(item, 1));
        EXPECT(!CheckBagHasItem(item, 1));
    }
}

TEST("Exp Share Small is an ordinary held effect item")
{
    EXPECT_EQ(GetItemPrice(ITEM_EXP_SHARE_SMALL), 6000);
    EXPECT_EQ(GetItemPocket(ITEM_EXP_SHARE_SMALL), POCKET_ITEMS);
    EXPECT_EQ(GetItemImportance(ITEM_EXP_SHARE_SMALL), 0);
    EXPECT_EQ(GetItemType(ITEM_EXP_SHARE_SMALL), ITEM_USE_BAG_MENU);
    EXPECT_EQ(GetItemFieldFunc(ITEM_EXP_SHARE_SMALL), ItemUseOutOfBattle_CannotUse);
    EXPECT_EQ(GetItemHoldEffect(ITEM_EXP_SHARE_SMALL), HOLD_EFFECT_EXP_SHARE);
    EXPECT_EQ(GetItemBattleUsage(ITEM_EXP_SHARE_SMALL), 0);
    EXPECT(GetItemEffect(ITEM_EXP_SHARE_SMALL) == NULL);
    EXPECT_EQ(gItemsInfo[ITEM_EXP_SHARE_SMALL].iconPic, gItemIcon_ExpShare);
    EXPECT_EQ(gItemsInfo[ITEM_EXP_SHARE_SMALL].iconPalette, gItemIconPalette_ExpShare);
}
