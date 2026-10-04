#include "global.h"
#include "cormoria/field_move_commands.h"
#include "event_data.h"
#include "item.h"
#include "load_save.h"
#include "pokemon.h"
#include "script.h"
#include "constants/items.h"
#include "constants/moves.h"
#include "constants/species.h"
#include "test/test.h"

static void ResetPartyMoveFixture(void)
{
    SetSaveBlocksPointers(0);
    SetBagItemsPointers();
    ClearBag();
    memset(gPlayerParty, 0, sizeof(gPlayerParty));
    gPlayerPartyCount = 0;
}

static void RunPartyMoveCheck(enum Move move)
{
    struct ScriptContext ctx;
    u8 operand[2] = {move, move >> 8};

    InitScriptContext(&ctx, NULL, NULL);
    ctx.scriptPtr = operand;
    EXPECT(!ScrCmd_CormoriaCheckPartyMove(&ctx));
    EXPECT_EQ(ctx.scriptPtr, operand + sizeof(operand));
}

TEST("Cormoria party-move command returns the first non-egg knower")
{
    ResetPartyMoveFixture();
    CreateMon(&gPlayerParty[0], SPECIES_RATTATA, 10, 1, OTID_STRUCT_PLAYER_ID);
    CreateMon(&gPlayerParty[1], SPECIES_ABRA, 10, 1, OTID_STRUCT_PLAYER_ID);
    SetMonMoveSlot(&gPlayerParty[1], MOVE_CUT, 0);
    RunPartyMoveCheck(MOVE_CUT);
    EXPECT_EQ(gSpecialVar_Result, 1);
    EXPECT_EQ(gSpecialVar_0x8004, SPECIES_ABRA);
}

TEST("Cormoria party-move command permits a carried HM when none knows it")
{
    ResetPartyMoveFixture();
    CreateMon(&gPlayerParty[0], SPECIES_RATTATA, 10, 1, OTID_STRUCT_PLAYER_ID);
    EXPECT(AddBagItem(ITEM_HM01, 1));
    RunPartyMoveCheck(MOVE_CUT);
    EXPECT_EQ(gSpecialVar_Result, 0);
    EXPECT_EQ(gSpecialVar_0x8004, SPECIES_RATTATA);
}

TEST("Cormoria party-move command rejects an egg and an unavailable move")
{
    u8 isEgg = TRUE;
    ResetPartyMoveFixture();
    CreateMon(&gPlayerParty[0], SPECIES_RATTATA, 10, 1, OTID_STRUCT_PLAYER_ID);
    SetMonMoveSlot(&gPlayerParty[0], MOVE_CUT, 0);
    SetMonData(&gPlayerParty[0], MON_DATA_IS_EGG, &isEgg);
    RunPartyMoveCheck(MOVE_CUT);
    EXPECT_EQ(gSpecialVar_Result, PARTY_SIZE);
    RunPartyMoveCheck(MOVE_SPLASH);
    EXPECT_EQ(gSpecialVar_Result, PARTY_SIZE);
}
