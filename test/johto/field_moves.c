#include "global.h"
#include "battle.h"
#include "event_data.h"
#include "johto/field_moves.h"
#include "load_save.h"
#include "main.h"
#include "pokemon.h"
#include "script.h"
#include "test/test.h"

static EWRAM_DATA struct Pokemon sParty[PARTY_SIZE + 2];
static EWRAM_DATA struct Pokemon sBefore[PARTY_SIZE];

static void MakeWhirlpoolMon(struct Pokemon *mon, u32 slot)
{
    u32 i;
    CreateMon(mon, SPECIES_TOTODILE, 10, 1, OTID_STRUCT_PLAYER_ID);
    for (i = 0; i < MAX_MON_MOVES; i++)
        SetMonMoveSlot(mon, i == slot ? MOVE_WHIRLPOOL : MOVE_NONE, i);
}

static u32 SavedHash(const void *data, u32 size)
{
    const u8 *bytes = data;
    u32 hash = 2166136261u;
    while (size--)
        hash = (hash ^ *bytes++) * 16777619u;
    return hash;
}

TEST("Whirlpool checks each move slot and returns the first qualifying party member")
{
    u32 move;
    SetSaveBlocksPointers(0);
    for (move = 0; move < MAX_MON_MOVES; move++)
    {
        memset(sParty, 0, sizeof(sParty));
        MakeWhirlpoolMon(&sParty[PARTY_SIZE - 1], move);
        EXPECT_EQ(JohtoFieldMoves_GetWhirlpoolUser(sParty, PARTY_SIZE), PARTY_SIZE - 1);
        MakeWhirlpoolMon(&sParty[0], move);
        EXPECT_EQ(JohtoFieldMoves_GetWhirlpoolUser(sParty, PARTY_SIZE), 0);
    }
}

TEST("Whirlpool rejects empty species eggs absent moves and out of count members")
{
    SetSaveBlocksPointers(0);
    memset(sParty, 0, sizeof(sParty));
    EXPECT_EQ(JohtoFieldMoves_GetWhirlpoolUser(sParty, PARTY_SIZE), PARTY_SIZE);
    MakeWhirlpoolMon(&sParty[0], 0);
    SetMonData(&sParty[0], MON_DATA_SPECIES, &(u16){SPECIES_NONE});
    MakeWhirlpoolMon(&sParty[1], 0);
    SetMonData(&sParty[1], MON_DATA_IS_EGG, &(u8){TRUE});
    MakeWhirlpoolMon(&sParty[2], MAX_MON_MOVES);
    EXPECT_EQ(JohtoFieldMoves_GetWhirlpoolUser(sParty, PARTY_SIZE), PARTY_SIZE);
    MakeWhirlpoolMon(&sParty[3], 0);
    EXPECT_EQ(JohtoFieldMoves_GetWhirlpoolUser(sParty, 3), PARTY_SIZE);
    EXPECT_EQ(JohtoFieldMoves_GetWhirlpoolUser(sParty, 4), 3);
    EXPECT_EQ(JohtoFieldMoves_GetWhirlpoolUser(sParty, 0), PARTY_SIZE);
}

TEST("Whirlpool allows fainted users and clamps oversized party counts")
{
    SetSaveBlocksPointers(0);
    memset(sParty, 0, sizeof(sParty));
    // Index PARTY_SIZE is the failure sentinel; use the next index so an
    // unbounded scan would return a distinguishable incorrect result.
    MakeWhirlpoolMon(&sParty[PARTY_SIZE + 1], 0);
    EXPECT_EQ(JohtoFieldMoves_GetWhirlpoolUser(sParty, 0xFFFFFFFF), PARTY_SIZE);
    MakeWhirlpoolMon(&sParty[PARTY_SIZE - 1], MAX_MON_MOVES - 1);
    SetMonData(&sParty[PARTY_SIZE - 1], MON_DATA_HP, &(u16){0});
    EXPECT_EQ(JohtoFieldMoves_GetWhirlpoolUser(sParty, 0xFFFFFFFF), PARTY_SIZE - 1);
}

TEST("Whirlpool native requests V1 before writing result and has no saved effects")
{
    struct ScriptContext ctx;
    u8 program[6];
    u32 i, hash1, hash2, pointer = (uintptr_t)Script_JohtoCheckWhirlpool | 0x0A000000;
    SetSaveBlocksPointers(0);
    InitEventData();
    gMain.inBattle = FALSE;
    gBattleTypeFlags = 0;
    memset(gParties, 0, sizeof(gParties));
    memset(gPartiesCount, 0, sizeof(gPartiesCount));
    gPlayerPartyCount = PARTY_SIZE;
    MakeWhirlpoolMon(&gPlayerParty[PARTY_SIZE - 1], 3);
    memcpy(sBefore, gPlayerParty, sizeof(sBefore));
    hash1 = SavedHash(gSaveBlock1Ptr, sizeof(*gSaveBlock1Ptr));
    hash2 = SavedHash(gSaveBlock2Ptr, sizeof(*gSaveBlock2Ptr));
    program[0] = 0x23; // callnative with requests_effects=1
    for (i = 0; i < 4; i++)
        program[i + 1] = pointer >> (8 * i);
    program[5] = 0x02; // end
    gSpecialVar_Result = 0xBEEF;
    EXPECT(!RunScriptImmediatelyUntilEffect(SCREFF_V1, program, NULL));
    EXPECT_EQ(gSpecialVar_Result, PARTY_SIZE - 1);
    gSpecialVar_Result = 0xBEEF;
    EXPECT(!RunScriptImmediatelyUntilEffect(SCREFF_V1 | SCREFF_SAVE | SCREFF_HARDWARE, program, NULL));
    EXPECT_EQ(gSpecialVar_Result, PARTY_SIZE - 1);
    InitScriptContext(&ctx, NULL, NULL);
    ctx.scriptPtr = program;
    Script_JohtoCheckWhirlpool(&ctx);
    EXPECT_EQ(ctx.scriptPtr, program);
    EXPECT_EQ(gSpecialVar_Result, PARTY_SIZE - 1);
    gPlayerPartyCount = 0;
    Script_JohtoCheckWhirlpool(&ctx);
    EXPECT_EQ(gSpecialVar_Result, PARTY_SIZE);
    EXPECT_EQ(memcmp(gPlayerParty, sBefore, sizeof(sBefore)), 0);
    EXPECT_EQ(SavedHash(gSaveBlock1Ptr, sizeof(*gSaveBlock1Ptr)), hash1);
    EXPECT_EQ(SavedHash(gSaveBlock2Ptr, sizeof(*gSaveBlock2Ptr)), hash2);
}
