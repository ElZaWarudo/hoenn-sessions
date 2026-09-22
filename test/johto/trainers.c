#include "global.h"
#include "battle.h"
#include "battle_main.h"
#include "battle_setup.h"
#include "coop/identity.h"
#include "coop/net_bridge.h"
#include "coop/save.h"
#include "data.h"
#include "event_data.h"
#include "malloc.h"
#include "load_save.h"
#include "johto/save.h"
#include "johto/trainers.h"
#include "pokemon.h"
#include "test/test.h"
#include "constants/battle_ai.h"
#include "constants/battle_frontier_trainers.h"
#include "constants/battle_partner.h"
#include "constants/flags.h"
#include "constants/johto_trainers.h"
#include "constants/johto_content.h"
#include "constants/moves.h"
#include "constants/region_map_sections.h"
#include "constants/species.h"
#include "constants/trainers.h"

static void SetActiveRegion(u8 engineRegion, u32 sectionId)
{
    gMapHeader.engineRegion = engineRegion;
    gMapHeader.regionMapSectionId = sectionId;
}

static void PrepareOfflineCoopSave(bool8 ambiguous)
{
    CoopSave_InitializeCurrent();
    if (ambiguous)
    {
        gSaveBlock3Ptr->coop.status_flags = COOP_SAVE_STATUS_MIGRATION_AMBIGUOUS;
        (void)CoopSave_Seal(&gSaveBlock3Ptr->coop);
    }
    else
    {
        gSaveBlock3Ptr->coop.magic ^= 1;
    }
    (void)CoopSave_Load();
    EXPECT(!CoopSave_IsOnlineEnabled());
}

static void ExpectOnlineJohtoRejectionPreservesStores(void)
{
    struct CoopSaveV1 coopBefore = gSaveBlock3Ptr->coop;
    struct JohtoSaveV1 johtoBefore = gSaveblock1.johto;
    bool8 legacyFlagBefore = FlagGet(TRAINER_FLAGS_START + TRAINER_JOEY);
    bool8 defeated = TRUE;

    EXPECT_EQ(CoopIdentity_GetTrainerDefeated(JOHTO_TRAINER_JOEY, &defeated),
              COOP_IDENTITY_ACCESS_REJECTED);
    EXPECT(defeated);
    EXPECT_EQ(CoopIdentity_SetTrainerDefeated(JOHTO_TRAINER_JOEY, TRUE),
              COOP_IDENTITY_ACCESS_REJECTED);
    EXPECT(memcmp(&gSaveBlock3Ptr->coop, &coopBefore, sizeof(coopBefore)) == 0);
    EXPECT(memcmp(&gSaveblock1.johto, &johtoBefore, sizeof(johtoBefore)) == 0);
    EXPECT_EQ(FlagGet(TRAINER_FLAGS_START + TRAINER_JOEY), legacyFlagBefore);
}

TEST("Johto trainer namespace keeps boundaries and imported records dense")
{
    const struct Trainer *none;
    const u16 lastId = JOHTO_TRAINER_ID_MIN + JOHTO_TRAINER_RECORD_COUNT - 1;
    const u16 afterId = JOHTO_TRAINER_ID_MIN + JOHTO_TRAINER_RECORD_COUNT;

    EXPECT(!JohtoTrainer_IsId(JOHTO_TRAINER_ID_MIN - 1));
    EXPECT(JohtoTrainer_IsId(JOHTO_TRAINER_ID_MIN));
    EXPECT(JohtoTrainer_IsId(JOHTO_TRAINER_ID_MAX));
    EXPECT(!JohtoTrainer_IsId(JOHTO_TRAINER_ID_MAX + 1));
    EXPECT(JohtoTrainer_IsPopulated(JOHTO_TRAINER_JOEY));
    EXPECT(JohtoTrainer_IsPopulated(lastId));
    EXPECT(!JohtoTrainer_IsPopulated(afterId));
    EXPECT(JohtoTrainer_GetStruct(JOHTO_TRAINER_JOEY) != NULL);
    EXPECT(JohtoTrainer_GetStruct(lastId) != NULL);
    EXPECT(JohtoTrainer_GetStruct(afterId) == NULL);
    EXPECT(JohtoTrainer_GetStruct(JOHTO_TRAINER_ID_MAX) == NULL);

    none = GetTrainerStructFromId(afterId);
    EXPECT_EQ(none, &gTrainers[DIFFICULTY_NORMAL][TRAINER_NONE]);
}

TEST("Johto Joey preserves donor party and host trainer equivalents")
{
    const struct Trainer *trainer = GetTrainerStructFromId(JOHTO_TRAINER_JOEY);
    const struct TrainerMon *party;
    u32 i;

    EXPECT(trainer != NULL);
    EXPECT_EQ(trainer->trainerClass, TRAINER_CLASS_YOUNGSTER);
    EXPECT_EQ(trainer->trainerPic, TRAINER_PIC_YOUNGSTER);
    EXPECT_EQ((u16)trainer->encounterMusic, TRAINER_ENCOUNTER_MUSIC_MALE);
    EXPECT_EQ(trainer->aiFlags, AI_FLAG_CHECK_BAD_MOVE);
    EXPECT_EQ((u16)trainer->partySize, 1);
    party = trainer->party;
    EXPECT_EQ(party[0].species, SPECIES_RATTATA);
    EXPECT_EQ(party[0].lvl, 4);
    EXPECT_EQ(party[0].iv, 0);
    EXPECT_EQ(party[0].heldItem, ITEM_NONE);
    for (i = 0; i < MAX_MON_MOVES; i++)
        EXPECT_EQ(party[0].moves[i], MOVE_NONE);
}

TEST("Johto Joey factory creates the configured default move party")
{
    const struct Trainer *trainer = GetTrainerStructFromId(JOHTO_TRAINER_JOEY);
    struct Pokemon *party = Alloc(6 * sizeof(*party));

    EXPECT(trainer != NULL);
    EXPECT_EQ((u16)trainer->partySize, 1);
    EXPECT_EQ(CreateNPCTrainerPartyFromTrainer(party, trainer, TRUE, BATTLE_TYPE_TRAINER), 1);
    EXPECT_EQ(GetMonData(&party[0], MON_DATA_SPECIES), SPECIES_RATTATA);
    EXPECT_EQ(GetMonData(&party[0], MON_DATA_LEVEL), 4);
    EXPECT_EQ(GetMonData(&party[0], MON_DATA_HELD_ITEM), ITEM_NONE);
    EXPECT_EQ(GetMonData(&party[0], MON_DATA_HP_IV), 0);
    EXPECT_EQ(GetMonData(&party[0], MON_DATA_ATK_IV), 0);
    EXPECT_EQ(GetMonData(&party[0], MON_DATA_DEF_IV), 0);
    EXPECT_EQ(GetMonData(&party[0], MON_DATA_SPEED_IV), 0);
    EXPECT_EQ(GetMonData(&party[0], MON_DATA_SPATK_IV), 0);
    EXPECT_EQ(GetMonData(&party[0], MON_DATA_SPDEF_IV), 0);
    EXPECT_EQ(GetMonData(&party[0], MON_DATA_MOVE1), MOVE_TACKLE);
    EXPECT_EQ(GetMonData(&party[0], MON_DATA_MOVE2), MOVE_TAIL_WHIP);
    EXPECT_EQ(GetMonData(&party[0], MON_DATA_MOVE3), MOVE_QUICK_ATTACK);
    EXPECT_EQ(GetMonData(&party[0], MON_DATA_MOVE4), MOVE_NONE);
    EXPECT_EQ(GetMonData(&party[0], MON_DATA_PP1), 35);
    EXPECT_EQ(GetMonData(&party[0], MON_DATA_PP2), 30);
    EXPECT_EQ(GetMonData(&party[0], MON_DATA_PP3), 30);
    EXPECT_EQ(GetMonData(&party[0], MON_DATA_PP4), 0);
    Free(party);
}

TEST("Johto imported roster preserves boss, double, custom move, and IV fields")
{
    const struct Trainer *falkner = GetTrainerStructFromId(JOHTO_TRAINER_FALKNER_1);
    const struct Trainer *doubleTrainer = GetTrainerStructFromId(JOHTO_TRAINER_AMY_AND_MAY);
    const struct TrainerMon *party;

    EXPECT(falkner != NULL);
    EXPECT_EQ(falkner->trainerClass, TRAINER_CLASS_LEADER);
    EXPECT_EQ(falkner->trainerPic, TRAINER_PIC_LEADER_FALKNER);
    EXPECT_EQ((u16)falkner->battleType, TRAINER_BATTLE_TYPE_SINGLES);
    EXPECT_EQ(falkner->aiFlags, AI_FLAG_CHECK_BAD_MOVE | AI_FLAG_TRY_TO_FAINT | AI_FLAG_CHECK_VIABILITY);
    EXPECT_EQ((u16)falkner->partySize, 2);
    party = falkner->party;
    EXPECT_EQ(party[0].iv, TRAINER_PARTY_IVS(12, 12, 12, 12, 12, 12));
    EXPECT_EQ(party[0].species, SPECIES_PIDGEY);
    EXPECT_EQ(party[0].moves[0], MOVE_TACKLE);
    EXPECT_EQ(party[0].heldItem, ITEM_NONE);
    EXPECT_EQ(party[1].heldItem, ITEM_SITRUS_BERRY);

    EXPECT(doubleTrainer != NULL);
    EXPECT_EQ((u16)doubleTrainer->battleType, TRAINER_BATTLE_TYPE_DOUBLES);
    EXPECT_EQ(doubleTrainer->trainerClass, TRAINER_CLASS_TWINS);
    EXPECT_EQ((u16)doubleTrainer->partySize, 2);
}

TEST("Johto difficulty falls back without changing partner lookup")
{
    enum DifficultyLevel previous = GetCurrentDifficultyLevel();
    const struct Trainer *partner;

    SetCurrentDifficultyLevel(DIFFICULTY_HARD);
    EXPECT_EQ(GetTrainerDifficultyLevel(JOHTO_TRAINER_JOEY), DIFFICULTY_NORMAL);
    EXPECT_EQ(GetTrainerDifficultyLevel(JOHTO_TRAINER_ID_MIN + JOHTO_TRAINER_RECORD_COUNT), DIFFICULTY_NORMAL);
    SetCurrentDifficultyLevel(DIFFICULTY_NORMAL);

    partner = GetTrainerStructFromId(TRAINER_PARTNER(PARTNER_STEVEN));
    EXPECT_EQ(partner, &gBattlePartners[DIFFICULTY_NORMAL][PARTNER_STEVEN]);
    EXPECT(IsSpecialTrainer(TRAINER_LINK_OPPONENT));
    SetCurrentDifficultyLevel(previous);
}

TEST("Johto legacy identity keeps ordinary and Frontier overlaps on legacy flags")
{
    bool8 defeated = FALSE;

    PrepareOfflineCoopSave(TRUE);
    EXPECT_EQ(TRAINER_JOEY, 322);
    EXPECT_EQ(CoopIdentity_GetTrainerDefeated(TRAINER_JOEY, &defeated),
              COOP_IDENTITY_ACCESS_LEGACY);
    EXPECT_EQ(CoopIdentity_GetTrainerDefeated(FRONTIER_TRAINERS_COUNT - 1, &defeated),
              COOP_IDENTITY_ACCESS_LEGACY);
}

TEST("Johto offline trainer defeat uses the Johto save bit")
{
    struct CoopSaveV1 coopBefore;
    bool8 defeated = TRUE;

    PrepareOfflineCoopSave(TRUE);
    JohtoSave_InitializeCurrent();
    coopBefore = gSaveBlock3Ptr->coop;
    EXPECT_EQ(CoopIdentity_GetTrainerDefeated(JOHTO_TRAINER_JOEY, &defeated),
              COOP_IDENTITY_ACCESS_HANDLED);
    EXPECT(!defeated);
    EXPECT_EQ(CoopIdentity_SetTrainerDefeated(JOHTO_TRAINER_JOEY, TRUE),
              COOP_IDENTITY_ACCESS_HANDLED);
    EXPECT(JohtoSave_GetTrainerDefeated(JOHTO_TRAINER_ORDINAL_JOEY));
    EXPECT(memcmp(&gSaveBlock3Ptr->coop, &coopBefore, sizeof(coopBefore)) == 0);
    EXPECT_EQ(CoopIdentity_GetTrainerDefeated(JOHTO_TRAINER_JOEY, &defeated),
              COOP_IDENTITY_ACCESS_HANDLED);
    EXPECT(defeated);
    ClearTrainerFlag(JOHTO_TRAINER_JOEY);
    EXPECT(!JohtoSave_GetTrainerDefeated(JOHTO_TRAINER_ORDINAL_JOEY));
    EXPECT(memcmp(&gSaveBlock3Ptr->coop, &coopBefore, sizeof(coopBefore)) == 0);
    ToggleTrainerFlag(JOHTO_TRAINER_JOEY);
    EXPECT(JohtoSave_GetTrainerDefeated(JOHTO_TRAINER_ORDINAL_JOEY));
    EXPECT(memcmp(&gSaveBlock3Ptr->coop, &coopBefore, sizeof(coopBefore)) == 0);
    EXPECT_EQ(CoopIdentity_GetTrainerDefeated(JOHTO_TRAINER_ID_MIN + JOHTO_TRAINER_RECORD_COUNT, &defeated),
              COOP_IDENTITY_ACCESS_REJECTED);

    PrepareOfflineCoopSave(FALSE);
    JohtoSave_InitializeCurrent();
    coopBefore = gSaveBlock3Ptr->coop;
    defeated = TRUE;
    EXPECT_EQ(CoopIdentity_GetTrainerDefeated(JOHTO_TRAINER_JOEY, &defeated),
              COOP_IDENTITY_ACCESS_HANDLED);
    EXPECT(!defeated);
    SetTrainerFlag(JOHTO_TRAINER_JOEY);
    EXPECT(JohtoSave_GetTrainerDefeated(JOHTO_TRAINER_ORDINAL_JOEY));
    ClearTrainerFlag(JOHTO_TRAINER_JOEY);
    EXPECT(!JohtoSave_GetTrainerDefeated(JOHTO_TRAINER_ORDINAL_JOEY));
    ToggleTrainerFlag(JOHTO_TRAINER_JOEY);
    EXPECT(JohtoSave_GetTrainerDefeated(JOHTO_TRAINER_ORDINAL_JOEY));
    EXPECT(memcmp(&gSaveBlock3Ptr->coop, &coopBefore, sizeof(coopBefore)) == 0);
}

TEST("Johto offline identity rejects partner and reserved IDs without flags")
{
    const u16 partnerId = TRAINER_PARTNER(PARTNER_STEVEN);
    const u16 gapId = TRAINERS_COUNT;
    const bool8 partnerFlag = FlagGet(TRAINER_FLAGS_START + partnerId);
    const bool8 gapFlag = FlagGet(TRAINER_FLAGS_START + gapId);
    u8 flagsBefore[sizeof(gSaveBlock1Ptr->flags)];
    bool8 defeated = TRUE;

    PrepareOfflineCoopSave(TRUE);
    JohtoSave_InitializeCurrent();
    memcpy(flagsBefore, gSaveBlock1Ptr->flags, sizeof(flagsBefore));
    EXPECT_EQ(CoopIdentity_GetTrainerDefeated(partnerId, &defeated),
              COOP_IDENTITY_ACCESS_REJECTED);
    EXPECT(defeated);
    EXPECT_EQ(CoopIdentity_SetTrainerDefeated(partnerId, TRUE),
              COOP_IDENTITY_ACCESS_REJECTED);
    EXPECT_EQ(CoopIdentity_SetTrainerDefeated(partnerId, FALSE),
              COOP_IDENTITY_ACCESS_REJECTED);
    EXPECT_EQ(CoopIdentity_GetTrainerDefeated(gapId, &defeated),
              COOP_IDENTITY_ACCESS_REJECTED);
    EXPECT(defeated);
    EXPECT_EQ(CoopIdentity_SetTrainerDefeated(gapId, TRUE),
              COOP_IDENTITY_ACCESS_REJECTED);
    EXPECT_EQ(CoopIdentity_SetTrainerDefeated(gapId, FALSE),
              COOP_IDENTITY_ACCESS_REJECTED);

    SetTrainerFlag(partnerId);
    ClearTrainerFlag(partnerId);
    ToggleTrainerFlag(partnerId);
    SetTrainerFlag(gapId);
    ClearTrainerFlag(gapId);
    ToggleTrainerFlag(gapId);
    EXPECT_EQ(FlagGet(TRAINER_FLAGS_START + partnerId), partnerFlag);
    EXPECT_EQ(FlagGet(TRAINER_FLAGS_START + gapId), gapFlag);
    EXPECT_EQ(memcmp(flagsBefore, gSaveBlock1Ptr->flags, sizeof(flagsBefore)), 0);
}

TEST("Johto online trainer defeat rejects missing qualified registry identity")
{
    CoopSave_InitializeCurrent();
    JohtoSave_InitializeCurrent();
    CoopNetBridge_Init();
    EXPECT(CoopSave_IsOnlineEnabled());
    SetActiveRegion(COOP_MAP_ENGINE_REGION_JOHTO, MAPSEC_NEW_BARK_TOWN);
    EXPECT(!CoopIdentity_ResolveTrainerOrdinal(COOP_REGION_JOHTO,
                                               JOHTO_TRAINER_JOEY, NULL));
    ExpectOnlineJohtoRejectionPreservesStores();

    SetActiveRegion(COOP_MAP_ENGINE_REGION_HOENN, MAPSEC_LITTLEROOT_TOWN);
    ExpectOnlineJohtoRejectionPreservesStores();

    SetActiveRegion(0xFF, MAPSEC_NEW_BARK_TOWN);
    ExpectOnlineJohtoRejectionPreservesStores();
}
