#include "global.h"
#include "data.h"
#include "johto/rival.h"
#include "johto/save.h"
#include "load_save.h"
#include "string_util.h"
#include "test/test.h"
#include "constants/characters.h"
#include "constants/regions.h"
#include "constants/johto_content.h"

TEST("Johto rival name initializes independently from Kanto storage")
{
    static const u8 sSilver[] = _("SILVER");
    u8 kantoName[PLAYER_NAME_LENGTH + 1];

    SetSaveBlocksPointers(0);
    memcpy(kantoName, gSaveBlock2Ptr->rivalName, sizeof(kantoName));
    SetSaveBlocksPointers(0);
    JohtoSave_InitializeCurrent();
    EXPECT_EQ(memcmp(gSaveblock1.johto.rival_name, sSilver, sizeof(sSilver)), 0);
    EXPECT_EQ(memcmp(gSaveBlock2Ptr->rivalName, kantoName, sizeof(kantoName)), 0);
    EXPECT_EQ(JohtoRival_GetName(), gSaveblock1.johto.rival_name);
}

TEST("Johto rival name accepts seven encoded characters and reseals")
{
    static const u8 sCrystal[] = _("CRYSTAL");
    static const u8 sShort[] = _("GOLD");
    struct JohtoSaveV1 snapshot;
    u8 kantoName[PLAYER_NAME_LENGTH + 1];

    SetSaveBlocksPointers(0);
    memcpy(kantoName, gSaveBlock2Ptr->rivalName, sizeof(kantoName));
    memcpy(gSaveBlock2Ptr->rivalName, kantoName, sizeof(kantoName));
    SetSaveBlocksPointers(0);
    JohtoSave_InitializeCurrent();
    EXPECT(JohtoRival_SetName(sCrystal));
    EXPECT_EQ(memcmp(JohtoRival_GetName(), sCrystal, sizeof(sCrystal)), 0);
    EXPECT_EQ(gSaveblock1.johto.rival_name[sizeof(sCrystal) - 1], EOS);
    EXPECT(JohtoRival_SetName(sShort));
    EXPECT_EQ(gSaveblock1.johto.rival_name[sizeof(sShort)], 0);
    EXPECT(JohtoRival_SetName(sCrystal));
    EXPECT(JohtoRival_SetName(JohtoRival_GetName()));
    EXPECT_EQ(memcmp(JohtoRival_GetName(), sCrystal, sizeof(sCrystal)), 0);
    snapshot = gSaveblock1.johto;
    EXPECT_EQ(JohtoSave_Load(), JOHTO_SAVE_LOAD_READY);
    EXPECT_EQ(memcmp(&snapshot, &gSaveblock1.johto, sizeof(snapshot)), 0);
    EXPECT_EQ(memcmp(gSaveBlock2Ptr->rivalName, kantoName, sizeof(kantoName)), 0);
}

TEST("Johto rival name rejects malformed input atomically")
{
    static const u8 sEmpty[] = {EOS};
    static const u8 sTooLong[] = {CHAR_A, CHAR_B, CHAR_C, CHAR_D, CHAR_E, CHAR_F, CHAR_G, CHAR_H};
    static const u8 sControl[] = {CHAR_A, EXT_CTRL_CODE_BEGIN, EOS};
    static const u8 sPlaceholder[] = {CHAR_A, PLACEHOLDER_BEGIN, EOS};
    struct JohtoSaveV1 snapshot;

    SetSaveBlocksPointers(0);
    JohtoSave_InitializeCurrent();
    EXPECT(JohtoRival_SetName(COMPOUND_STRING("GOLD")));
    snapshot = gSaveblock1.johto;
    EXPECT(!JohtoRival_SetName(NULL));
    EXPECT(!JohtoRival_SetName(sEmpty));
    EXPECT(!JohtoRival_SetName(sTooLong));
    EXPECT(!JohtoRival_SetName(sControl));
    EXPECT(!JohtoRival_SetName(sPlaceholder));
    EXPECT_EQ(memcmp(&snapshot, &gSaveblock1.johto, sizeof(snapshot)), 0);
}

TEST("Johto trainer marker and dialogue resolve by encoded marker and region")
{
    static const u8 ordinary[JOHTO_RIVAL_NAME_SIZE] = {CHAR_S, CHAR_I, CHAR_L, CHAR_V, CHAR_E, CHAR_R, EOS, 0};
    enum Region oldRegion = gMapHeader.engineRegion;
    const u8 *kantoPlaceholder;

    SetSaveBlocksPointers(0);
    JohtoSave_InitializeCurrent();
    EXPECT(JohtoRival_SetName(COMPOUND_STRING("SILVER")));
    EXPECT_EQ(JohtoRival_ResolveTrainerName(gJohtoRivalNameMarker),
              gSaveblock1.johto.rival_name);
    EXPECT_EQ(JohtoRival_ResolveTrainerName(ordinary), ordinary);

    gMapHeader.engineRegion = REGION_KANTO;
    kantoPlaceholder = GetExpandedPlaceholder(PLACEHOLDER_ID_RIVAL);
    EXPECT(JohtoRival_SetName(COMPOUND_STRING("CRYSTAL")));
    EXPECT_EQ(GetExpandedPlaceholder(PLACEHOLDER_ID_RIVAL), kantoPlaceholder);
    EXPECT_EQ(StringCompare(GetTrainerNameFromId(JOHTO_TRAINER_RIVAL_CHIKORITA_1), COMPOUND_STRING("???")), 0);
    EXPECT_EQ(GetTrainerNameFromId(JOHTO_TRAINER_RIVAL_CHIKORITA_2), JohtoRival_GetName());
    EXPECT_EQ(StringCompare(GetTrainerNameFromId(JOHTO_TRAINER_JOEY), COMPOUND_STRING("JOEY")), 0);
    gMapHeader.engineRegion = REGION_JOHTO;
    EXPECT_EQ(GetExpandedPlaceholder(PLACEHOLDER_ID_RIVAL),
              gSaveblock1.johto.rival_name);
    gMapHeader.engineRegion = oldRegion;
}

TEST("Johto rival getter falls back without repairing malformed records")
{
    struct JohtoSaveV1 snapshot;

    SetSaveBlocksPointers(0);
    JohtoSave_InitializeCurrent();
    gSaveblock1.johto.rival_name[0] = EOS;
    snapshot = gSaveblock1.johto;
    EXPECT_EQ(StringCompare(JohtoRival_GetName(), COMPOUND_STRING("SILVER")), 0);
    EXPECT_EQ(memcmp(&snapshot, &gSaveblock1.johto, sizeof(snapshot)), 0);
}
