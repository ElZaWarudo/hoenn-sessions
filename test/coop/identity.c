#include "global.h"
#include "battle_setup.h"
#include "coop/generated_regional_identities.h"
#include "coop/identity.h"
#include "coop/save.h"
#include "constants/flags.h"
#include "constants/opponents.h"
#include "constants/region_map_sections.h"
#include "event_data.h"
#include "test/test.h"

#define HOENN_TRAINER_IDENTITY_COUNT COOP_TRAINER_KANTO_TRAINER_BROCK_ORDINAL

static void SetActiveRegion(u8 engineRegion, u32 sectionId)
{
    gMapHeader.engineRegion = engineRegion;
    gMapHeader.regionMapSectionId = sectionId;
}

TEST("Cloud Coop trainer registry resolves every Hoenn opponent without raw flag identity")
{
    u16 i;

    EXPECT_EQ(HOENN_TRAINER_IDENTITY_COUNT, 854);
    EXPECT_EQ(COOP_TRAINER_IDENTITY_COUNT, 856);
    for (i = 0; i < HOENN_TRAINER_IDENTITY_COUNT; i++)
    {
        const struct CoopIdentityRegistryEntry *entry = &gCoopTrainerIdentityRegistry[i];
        u16 ordinal = COOP_IDENTITY_ORDINAL_NONE;

        EXPECT_EQ(entry->ordinal, i);
        EXPECT_EQ(entry->legacy_value, i + 1);
        EXPECT_EQ(entry->region, COOP_REGION_HOENN);
        EXPECT(CoopIdentity_ResolveTrainerOrdinal(COOP_REGION_HOENN, i + 1, &ordinal));
        EXPECT_EQ(ordinal, i);
    }

    EXPECT(!CoopIdentity_ResolveTrainerOrdinal(COOP_REGION_HOENN, 0, NULL));
    EXPECT(!CoopIdentity_ResolveTrainerOrdinal(COOP_REGION_HOENN,
                                               HOENN_TRAINER_IDENTITY_COUNT + 1,
                                               NULL));
    EXPECT(!CoopIdentity_ResolveTrainerOrdinal(COOP_REGION_UNSPECIFIED,
                                               TRAINER_WALLY_VR_1,
                                               NULL));
}

TEST("Cloud Coop Wally and Brock identities remain isolated by active region")
{
    u16 ordinal = COOP_IDENTITY_ORDINAL_NONE;

    EXPECT(CoopIdentity_ResolveTrainerOrdinal(COOP_REGION_HOENN,
                                              TRAINER_WALLY_VR_1,
                                              &ordinal));
    EXPECT_EQ(ordinal, COOP_TRAINER_HOENN_TRAINER_WALLY_1_ORDINAL);
    EXPECT(CoopIdentity_ResolveTrainerOrdinal(COOP_REGION_KANTO,
                                              TRAINER_LEADER_BROCK,
                                              &ordinal));
    EXPECT_EQ(ordinal, COOP_TRAINER_KANTO_TRAINER_BROCK_ORDINAL);

    EXPECT(!CoopIdentity_ResolveTrainerOrdinal(COOP_REGION_KANTO,
                                               TRAINER_WALLY_VR_1,
                                               &ordinal));
    EXPECT(!CoopIdentity_ResolveTrainerOrdinal(COOP_REGION_HOENN,
                                               TRAINER_LEADER_BROCK,
                                               &ordinal));
    EXPECT(!CoopIdentity_ResolveTrainerOrdinal(COOP_REGION_SEVII,
                                               TRAINER_LEADER_BROCK,
                                               &ordinal));
    EXPECT(!CoopIdentity_ResolveTrainerOrdinal(COOP_REGION_JOHTO,
                                               COOP_IDENTITY_LEGACY_NONE,
                                               &ordinal));
}

TEST("Cloud Coop online trainer access handles set clear toggle and two trainers")
{
    bool8 defeated = TRUE;

    CoopSave_InitializeCurrent();
    SetActiveRegion(COOP_MAP_ENGINE_REGION_HOENN, MAPSEC_LITTLEROOT_TOWN);

    EXPECT_EQ(CoopIdentity_GetTrainerDefeated(TRAINER_WALLY_VR_1, &defeated),
              COOP_IDENTITY_ACCESS_HANDLED);
    EXPECT(!defeated);
    EXPECT_EQ(CoopIdentity_SetTrainerDefeated(TRAINER_WALLY_VR_1, TRUE),
              COOP_IDENTITY_ACCESS_HANDLED);
    EXPECT(CoopSave_GetTrainerDefeated(COOP_TRAINER_HOENN_TRAINER_WALLY_1_ORDINAL));
    EXPECT(!FlagGet(TRAINER_FLAGS_START + TRAINER_WALLY_VR_1));

    EXPECT_EQ(CoopIdentity_SetTrainerDefeated(TRAINER_WALLY_VR_1, FALSE),
              COOP_IDENTITY_ACCESS_HANDLED);
    EXPECT(!CoopSave_GetTrainerDefeated(COOP_TRAINER_HOENN_TRAINER_WALLY_1_ORDINAL));

    SetTrainerFlag(TRAINER_WALLY_VR_1);
    SetTrainerFlag(TRAINER_WALLY_MAUVILLE);
    EXPECT(HasTrainerBeenFought(TRAINER_WALLY_VR_1));
    EXPECT(HasTrainerBeenFought(TRAINER_WALLY_MAUVILLE));
    EXPECT(!FlagGet(TRAINER_FLAGS_START + TRAINER_WALLY_VR_1));
    EXPECT(!FlagGet(TRAINER_FLAGS_START + TRAINER_WALLY_MAUVILLE));

    ToggleTrainerFlag(TRAINER_WALLY_VR_1);
    EXPECT(!HasTrainerBeenFought(TRAINER_WALLY_VR_1));
    ToggleTrainerFlag(TRAINER_WALLY_VR_1);
    EXPECT(HasTrainerBeenFought(TRAINER_WALLY_VR_1));
    ClearTrainerFlag(TRAINER_WALLY_VR_1);
    ClearTrainerFlag(TRAINER_WALLY_MAUVILLE);
    EXPECT(!HasTrainerBeenFought(TRAINER_WALLY_VR_1));
    EXPECT(!HasTrainerBeenFought(TRAINER_WALLY_MAUVILLE));
    EXPECT(CoopSave_Validate(&gSaveBlock3Ptr->coop));
}

TEST("Cloud Coop online trainer access rejects wrong region and unregistered opponents")
{
    bool8 defeated = TRUE;

    CoopSave_InitializeCurrent();
    SetActiveRegion(COOP_MAP_ENGINE_REGION_HOENN, MAPSEC_LITTLEROOT_TOWN);

    EXPECT_EQ(CoopIdentity_GetTrainerDefeated(TRAINER_LEADER_BROCK, &defeated),
              COOP_IDENTITY_ACCESS_REJECTED);
    EXPECT(defeated);
    EXPECT_EQ(CoopIdentity_SetTrainerDefeated(TRAINER_LEADER_BROCK, TRUE),
              COOP_IDENTITY_ACCESS_REJECTED);
    EXPECT_EQ(CoopIdentity_SetTrainerDefeated(0, TRUE), COOP_IDENTITY_ACCESS_REJECTED);

    SetTrainerFlag(TRAINER_LEADER_BROCK);
    EXPECT(!CoopSave_GetTrainerDefeated(COOP_TRAINER_KANTO_TRAINER_BROCK_ORDINAL));

    SetActiveRegion(COOP_MAP_ENGINE_REGION_KANTO, MAPSEC_PALLET_TOWN);
    SetTrainerFlag(TRAINER_LEADER_BROCK);
    EXPECT(HasTrainerBeenFought(TRAINER_LEADER_BROCK));
    EXPECT(CoopSave_GetTrainerDefeated(COOP_TRAINER_KANTO_TRAINER_BROCK_ORDINAL));

    SetActiveRegion(COOP_MAP_ENGINE_REGION_KANTO, MAPSEC_ONE_ISLAND);
    ClearTrainerFlag(TRAINER_LEADER_BROCK);
    EXPECT(CoopSave_GetTrainerDefeated(COOP_TRAINER_KANTO_TRAINER_BROCK_ORDINAL));
    EXPECT(!HasTrainerBeenFought(TRAINER_LEADER_BROCK));

    SetActiveRegion(REGION_JOHTO, MAPSEC_NONE);
    SetTrainerFlag(TRAINER_WALLY_VR_1);
    EXPECT(!CoopSave_GetTrainerDefeated(COOP_TRAINER_HOENN_TRAINER_WALLY_1_ORDINAL));
}

TEST("Cloud Coop invalid and ambiguous saves retain legacy trainer flags")
{
    CoopSave_InitializeCurrent();
    SetActiveRegion(COOP_MAP_ENGINE_REGION_HOENN, MAPSEC_LITTLEROOT_TOWN);
    gSaveBlock3Ptr->coop.magic ^= 1;
    EXPECT_EQ(CoopSave_Load(), COOP_SAVE_LOAD_INCOMPATIBLE);

    SetTrainerFlag(TRAINER_WALLY_VR_1);
    EXPECT(FlagGet(TRAINER_FLAGS_START + TRAINER_WALLY_VR_1));
    EXPECT(HasTrainerBeenFought(TRAINER_WALLY_VR_1));
    ToggleTrainerFlag(TRAINER_WALLY_VR_1);
    EXPECT(!FlagGet(TRAINER_FLAGS_START + TRAINER_WALLY_VR_1));
    SetTrainerFlag(TRAINER_WALLY_VR_1);
    ClearTrainerFlag(TRAINER_WALLY_VR_1);
    EXPECT(!FlagGet(TRAINER_FLAGS_START + TRAINER_WALLY_VR_1));

    CoopSave_InitializeCurrent();
    gSaveBlock3Ptr->coop.status_flags = COOP_SAVE_STATUS_MIGRATION_AMBIGUOUS;
    EXPECT(CoopSave_Seal(&gSaveBlock3Ptr->coop));
    SetTrainerFlag(TRAINER_WALLY_VR_1);
    EXPECT(FlagGet(TRAINER_FLAGS_START + TRAINER_WALLY_VR_1));
    EXPECT(!CoopSave_GetTrainerDefeated(COOP_TRAINER_HOENN_TRAINER_WALLY_1_ORDINAL));
}
