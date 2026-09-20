#include "global.h"
#include "constants/items.h"
#include "constants/species.h"
#include "event_data.h"
#include "load_save.h"
#include "mail.h"
#include "pokemon.h"
#include "script.h"
#include "test/test.h"
#include "johto/gifts.h"
#include "johto/quest_party.h"

static const u8 sKenyaTestNickname[] = _("KENYA");
static const u8 sShuckieTestNickname[] = _("SHUCKIE");

static void PutHalfword(u8 *script, u16 value)
{
    script[0] = value;
    script[1] = value >> 8;
}

static void ResetQuestFixture(void)
{
    SetSaveBlocksPointers(0);
    memset(gPlayerParty, 0, sizeof(gPlayerParty));
    memset(gEnemyParty, 0, sizeof(gEnemyParty));
    memset(gPartiesCount, 0, sizeof(gPartiesCount));
    ClearAllMail();
}

static u32 RemoveNamed(u16 giftId)
{
    struct ScriptContext ctx;
    u8 payload[2];

    PutHalfword(payload, giftId);
    InitScriptContext(&ctx, NULL, NULL);
    ctx.scriptPtr = payload;
    Script_JohtoRemoveNamedMon(&ctx);
    EXPECT_EQ(ctx.scriptPtr, payload + sizeof(payload));
    return gSpecialVar_Result;
}

static u32 GiveNamedGift(u16 giftId)
{
    struct ScriptContext ctx;
    u8 payload[2];

    PutHalfword(payload, giftId);
    InitScriptContext(&ctx, NULL, NULL);
    ctx.scriptPtr = payload;
    Script_JohtoGiveNamedMon(&ctx);
    EXPECT_EQ(ctx.scriptPtr, payload + sizeof(payload));
    return gSpecialVar_Result;
}

static u32 RemoveGeneric(u16 species, u16 partyIndex)
{
    struct ScriptContext ctx;
    u8 payload[2];

    VarSet(VAR_0x8004, partyIndex);
    PutHalfword(payload, species);
    InitScriptContext(&ctx, NULL, NULL);
    ctx.scriptPtr = payload;
    Script_JohtoRemoveGenericMon(&ctx);
    EXPECT_EQ(ctx.scriptPtr, payload + sizeof(payload));
    return gSpecialVar_Result;
}

static u32 CheckBaoba(u8 checkId, u16 partyIndex)
{
    struct ScriptContext ctx;
    u8 payload[1] = {checkId};

    VarSet(VAR_0x8004, partyIndex);
    InitScriptContext(&ctx, NULL, NULL);
    ctx.scriptPtr = payload;
    Script_JohtoBaobaCheckMon(&ctx);
    EXPECT_EQ(ctx.scriptPtr, payload + sizeof(payload));
    return gSpecialVar_Result;
}

static void MakeMon(u16 slot, enum Species species, u8 level)
{
    CreateMon(&gPlayerParty[slot], species, level, slot + 1, OTID_STRUCT_PLAYER_ID);
}

static void SetNickname(struct Pokemon *mon, const u8 *nickname)
{
    SetMonData(mon, MON_DATA_NICKNAME, nickname);
}

TEST("Johto named quest returns consume exactly one gift and its mail")
{
    u8 mailId;

    ResetQuestFixture();
    EXPECT_EQ(GiveNamedGift(JOHTO_QUEST_NAMED_GIFT_KENYA), MON_GIVEN_TO_PARTY);
    mailId = GetMonData(&gPlayerParty[0], MON_DATA_MAIL);
    EXPECT_LT(mailId, MAIL_COUNT);
    MakeMon(1, SPECIES_RATTATA, 5);
    CalculatePlayerPartyCount();

    EXPECT_EQ(RemoveNamed(JOHTO_QUEST_NAMED_GIFT_KENYA), JOHTO_QUEST_RESULT_NAMED_GIVEN);
    EXPECT_EQ(GetMonData(&gPlayerParty[0], MON_DATA_SPECIES), SPECIES_RATTATA);
    EXPECT_EQ(gPartiesCount[B_TRAINER_0], 1);
    EXPECT_EQ(gSaveBlock1Ptr->mail[mailId].itemId, ITEM_NONE);
    EXPECT_EQ(GetMonData(&gPlayerParty[0], MON_DATA_HELD_ITEM), ITEM_NONE);

    ResetQuestFixture();
    MakeMon(0, SPECIES_FEAROW, 30);
    SetNickname(&gPlayerParty[0], sKenyaTestNickname);
    GiveMailToMonByItemId(&gPlayerParty[0], ITEM_RETRO_MAIL);
    MakeMon(1, SPECIES_RATTATA, 5);
    CalculatePlayerPartyCount();
    EXPECT_EQ(RemoveNamed(JOHTO_QUEST_NAMED_GIFT_KENYA), JOHTO_QUEST_RESULT_NAMED_GIVEN);
    EXPECT_EQ(GetMonData(&gPlayerParty[0], MON_DATA_SPECIES), SPECIES_RATTATA);
}

TEST("Johto Shuckie friendship and last non-egg checks are atomic")
{
    struct Pokemon partyBefore[PARTY_SIZE];
    u16 friendship;
    bool8 isEgg = TRUE;

    ResetQuestFixture();
    EXPECT_EQ(GiveNamedGift(JOHTO_QUEST_NAMED_GIFT_SHUCKIE), MON_GIVEN_TO_PARTY);
    friendship = 200;
    SetMonData(&gPlayerParty[0], MON_DATA_FRIENDSHIP, &friendship);
    MakeMon(1, SPECIES_RATTATA, 5);
    CalculatePlayerPartyCount();
    EXPECT_EQ(RemoveNamed(JOHTO_QUEST_NAMED_GIFT_SHUCKIE), JOHTO_QUEST_RESULT_NAMED_GIVEN);
    EXPECT_EQ(GetMonData(&gPlayerParty[0], MON_DATA_SPECIES), SPECIES_RATTATA);

    ResetQuestFixture();
    MakeMon(0, SPECIES_SHUCKLE, 20);
    SetNickname(&gPlayerParty[0], sShuckieTestNickname);
    friendship = 201;
    SetMonData(&gPlayerParty[0], MON_DATA_FRIENDSHIP, &friendship);
    MakeMon(1, SPECIES_RATTATA, 5);
    CalculatePlayerPartyCount();
    memcpy(partyBefore, gPlayerParty, sizeof(partyBefore));
    EXPECT_EQ(RemoveNamed(JOHTO_QUEST_NAMED_GIFT_SHUCKIE), JOHTO_QUEST_RESULT_SHUCKIE_TOO_FRIENDLY);
    EXPECT_EQ(memcmp(partyBefore, gPlayerParty, sizeof(partyBefore)), 0);

    ResetQuestFixture();
    MakeMon(0, SPECIES_SHUCKLE, 20);
    SetNickname(&gPlayerParty[0], sShuckieTestNickname);
    MakeMon(1, SPECIES_RATTATA, 1);
    SetMonData(&gPlayerParty[1], MON_DATA_IS_EGG, &isEgg);
    CalculatePlayerPartyCount();
    memcpy(partyBefore, gPlayerParty, sizeof(partyBefore));
    EXPECT_EQ(RemoveNamed(JOHTO_QUEST_NAMED_GIFT_SHUCKIE), JOHTO_QUEST_RESULT_NAMED_CANT_GIVE);
    EXPECT_EQ(memcmp(partyBefore, gPlayerParty, sizeof(partyBefore)), 0);
}

TEST("Johto generic return uses full species and party index payloads")
{
    struct Pokemon partyBefore[PARTY_SIZE];
    struct Mail mailBefore[MAIL_COUNT];
    u16 targetSlot;
    u16 sourceSlot;
    u16 survivorSlot;

    for (targetSlot = 0; targetSlot < 3; targetSlot++)
    {
        ResetQuestFixture();
        MakeMon(0, SPECIES_RATTATA, 5);
        MakeMon(1, SPECIES_RATTATA, 5);
        MakeMon(2, SPECIES_RATTATA, 5);
        ZeroMonData(&gPlayerParty[targetSlot]);
        MakeMon(targetSlot, SPECIES_MAGIKARP, 20);
        GiveMailToMonByItemId(&gPlayerParty[(targetSlot + 1) % 3], ITEM_RETRO_MAIL);
        CalculatePlayerPartyCount();
        memcpy(partyBefore, gPlayerParty, sizeof(partyBefore));
        memcpy(mailBefore, gSaveBlock1Ptr->mail, sizeof(mailBefore));
        EXPECT_EQ(RemoveGeneric(SPECIES_MAGIKARP, targetSlot), JOHTO_QUEST_RESULT_MAGIKARP);
        EXPECT_EQ(memcmp(mailBefore, gSaveBlock1Ptr->mail, sizeof(mailBefore)), 0);
        survivorSlot = 0;
        for (sourceSlot = 0; sourceSlot < 3; sourceSlot++)
            if (sourceSlot != targetSlot)
            {
                EXPECT_EQ(memcmp(&partyBefore[sourceSlot], &gPlayerParty[survivorSlot], sizeof(struct Pokemon)), 0);
                survivorSlot++;
            }
        EXPECT_EQ(gPartiesCount[B_TRAINER_0], 2);
        EXPECT_EQ(GetMonData(&gPlayerParty[0], MON_DATA_SPECIES), SPECIES_RATTATA);
        EXPECT_EQ(GetMonData(&gPlayerParty[1], MON_DATA_SPECIES), SPECIES_RATTATA);
    }

    ResetQuestFixture();
    MakeMon(0, SPECIES_RATTATA, 5);
    MakeMon(1, SPECIES_MAGIKARP, 99);
    MakeMon(2, SPECIES_RATTATA, 5);
    CalculatePlayerPartyCount();
    EXPECT_EQ(RemoveGeneric(SPECIES_MAGIKARP, 1), JOHTO_QUEST_RESULT_MAGIKARP);
    EXPECT_EQ(GetMonData(&gPlayerParty[1], MON_DATA_SPECIES), SPECIES_RATTATA);
    EXPECT_EQ(gPartiesCount[B_TRAINER_0], 2);

    ResetQuestFixture();
    MakeMon(0, SPECIES_RATTATA, 5);
    MakeMon(1, SPECIES_MAGIKARP, MAX_LEVEL);
    CalculatePlayerPartyCount();
    EXPECT_EQ(RemoveGeneric(SPECIES_MAGIKARP, 1), JOHTO_QUEST_RESULT_MAGIKARP_LEVEL_100);
    EXPECT_EQ(GetMonData(&gPlayerParty[0], MON_DATA_SPECIES), SPECIES_RATTATA);

    ResetQuestFixture();
    MakeMon(0, SPECIES_MAGIKARP, 20);
    CalculatePlayerPartyCount();
    memcpy(partyBefore, gPlayerParty, sizeof(partyBefore));
    EXPECT_EQ(RemoveGeneric(SPECIES_MAGIKARP, 0xFFFF), JOHTO_QUEST_RESULT_FAILURE);
    EXPECT_EQ(RemoveGeneric(SPECIES_RATTATA, 0), JOHTO_QUEST_RESULT_FAILURE);
    EXPECT_EQ(memcmp(partyBefore, gPlayerParty, sizeof(partyBefore)), 0);
}

TEST("Johto Baoba checks accept every donor list and reject invalid inputs")
{
    static const enum Species sBaobaLists[][8] = {
        {SPECIES_CACNEA, SPECIES_LOTAD, SPECIES_MAKUHITA, SPECIES_LOMBRE, SPECIES_TRAPINCH, SPECIES_BELDUM, SPECIES_VIBRAVA, SPECIES_NONE},
        {SPECIES_TROPIUS, SPECIES_CHIMECHO, SPECIES_ABSOL, SPECIES_CASTFORM, SPECIES_NONE, SPECIES_NONE, SPECIES_NONE, SPECIES_NONE},
        {SPECIES_BARBOACH, SPECIES_WHISCASH, SPECIES_MEDITITE, SPECIES_NUMEL, SPECIES_BALTOY, SPECIES_ABSOL, SPECIES_MEDICHAM, SPECIES_CAMERUPT},
        {SPECIES_WHISMUR, SPECIES_NOSEPASS, SPECIES_BAGON, SPECIES_RELICANTH, SPECIES_FEEBAS, SPECIES_NONE, SPECIES_NONE, SPECIES_NONE},
    };
    u16 checkId;
    u16 i;

    for (checkId = 0; checkId < ARRAY_COUNT(sBaobaLists); checkId++)
    {
        for (i = 0; i < ARRAY_COUNT(sBaobaLists[checkId]); i++)
        {
            if (sBaobaLists[checkId][i] == SPECIES_NONE)
                break;
            ResetQuestFixture();
            MakeMon(0, sBaobaLists[checkId][i], 20);
            EXPECT_EQ(CheckBaoba(checkId + 1, 0), TRUE);
        }
    }

    ResetQuestFixture();
    MakeMon(0, SPECIES_RATTATA, 20);
    EXPECT_EQ(CheckBaoba(1, 0), FALSE);
    EXPECT_EQ(CheckBaoba(0, 0), FALSE);
    EXPECT_EQ(CheckBaoba(5, 0), FALSE);
    EXPECT_EQ(CheckBaoba(1, PARTY_SIZE), FALSE);
}

TEST("Johto quest returns reject PC and shared mail links without mutation")
{
    struct Pokemon partyBefore[PARTY_SIZE];
    struct Mail mailBefore[MAIL_COUNT];
    u32 named;
    u32 shared;
    u8 mailId;
    u16 heldItem;

    for (named = 0; named < 2; named++)
    {
        for (shared = 0; shared < 2; shared++)
        {
            ResetQuestFixture();
            if (named)
                EXPECT_EQ(GiveNamedGift(JOHTO_QUEST_NAMED_GIFT_KENYA), MON_GIVEN_TO_PARTY);
            else
            {
                MakeMon(0, SPECIES_MAGIKARP, 99);
                GiveMailToMonByItemId(&gPlayerParty[0], ITEM_RETRO_MAIL);
            }
            MakeMon(1, SPECIES_RATTATA, 5);
            CalculatePlayerPartyCount();
            mailId = GetMonData(&gPlayerParty[0], MON_DATA_MAIL);
            heldItem = GetMonData(&gPlayerParty[0], MON_DATA_HELD_ITEM);
            if (shared)
            {
                SetMonData(&gPlayerParty[1], MON_DATA_MAIL, &mailId);
                SetMonData(&gPlayerParty[1], MON_DATA_HELD_ITEM, &heldItem);
            }
            else
            {
                gSaveBlock1Ptr->mail[PARTY_SIZE] = gSaveBlock1Ptr->mail[mailId];
                mailId = PARTY_SIZE;
                SetMonData(&gPlayerParty[0], MON_DATA_MAIL, &mailId);
            }
            memcpy(partyBefore, gPlayerParty, sizeof(partyBefore));
            memcpy(mailBefore, gSaveBlock1Ptr->mail, sizeof(mailBefore));
            if (named)
                EXPECT_EQ(RemoveNamed(JOHTO_QUEST_NAMED_GIFT_KENYA), JOHTO_QUEST_RESULT_NAMED_CANT_GIVE);
            else
                EXPECT_EQ(RemoveGeneric(SPECIES_MAGIKARP, 0), JOHTO_QUEST_RESULT_FAILURE);
            EXPECT_EQ(memcmp(partyBefore, gPlayerParty, sizeof(partyBefore)), 0);
            EXPECT_EQ(memcmp(mailBefore, gSaveBlock1Ptr->mail, sizeof(mailBefore)), 0);
            EXPECT_EQ(gPartiesCount[B_TRAINER_0], 2);
        }
    }
}

TEST("Johto Baoba checks reject eggs empty slots and full-width cancellation")
{
    struct Pokemon partyBefore[PARTY_SIZE];
    bool8 isEgg = TRUE;

    ResetQuestFixture();
    MakeMon(0, SPECIES_CACNEA, 20);
    SetMonData(&gPlayerParty[0], MON_DATA_IS_EGG, &isEgg);
    CalculatePlayerPartyCount();
    memcpy(partyBefore, gPlayerParty, sizeof(partyBefore));
    EXPECT_EQ(CheckBaoba(1, 0), FALSE);
    EXPECT_EQ(CheckBaoba(1, 1), FALSE);
    EXPECT_EQ(CheckBaoba(1, 0xFFFF), FALSE);
    EXPECT_EQ(CheckBaoba(1, 0x100), FALSE);
    EXPECT_EQ(memcmp(partyBefore, gPlayerParty, sizeof(partyBefore)), 0);
    EXPECT_EQ(gPartiesCount[B_TRAINER_0], 1);
}

TEST("Johto generic return rejects eggs last members and a truncating index")
{
    struct Pokemon partyBefore[PARTY_SIZE];
    struct Mail mailBefore[MAIL_COUNT];
    bool8 isEgg;
    u32 scenario;

    for (scenario = 0; scenario < 3; scenario++)
    {
        ResetQuestFixture();
        MakeMon(0, SPECIES_MAGIKARP, 99);
        MakeMon(1, SPECIES_RATTATA, 5);
        isEgg = TRUE;
        if (scenario < 2)
            SetMonData(&gPlayerParty[scenario], MON_DATA_IS_EGG, &isEgg);
        CalculatePlayerPartyCount();
        memcpy(partyBefore, gPlayerParty, sizeof(partyBefore));
        memcpy(mailBefore, gSaveBlock1Ptr->mail, sizeof(mailBefore));
        EXPECT_EQ(RemoveGeneric(SPECIES_MAGIKARP, scenario == 2 ? 0x100 : 0), JOHTO_QUEST_RESULT_FAILURE);
        EXPECT_EQ(memcmp(partyBefore, gPlayerParty, sizeof(partyBefore)), 0);
        EXPECT_EQ(memcmp(mailBefore, gSaveBlock1Ptr->mail, sizeof(mailBefore)), 0);
        EXPECT_EQ(gPartiesCount[B_TRAINER_0], 2);
    }
}

TEST("Johto quest native effect analysis stops mutations and permits Baoba")
{
    struct Pokemon partyBefore[PARTY_SIZE];
    struct Mail mailBefore[MAIL_COUNT];
    u8 program[8];
    u32 native;
    u32 pointer;
    u32 i;

    for (native = 0; native < 3; native++)
    {
        ResetQuestFixture();
        if (native == 0)
            EXPECT_EQ(GiveNamedGift(JOHTO_QUEST_NAMED_GIFT_KENYA), MON_GIVEN_TO_PARTY);
        else
            MakeMon(0, native == 1 ? SPECIES_MAGIKARP : SPECIES_CACNEA, 20);
        MakeMon(1, SPECIES_RATTATA, 5);
        CalculatePlayerPartyCount();
        VarSet(VAR_0x8004, 0);
        memcpy(partyBefore, gPlayerParty, sizeof(partyBefore));
        memcpy(mailBefore, gSaveBlock1Ptr->mail, sizeof(mailBefore));
        pointer = (uintptr_t)(native == 0 ? Script_JohtoRemoveNamedMon
                           : native == 1 ? Script_JohtoRemoveGenericMon
                                         : Script_JohtoBaobaCheckMon) | 0x0A000000;
        program[0] = 0x23; // callnative; matches the actual assembler fixture.
        for (i = 0; i < 4; i++)
            program[i + 1] = pointer >> (8 * i);
        PutHalfword(program + 5, native == 1 ? SPECIES_MAGIKARP : 1);
        program[native == 2 ? 6 : 7] = 0x02; // end
        EXPECT_EQ(RunScriptImmediatelyUntilEffect(SCREFF_V1 | SCREFF_SAVE, program, NULL), native != 2);
        if (native == 2)
            EXPECT_EQ(gSpecialVar_Result, TRUE);
        EXPECT_EQ(memcmp(partyBefore, gPlayerParty, sizeof(partyBefore)), 0);
        EXPECT_EQ(memcmp(mailBefore, gSaveBlock1Ptr->mail, sizeof(mailBefore)), 0);
        EXPECT_EQ(gPartiesCount[B_TRAINER_0], 2);
    }
}
