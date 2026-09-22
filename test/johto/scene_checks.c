#include "global.h"
#include "event_object_movement.h"
#include "load_save.h"
#include "pokemon.h"
#include "test/test.h"
#include "johto/scene_checks.h"
#include "constants/event_objects.h"
#include "constants/species.h"

static void ResetSceneFixture(void)
{
    SetSaveBlocksPointers(0);
    memset(gPlayerParty, 0, sizeof(gPlayerParty));
    gPlayerPartyCount = 0;
    memset(gObjectEvents, 0, sizeof(gObjectEvents));
}

static void SetLeadSpecies(enum Species species)
{
    CreateMon(&gPlayerParty[0], species, 50, 0, OTID_STRUCT_PLAYER_ID);
    CalculateMonStats(&gPlayerParty[0]);
    gPlayerPartyCount = 1;
}

static void SetFollower(bool8 invisible)
{
    gObjectEvents[0].active = TRUE;
    gObjectEvents[0].localId = OBJ_EVENT_ID_FOLLOWER;
    gObjectEvents[0].invisible = invisible;
}

TEST("Johto scene checks accept only the required lead species")
{
    ResetSceneFixture();
    EXPECT(!Johto_CheckHooh());

    SetLeadSpecies(SPECIES_HO_OH);
    EXPECT(Johto_CheckHooh());
    EXPECT(!Johto_CheckAerodactyl());
    EXPECT(!Johto_CheckKabuto());
    EXPECT(!Johto_CheckOmanyte());

    SetLeadSpecies(SPECIES_AERODACTYL);
    EXPECT(Johto_CheckAerodactyl());
    SetLeadSpecies(SPECIES_KABUTO);
    EXPECT(Johto_CheckKabuto());
    SetLeadSpecies(SPECIES_OMANYTE);
    EXPECT(Johto_CheckOmanyte());

    SetLeadSpecies(SPECIES_RATTATA);
    CreateMon(&gPlayerParty[1], SPECIES_HO_OH, 50, 0, OTID_STRUCT_PLAYER_ID);
    gPlayerPartyCount = 2;
    EXPECT(!Johto_CheckHooh());
}

TEST("Johto Togepi scene check accepts evolutions and rejects eggs")
{
    bool8 isEgg = TRUE;

    ResetSceneFixture();
    SetLeadSpecies(SPECIES_TOGEPI);
    EXPECT(Johto_CheckTogepi());
    SetLeadSpecies(SPECIES_TOGETIC);
    EXPECT(Johto_CheckTogepi());
    SetLeadSpecies(SPECIES_TOGEKISS);
    EXPECT(Johto_CheckTogepi());

    SetLeadSpecies(SPECIES_TOGEPI);
    SetMonData(&gPlayerParty[0], MON_DATA_IS_EGG, &isEgg);
    EXPECT(!Johto_CheckTogepi());
}

TEST("Johto Celebi scene check requires a full health visible follower")
{
    bool8 isEgg = TRUE;
    u16 hp;

    ResetSceneFixture();
    SetLeadSpecies(SPECIES_CELEBI);
    SetFollower(FALSE);
    EXPECT(Johto_CheckCelebi());

    hp = GetMonData(&gPlayerParty[0], MON_DATA_MAX_HP) - 1;
    SetMonData(&gPlayerParty[0], MON_DATA_HP, &hp);
    EXPECT(!Johto_CheckCelebi());
    hp = 0;
    SetMonData(&gPlayerParty[0], MON_DATA_HP, &hp);
    EXPECT(!Johto_CheckCelebi());

    CalculateMonStats(&gPlayerParty[0]);
    hp = GetMonData(&gPlayerParty[0], MON_DATA_MAX_HP);
    SetMonData(&gPlayerParty[0], MON_DATA_HP, &hp);
    memset(gObjectEvents, 0, sizeof(gObjectEvents));
    EXPECT(!Johto_CheckCelebi());

    SetFollower(FALSE);
    SetLeadSpecies(SPECIES_RATTATA);
    CreateMon(&gPlayerParty[1], SPECIES_CELEBI, 50, 0, OTID_STRUCT_PLAYER_ID);
    EXPECT(!Johto_CheckCelebi());

    SetLeadSpecies(SPECIES_CELEBI);
    SetMonData(&gPlayerParty[0], MON_DATA_IS_EGG, &isEgg);
    EXPECT(!Johto_CheckCelebi());

    SetLeadSpecies(SPECIES_CELEBI);
    SetFollower(TRUE);
    EXPECT(!Johto_CheckCelebi());
}

TEST("Johto scene checks do not mutate party or follower state")
{
    struct Pokemon partyBefore[PARTY_SIZE];
    struct ObjectEvent objectsBefore[OBJECT_EVENTS_COUNT];
    u8 partyCountBefore;

    ResetSceneFixture();
    SetLeadSpecies(SPECIES_CELEBI);
    SetFollower(FALSE);
    memcpy(partyBefore, gPlayerParty, sizeof(partyBefore));
    memcpy(objectsBefore, gObjectEvents, sizeof(objectsBefore));
    partyCountBefore = gPlayerPartyCount;

    Johto_CheckHooh();
    Johto_CheckAerodactyl();
    Johto_CheckKabuto();
    Johto_CheckOmanyte();
    Johto_CheckTogepi();
    Johto_CheckCelebi();

    EXPECT_EQ(memcmp(partyBefore, gPlayerParty, sizeof(partyBefore)), 0);
    EXPECT_EQ(memcmp(objectsBefore, gObjectEvents, sizeof(objectsBefore)), 0);
    EXPECT_EQ(gPlayerPartyCount, partyCountBefore);
}
