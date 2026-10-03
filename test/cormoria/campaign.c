#include "global.h"
#include "event_data.h"
#include "load_save.h"
#include "pokedex.h"
#include "pokemon.h"
#include "pokemon_storage_system.h"
#include "test/overworld_script.h"
#include "test/test.h"

// Native gift execution only. The full lab entry needs object/UI fixtures.
static const u16 sStarters[] = {SPECIES_GOTHITA, SPECIES_TIMBURR, SPECIES_ZIGZAGOON_GALAR};

static void ResetStarterStorage(bool32 partyFull, bool32 pcFull, struct BoxPokemon *occupied)
{
    u32 box, slot;
    struct Pokemon mon;
    SetSaveBlocksPointers(0);
    ZeroPlayerPartyMons();
    ResetPokemonStorageSystem();
    ResetPokedex();
    CreateMonWithIVs(&mon, SPECIES_RATTATA, 5, 1, OTID_STRUCT_PLAYER_ID, USE_RANDOM_IVS);
    *occupied = mon.box;
    if (partyFull)
        for (slot = 0; slot < PARTY_SIZE; slot++)
            gPlayerParty[slot] = mon;
    CalculatePlayerPartyCount();
    if (pcFull)
        for (box = 0; box < TOTAL_BOXES_COUNT; box++)
            for (slot = 0; slot < IN_BOX_COUNT; slot++)
                SetBoxMonAt(box, slot, occupied);
}

static void GiveStarter(u16 species, bool32 shiny)
{
    gSpecialVar_0x8004 = species;
    if (shiny)
        RUN_OVERWORLD_SCRIPT(givemon VAR_0x8004, 5, ITEM_ORAN_BERRY, shinyMode=SHINY_MODE_ALWAYS;);
    else
        RUN_OVERWORLD_SCRIPT(givemon VAR_0x8004, 5, ITEM_ORAN_BERRY, shinyMode=SHINY_MODE_NEVER;);
}

static void ExpectStarter(struct Pokemon *mon, u16 species, bool32 shiny)
{
    EXPECT_EQ(GetMonData(mon, MON_DATA_SPECIES), species);
    EXPECT_EQ(GetMonData(mon, MON_DATA_LEVEL), 5);
    EXPECT_EQ(GetMonData(mon, MON_DATA_HELD_ITEM), ITEM_ORAN_BERRY);
    EXPECT_EQ(IsMonShiny(mon), shiny);
}

TEST("Cormoria starter uses party space even when every PC slot is full")
{
    u32 i, variant, box, slot;
    u16 species = SPECIES_GOTHITA;
    bool32 shiny = FALSE;
    struct BoxPokemon occupied;
    for (i = 0; i < ARRAY_COUNT(sStarters); i++)
        for (variant = 0; variant < 2; variant++)
            PARAMETRIZE { species = sStarters[i]; shiny = variant; }
    ResetStarterStorage(FALSE, TRUE, &occupied);
    EXPECT(!CheckFreePokemonStorageSpace());
    GiveStarter(species, shiny);
    EXPECT_EQ(gSpecialVar_Result, MON_GIVEN_TO_PARTY);
    EXPECT_EQ(gPlayerPartyCount, 1);
    ExpectStarter(&gPlayerParty[0], species, shiny);
    for (box = 0; box < TOTAL_BOXES_COUNT; box++)
        for (slot = 0; slot < IN_BOX_COUNT; slot++)
            EXPECT_EQ(memcmp(GetBoxedMonPtr(box, slot), &occupied, sizeof(occupied)), 0);
}

TEST("Cormoria starter uses PC space when all party slots are occupied")
{
    u32 i, variant;
    u16 species = SPECIES_GOTHITA;
    bool32 shiny = FALSE;
    struct BoxPokemon occupied;
    struct Pokemon partyBefore[PARTY_SIZE], delivered;
    for (i = 0; i < ARRAY_COUNT(sStarters); i++)
        for (variant = 0; variant < 2; variant++)
            PARAMETRIZE { species = sStarters[i]; shiny = variant; }
    ResetStarterStorage(TRUE, FALSE, &occupied);
    memcpy(partyBefore, gPlayerParty, sizeof(partyBefore));
    EXPECT(CheckFreePokemonStorageSpace());
    GiveStarter(species, shiny);
    EXPECT_EQ(gSpecialVar_Result, MON_GIVEN_TO_PC);
    EXPECT_EQ(gPlayerPartyCount, PARTY_SIZE);
    EXPECT_EQ(memcmp(partyBefore, gPlayerParty, sizeof(partyBefore)), 0);
    EXPECT_EQ(CountAllStorageMons(), 1);
    BoxMonAtToMon(0, 0, &delivered);
    ExpectStarter(&delivered, species, shiny);
}

TEST("Cormoria starter full storage rejects without losing data and permits retry")
{
    u32 i, variant, box, slot;
    u16 species = SPECIES_GOTHITA;
    bool32 shiny = FALSE;
    struct BoxPokemon occupied;
    struct Pokemon partyBefore[PARTY_SIZE], delivered;
    for (i = 0; i < ARRAY_COUNT(sStarters); i++)
        for (variant = 0; variant < 2; variant++)
            PARAMETRIZE { species = sStarters[i]; shiny = variant; }
    ResetStarterStorage(TRUE, TRUE, &occupied);
    memcpy(partyBefore, gPlayerParty, sizeof(partyBefore));
    EXPECT(!CheckFreePokemonStorageSpace());
    GiveStarter(species, shiny);
    EXPECT_EQ(gSpecialVar_Result, MON_CANT_GIVE);
    EXPECT_EQ(gPlayerPartyCount, PARTY_SIZE);
    EXPECT_EQ(memcmp(partyBefore, gPlayerParty, sizeof(partyBefore)), 0);
    EXPECT(!GetSetPokedexFlag(SpeciesToNationalPokedexNum(species), FLAG_GET_CAUGHT));
    for (box = 0; box < TOTAL_BOXES_COUNT; box++)
        for (slot = 0; slot < IN_BOX_COUNT; slot++)
            EXPECT_EQ(memcmp(GetBoxedMonPtr(box, slot), &occupied, sizeof(occupied)), 0);
    ZeroBoxMonAt(TOTAL_BOXES_COUNT - 1, IN_BOX_COUNT - 1);
    EXPECT(CheckFreePokemonStorageSpace());
    GiveStarter(species, shiny);
    EXPECT_EQ(gSpecialVar_Result, MON_GIVEN_TO_PC);
    EXPECT_EQ(memcmp(partyBefore, gPlayerParty, sizeof(partyBefore)), 0);
    BoxMonAtToMon(TOTAL_BOXES_COUNT - 1, IN_BOX_COUNT - 1, &delivered);
    ExpectStarter(&delivered, species, shiny);
    EXPECT_EQ(CountAllStorageMons(), TOTAL_BOXES_COUNT * IN_BOX_COUNT);
    EXPECT(GetSetPokedexFlag(SpeciesToNationalPokedexNum(species), FLAG_GET_CAUGHT));
    for (box = 0; box < TOTAL_BOXES_COUNT; box++)
        for (slot = 0; slot < IN_BOX_COUNT; slot++)
            if (box != TOTAL_BOXES_COUNT - 1 || slot != IN_BOX_COUNT - 1)
                EXPECT_EQ(memcmp(GetBoxedMonPtr(box, slot), &occupied, sizeof(occupied)), 0);
}
