#include "global.h"
#include "event_data.h"
#include "daycare.h"
#include "load_save.h"
#include "mail.h"
#include "pokedex.h"
#include "pokemon.h"
#include "script.h"
#include "script_pokemon_util.h"
#include "string_util.h"
#include "test/test.h"
#include "johto/gifts.h"

static void PutHalfword(u8 *script, u16 value)
{
    script[0] = value;
    script[1] = value >> 8;
}

static const u8 sKenyaTestNickname[] = _("KENYA");
static const u8 sRudyTestOtName[] = _("RUDY");

static void ResetGiftFixture(void)
{
    SetSaveBlocksPointers(0);
    memset(gPlayerParty, 0, sizeof(gPlayerParty));
    memset(gEnemyParty, 0, sizeof(gEnemyParty));
    memset(gPartiesCount, 0, sizeof(gPartiesCount));
    ClearAllMail();
    ResetPokedex();
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

static u32 GiveOddEgg(u16 which)
{
    struct ScriptContext ctx;
    u8 payload[2];

    PutHalfword(payload, which);
    InitScriptContext(&ctx, NULL, NULL);
    ctx.scriptPtr = payload;
    Script_JohtoGiveOddEgg(&ctx);
    EXPECT_EQ(ctx.scriptPtr, payload + sizeof(payload));
    return gSpecialVar_Result;
}

TEST("Johto named gifts preserve reward fields and party bookkeeping")
{
    u8 nickname[POKEMON_NAME_LENGTH + 1];
    u8 otName[PLAYER_NAME_LENGTH + 1];
    u8 mailId;
    struct Pokemon *mon;

    ResetGiftFixture();
    EXPECT_EQ(GiveNamedGift(1), MON_GIVEN_TO_PARTY);
    mon = &gPlayerParty[0];
    GetMonData(mon, MON_DATA_NICKNAME, nickname);
    GetMonData(mon, MON_DATA_OT_NAME, otName);
    mailId = GetMonData(mon, MON_DATA_MAIL);
    EXPECT_EQ(GetMonData(mon, MON_DATA_SPECIES), SPECIES_SPEAROW);
    EXPECT_EQ(GetMonData(mon, MON_DATA_LEVEL), 20);
    EXPECT_EQ(GetMonData(mon, MON_DATA_HELD_ITEM), ITEM_RETRO_MAIL);
    EXPECT_EQ(GetMonData(mon, MON_DATA_OT_ID), 61225);
    EXPECT_EQ(StringCompare(nickname, sKenyaTestNickname), 0);
    EXPECT_EQ(StringCompare(otName, sRudyTestOtName), 0);
    EXPECT(mailId < PARTY_SIZE);
    EXPECT_EQ(gSaveBlock1Ptr->mail[mailId].itemId, ITEM_RETRO_MAIL);
    EXPECT_EQ(gSaveBlock1Ptr->mail[mailId].species, SPECIES_SPEAROW);
    EXPECT_EQ(gPartiesCount[B_TRAINER_0], 1);
    EXPECT_EQ(GetSetPokedexFlag(SpeciesToNationalPokedexNum(SPECIES_SPEAROW), FLAG_GET_CAUGHT), TRUE);

    EXPECT_EQ(GiveNamedGift(2), MON_GIVEN_TO_PARTY);
    mon = &gPlayerParty[1];
    EXPECT_EQ(GetMonData(mon, MON_DATA_SPECIES), SPECIES_SHUCKLE);
    EXPECT_EQ(GetMonData(mon, MON_DATA_LEVEL), 20);
    EXPECT_EQ(GetMonData(mon, MON_DATA_HELD_ITEM), ITEM_BERRY_JUICE);
    EXPECT_EQ(GetMonData(mon, MON_DATA_OT_ID), 4336);

    EXPECT_EQ(GiveNamedGift(3), MON_GIVEN_TO_PARTY);
    mon = &gPlayerParty[2];
    EXPECT_EQ(GetMonData(mon, MON_DATA_SPECIES), SPECIES_EEVEE);
    EXPECT_EQ(GetMonData(mon, MON_DATA_LEVEL), 20);
    EXPECT_EQ(GetMonData(mon, MON_DATA_OT_ID), 5231);

    EXPECT_EQ(GiveNamedGift(4), MON_GIVEN_TO_PARTY);
    mon = &gPlayerParty[3];
    EXPECT_EQ(GetMonData(mon, MON_DATA_SPECIES), SPECIES_DRATINI);
    EXPECT_EQ(GetMonData(mon, MON_DATA_LEVEL), 15);
    EXPECT_EQ(GetMonData(mon, MON_DATA_IS_SHINY), TRUE);
    EXPECT_EQ(GetNatureFromPersonality(GetMonData(mon, MON_DATA_PERSONALITY)), NATURE_ADAMANT);
    EXPECT_EQ(GetMonData(mon, MON_DATA_MOVE1), MOVE_EXTREME_SPEED);
    EXPECT_EQ(GetMonData(mon, MON_DATA_PP1), 5);
    EXPECT(gPartiesCount[B_TRAINER_0] == 4);
}

TEST("Johto Odd Eggs retain egg semantics for every identity")
{
    static const enum Species species[] = {
        SPECIES_PICHU,
        SPECIES_CLEFFA,
        SPECIES_IGGLYBUFF,
        SPECIES_TYROGUE,
        SPECIES_SMOOCHUM,
        SPECIES_ELEKID,
        SPECIES_MAGBY,
    };
    u32 i;

    for (i = 0; i < ARRAY_COUNT(species); i++)
    {
        struct Pokemon *mon;
        ResetGiftFixture();
        EXPECT_EQ(GiveOddEgg(i + 1), MON_GIVEN_TO_PARTY);
        mon = &gPlayerParty[0];
        EXPECT_EQ(GetMonData(mon, MON_DATA_SPECIES), species[i]);
        EXPECT_EQ(GetMonData(mon, MON_DATA_IS_EGG), TRUE);
        EXPECT_EQ(GetMonData(mon, MON_DATA_MOVE2), MOVE_DIZZY_PUNCH);
        EXPECT_EQ(GetMonData(mon, MON_DATA_PP2), 10);
        EXPECT_EQ(GetMonData(mon, MON_DATA_FRIENDSHIP), min(gSpeciesInfo[species[i]].eggCycles, 2));
        EXPECT_EQ(gPartiesCount[B_TRAINER_0], 1);
        EXPECT_EQ(GetSetPokedexFlag(SpeciesToNationalPokedexNum(species[i]), FLAG_GET_CAUGHT), FALSE);
    }
}

TEST("Johto gift capacity failures preserve party and mail")
{
    struct Pokemon partyBefore[PARTY_SIZE];
    struct Mail mailBefore[MAIL_COUNT];
    u8 countBefore;
    u32 i;

    ResetGiftFixture();
    for (i = 0; i < PARTY_SIZE; i++)
        CreateMonWithIVs(&gPlayerParty[i], SPECIES_RATTATA, 5, i + 1, OTID_STRUCT_PLAYER_ID, USE_RANDOM_IVS);
    CalculatePlayerPartyCount();
    memcpy(partyBefore, gPlayerParty, sizeof(partyBefore));
    memcpy(mailBefore, gSaveBlock1Ptr->mail, sizeof(mailBefore));
    countBefore = gPartiesCount[B_TRAINER_0];
    EXPECT_EQ(GiveNamedGift(1), MON_CANT_GIVE);
    EXPECT_EQ(memcmp(partyBefore, gPlayerParty, sizeof(partyBefore)), 0);
    EXPECT_EQ(memcmp(mailBefore, gSaveBlock1Ptr->mail, sizeof(mailBefore)), 0);
    EXPECT_EQ(gPartiesCount[B_TRAINER_0], countBefore);

    ResetGiftFixture();
    for (i = 0; i < PARTY_SIZE; i++)
        gSaveBlock1Ptr->mail[i].itemId = ITEM_RETRO_MAIL;
    CreateMonWithIVs(&gPlayerParty[0], SPECIES_RATTATA, 5, 1, OTID_STRUCT_PLAYER_ID, USE_RANDOM_IVS);
    CalculatePlayerPartyCount();
    memcpy(partyBefore, gPlayerParty, sizeof(partyBefore));
    memcpy(mailBefore, gSaveBlock1Ptr->mail, sizeof(mailBefore));
    countBefore = gPartiesCount[B_TRAINER_0];
    EXPECT_EQ(GiveNamedGift(1), MON_CANT_GIVE);
    EXPECT_EQ(memcmp(partyBefore, gPlayerParty, sizeof(partyBefore)), 0);
    EXPECT_EQ(memcmp(mailBefore, gSaveBlock1Ptr->mail, sizeof(mailBefore)), 0);
    EXPECT_EQ(gPartiesCount[B_TRAINER_0], countBefore);
    EXPECT_EQ(GiveNamedGift(0), MON_CANT_GIVE);
}

TEST("Johto static shiny marker preserves host wild construction")
{
    struct ScriptContext ctx;

    ResetGiftFixture();
    CreateScriptedWildMon(SPECIES_GYARADOS, 30, ITEM_NONE);
    EXPECT_EQ(GetMonData(&gEnemyParty[0], MON_DATA_SPECIES), SPECIES_GYARADOS);
    EXPECT_EQ(GetMonData(&gEnemyParty[0], MON_DATA_LEVEL), 30);
    InitScriptContext(&ctx, NULL, NULL);
    Script_JohtoMarkWildShiny(&ctx);
    EXPECT_EQ(GetMonData(&gEnemyParty[0], MON_DATA_IS_SHINY), TRUE);
    EXPECT(GetMonData(&gEnemyParty[0], MON_DATA_MAX_HP) > 0);
}
