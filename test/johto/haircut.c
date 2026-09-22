#include "global.h"
#include "battle.h"
#include "constants/items.h"
#include "constants/maps.h"
#include "constants/region_map_sections.h"
#include "constants/party_menu.h"
#include "constants/species.h"
#include "event_data.h"
#include "johto/haircut.h"
#include "load_save.h"
#include "main.h"
#include "pokemon.h"
#include "test/test.h"

static void ResetHaircutFixture(void)
{
    SetSaveBlocksPointers(0);
    memset(gParties, 0, sizeof(gParties));
    memset(gPartiesCount, 0, sizeof(gPartiesCount));
    gMain.inBattle = FALSE;
    gBattleTypeFlags = 0;
    gSaveBlock1Ptr->location.mapGroup = MAP_GROUP(MAP_LITTLEROOT_TOWN);
    gSaveBlock1Ptr->location.mapNum = MAP_NUM(MAP_LITTLEROOT_TOWN);
}

static void MakeMon(u16 slot, u16 friendship)
{
    CreateMon(&gPlayerParty[slot], SPECIES_RATTATA, 5, slot + 1, OTID_STRUCT_PLAYER_ID);
    SetMonData(&gPlayerParty[slot], MON_DATA_FRIENDSHIP, &friendship);
    SetMonData(&gPlayerParty[slot], MON_DATA_MET_LOCATION, &(u16){MAPSEC_ROUTE_101});
}

TEST("Haircut Brother 1 applies donor +99 through the production friendship helpers")
{
    static const u16 levels[] = {0, 99, 100, 199, 200, 254, MAX_FRIENDSHIP};
    u16 i;

    for (i = 0; i < ARRAY_COUNT(levels); i++)
    {
        u16 expected = levels[i] + 99;
        if (expected > MAX_FRIENDSHIP)
            expected = MAX_FRIENDSHIP;
        ResetHaircutFixture();
        MakeMon(0, levels[i]);
        CalculatePlayerPartyCount();
        EXPECT(JohtoHaircut_Apply(0));
        EXPECT_EQ(GetMonData(&gPlayerParty[0], MON_DATA_FRIENDSHIP), expected);
    }
}

TEST("Haircut safely rejects invalid, empty and egg slots without touching another mon")
{
    struct Pokemon before;
    u16 friendship = 80;
    bool8 isEgg = TRUE;

    ResetHaircutFixture();
    MakeMon(0, friendship);
    MakeMon(1, friendship);
    CalculatePlayerPartyCount();
    memcpy(&before, &gPlayerParty[1], sizeof(before));
    EXPECT(!JohtoHaircut_Apply(PARTY_NOTHING_CHOSEN));
    EXPECT_EQ(memcmp(&before, &gPlayerParty[1], sizeof(before)), 0);
    EXPECT(!JohtoHaircut_Apply(PARTY_SIZE));
    EXPECT_EQ(memcmp(&before, &gPlayerParty[1], sizeof(before)), 0);

    ResetHaircutFixture();
    MakeMon(0, friendship);
    SetMonData(&gPlayerParty[0], MON_DATA_IS_EGG, &isEgg);
    CalculatePlayerPartyCount();
    memcpy(&before, &gPlayerParty[0], sizeof(before));
    EXPECT(!JohtoHaircut_Apply(0));
    EXPECT_EQ(memcmp(&before, &gPlayerParty[0], sizeof(before)), 0);

    ResetHaircutFixture();
    EXPECT(!JohtoHaircut_Apply(0));
}

TEST("Haircut honors host friendship bonuses and skip policy")
{
    u16 friendship = 0;
    u16 expected;

    ResetHaircutFixture();
    MakeMon(0, friendship);
    SetMonData(&gPlayerParty[0], MON_DATA_HELD_ITEM, &(u16){ITEM_SOOTHE_BELL});
    CalculatePlayerPartyCount();
    expected = 148;
    EXPECT(JohtoHaircut_Apply(0));
    EXPECT_EQ(GetMonData(&gPlayerParty[0], MON_DATA_FRIENDSHIP), expected);

    ResetHaircutFixture();
    MakeMon(0, friendship);
    SetMonData(&gPlayerParty[0], MON_DATA_POKEBALL, &(u16){ITEM_LUXURY_BALL});
    CalculatePlayerPartyCount();
    EXPECT(JohtoHaircut_Apply(0));
    EXPECT_EQ(GetMonData(&gPlayerParty[0], MON_DATA_FRIENDSHIP), 100);

    ResetHaircutFixture();
    MakeMon(0, friendship);
    SetMonData(&gPlayerParty[0], MON_DATA_MET_LOCATION, &(u16){MAPSEC_LITTLEROOT_TOWN});
    CalculatePlayerPartyCount();
    EXPECT(JohtoHaircut_Apply(0));
    EXPECT_EQ(GetMonData(&gPlayerParty[0], MON_DATA_FRIENDSHIP), 100);

    ResetHaircutFixture();
    MakeMon(0, friendship);
    CalculatePlayerPartyCount();
    gMain.inBattle = TRUE;
    gBattleTypeFlags = BATTLE_TYPE_FRONTIER;
    EXPECT(JohtoHaircut_Apply(0));
    EXPECT_EQ(GetMonData(&gPlayerParty[0], MON_DATA_FRIENDSHIP), 0);
}

TEST("Haircut special reports success and requests save effects")
{
    ResetHaircutFixture();
    MakeMon(0, 200);
    CalculatePlayerPartyCount();
    gSpecialVar_0x8004 = 0;
    HaircutBrother1();
    EXPECT_EQ(gSpecialVar_Result, TRUE);
    EXPECT_EQ(GetMonData(&gPlayerParty[0], MON_DATA_FRIENDSHIP), MAX_FRIENDSHIP);

    gSpecialVar_0x8004 = PARTY_NOTHING_CHOSEN;
    HaircutBrother1();
    EXPECT_EQ(gSpecialVar_Result, FALSE);
}
