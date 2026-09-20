#include "global.h"
#include "item.h"
#include "load_save.h"
#include "mail.h"
#include "pokemon.h"
#include "pokemon_storage_system.h"
#include "johto/bug_contest.h"
#include "test/test.h"
#include "constants/items.h"
#include "constants/species.h"

static EWRAM_DATA struct Pokemon sOriginalParty[PARTY_SIZE];
static EWRAM_DATA struct Mail sOriginalMail[MAIL_COUNT];

static void ResetSettlementFixture(void)
{
    JohtoBugContest_TestReset();
    SetSaveBlocksPointers(0);
    SetBagItemsPointers();
    ClearBag();
    ResetPokemonStorageSystem();
    ClearAllMail();
    memset(gPlayerParty, 0, sizeof(gPlayerParty));
    gPlayerPartyCount = 0;
}

static void MakeContestParty(void)
{
    u32 i;

    ResetSettlementFixture();
    for (i = 0; i < PARTY_SIZE; i++)
        CreateMonWithIVs(&gPlayerParty[i], SPECIES_RATTATA, 20, i + 1, OTID_STRUCT_PLAYER_ID, 10);
    CalculatePlayerPartyCount();
    GiveMailToMonByItemId(&gPlayerParty[3], ITEM_RETRO_MAIL);
    memcpy(sOriginalParty, gPlayerParty, sizeof(sOriginalParty));
    memcpy(sOriginalMail, gSaveBlock1Ptr->mail, sizeof(sOriginalMail));
}

static void AddContestCatch(void)
{
    CreateMonWithIVs(&gPlayerParty[1], SPECIES_SCYTHER, 20, 77, OTID_STRUCT_PLAYER_ID, 10);
    SetMonData(&gPlayerParty[1], MON_DATA_MAX_HP, &(u16){60});
    CalculatePlayerPartyCount();
}

static void ExpectOriginalParty(void)
{
    EXPECT_EQ(gPlayerPartyCount, PARTY_SIZE);
    EXPECT_EQ(memcmp(sOriginalParty, gPlayerParty, sizeof(sOriginalParty)), 0);
    EXPECT_EQ(memcmp(sOriginalMail, gSaveBlock1Ptr->mail, sizeof(sOriginalMail)), 0);
}

static void FillContestPC(void)
{
    u32 box, slot;

    for (box = 0; box < TOTAL_BOXES_COUNT; box++)
        for (slot = 0; slot < IN_BOX_COUNT; slot++)
            memcpy(GetBoxedMonPtr(box, slot), &gPlayerParty[0].box, sizeof(struct BoxPokemon));
}

static void FillContestPocket(enum Pocket pocketId, enum Item filler)
{
    u32 i;
    struct BagPocket *pocket = &gBagPockets[pocketId];

    for (i = 0; i < pocket->capacity; i++)
        BagPocket_SetSlotItemIdAndCount(pocket, i, filler, MAX_BAG_ITEM_CAPACITY);
}

TEST("Settlement rejects active and unjudged contests without mutation")
{
    struct Pokemon party[PARTY_SIZE];
    struct Mail mail[MAIL_COUNT];
    u16 balls;

    MakeContestParty();
    EXPECT_EQ(JohtoBugContest_Begin(100), JOHTO_BUG_CONTEST_OK);
    memcpy(party, gPlayerParty, sizeof(party));
    memcpy(mail, gSaveBlock1Ptr->mail, sizeof(mail));
    balls = CountTotalItemQuantityInBag(ITEM_SAFARI_BALL);
    EXPECT_EQ(JohtoBugContest_PrepareSettlement(), JOHTO_BUG_CONTEST_NOT_ENDING);
    EXPECT_EQ(memcmp(party, gPlayerParty, sizeof(party)), 0);
    EXPECT_EQ(memcmp(mail, gSaveBlock1Ptr->mail, sizeof(mail)), 0);
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_SAFARI_BALL), balls);
    EXPECT(JohtoBugContest_IsSerializationBlocked());

    EXPECT_EQ(JohtoBugContest_RequestEnd(JOHTO_BUG_CONTEST_END_RETIRE), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(JohtoBugContest_PrepareSettlement(), JOHTO_BUG_CONTEST_NOT_JUDGED);
    EXPECT_EQ(memcmp(party, gPlayerParty, sizeof(party)), 0);
    EXPECT_EQ(memcmp(mail, gSaveBlock1Ptr->mail, sizeof(mail)), 0);
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_SAFARI_BALL), balls);
    EXPECT_EQ(JohtoBugContest_Abort(), JOHTO_BUG_CONTEST_OK);
    EXPECT(!JohtoBugContest_IsSerializationBlocked());
    ExpectOriginalParty();
}

TEST("Settlement restores once and preserves post-settlement party PC mail and bag edits")
{
    struct Pokemon extra;
    struct Mail editedMail[MAIL_COUNT];
    u8 selectedName[POKEMON_NAME_LENGTH + 1];
    u8 selectedNickname[POKEMON_NAME_LENGTH + 1];
    u8 boxedNickname[POKEMON_NAME_LENGTH + 1];
    u32 selectedPersonality;
    u16 selectedSpecies, reward, placement;
    u16 originalBalls, editedSafariBalls;

    MakeContestParty();
    originalBalls = 5;
    EXPECT(AddBagItem(ITEM_SAFARI_BALL, originalBalls));
    EXPECT_EQ(JohtoBugContest_Begin(0), JOHTO_BUG_CONTEST_OK);
    AddContestCatch();
    EXPECT_EQ(JohtoBugContest_RequestEnd(JOHTO_BUG_CONTEST_END_FULL_PARTY), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(JohtoBugContest_Judge(1), JOHTO_BUG_CONTEST_OK);
    selectedSpecies = JohtoBugContest_GetSelectedSpecies();
    placement = JohtoBugContest_GetSelectedPlacement();
    reward = JohtoBugContest_GetReward();
    memcpy(selectedName, JohtoBugContest_GetSelectedName(), sizeof(selectedName));
    selectedPersonality = GetMonData(&gPlayerParty[1], MON_DATA_PERSONALITY);
    GetMonData(&gPlayerParty[1], MON_DATA_NICKNAME, selectedNickname);
    EXPECT(JohtoBugContest_IsSerializationBlocked());

    EXPECT_EQ(JohtoBugContest_PrepareSettlement(), JOHTO_BUG_CONTEST_OK);
    EXPECT(JohtoBugContest_IsSerializationBlocked());
    ExpectOriginalParty();
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_SAFARI_BALL), originalBalls);
    EXPECT_EQ(JohtoBugContest_GetSelectedSpecies(), selectedSpecies);
    EXPECT_EQ(JohtoBugContest_GetSelectedPlacement(), placement);
    EXPECT_EQ(JohtoBugContest_GetReward(), reward);
    EXPECT_EQ(memcmp(JohtoBugContest_GetSelectedName(), selectedName, sizeof(selectedName)), 0);

    SetMonData(&gPlayerParty[0], MON_DATA_HP, &(u16){1});
    ZeroMonData(&gPlayerParty[5]);
    gPlayerPartyCount = PARTY_SIZE - 1;
    ClearAllMail();
    memcpy(editedMail, gSaveBlock1Ptr->mail, sizeof(editedMail));
    EXPECT(RemoveBagItem(ITEM_SAFARI_BALL, 2));
    editedSafariBalls = CountTotalItemQuantityInBag(ITEM_SAFARI_BALL);
    EXPECT(AddBagItem(ITEM_POTION, 1));
    CreateMonWithIVs(&extra, SPECIES_RATTATA, 5, 101, OTID_STRUCT_PLAYER_ID, 10);
    EXPECT_EQ(CopyMonToPC(&extra), MON_GIVEN_TO_PC);
    EXPECT_EQ(CountMonsInBox(0), 1);
    EXPECT_EQ(JohtoBugContest_PrepareSettlement(), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(GetMonData(&gPlayerParty[0], MON_DATA_HP), 1);
    EXPECT_EQ(gPlayerPartyCount, PARTY_SIZE - 1);
    EXPECT_EQ(memcmp(editedMail, gSaveBlock1Ptr->mail, sizeof(editedMail)), 0);
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_SAFARI_BALL), editedSafariBalls);
    EXPECT_EQ(CountMonsInBox(0), 1);
    EXPECT_EQ(JohtoBugContest_TransferSelected(), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(GetBoxMonDataAt(0, 1, MON_DATA_SPECIES), selectedSpecies);
    EXPECT_EQ(GetBoxMonDataAt(0, 1, MON_DATA_PERSONALITY), selectedPersonality);
    GetAndCopyBoxMonDataAt(0, 1, MON_DATA_NICKNAME, boxedNickname);
    EXPECT_EQ(memcmp(selectedNickname, boxedNickname, sizeof(selectedNickname)), 0);
    EXPECT_EQ(JohtoBugContest_ClaimReward(), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(JohtoBugContest_Exit(), JOHTO_BUG_CONTEST_OK);
    EXPECT(!JohtoBugContest_IsSerializationBlocked());
    EXPECT_EQ(GetMonData(&gPlayerParty[0], MON_DATA_HP), 1);
    EXPECT_EQ(gPlayerPartyCount, PARTY_SIZE - 1);
    EXPECT_EQ(memcmp(editedMail, gSaveBlock1Ptr->mail, sizeof(editedMail)), 0);
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_SAFARI_BALL), editedSafariBalls);
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_POTION), 1);
    EXPECT_EQ(CountMonsInBox(0), 2);
}

TEST("Prepared settlement keeps transfer and reward retries recoverable")
{
    u8 selectedNickname[POKEMON_NAME_LENGTH + 1];
    u8 boxedNickname[POKEMON_NAME_LENGTH + 1];
    u32 selectedPersonality;
    u16 selectedSpecies;
    u16 reward;

    MakeContestParty();
    EXPECT_EQ(JohtoBugContest_Begin(0), JOHTO_BUG_CONTEST_OK);
    AddContestCatch();
    EXPECT_EQ(JohtoBugContest_RequestEnd(JOHTO_BUG_CONTEST_END_FULL_PARTY), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(JohtoBugContest_Judge(1), JOHTO_BUG_CONTEST_OK);
    selectedSpecies = GetMonData(&gPlayerParty[1], MON_DATA_SPECIES);
    selectedPersonality = GetMonData(&gPlayerParty[1], MON_DATA_PERSONALITY);
    GetMonData(&gPlayerParty[1], MON_DATA_NICKNAME, selectedNickname);
    reward = JohtoBugContest_GetReward();
    EXPECT_EQ(JohtoBugContest_PrepareSettlement(), JOHTO_BUG_CONTEST_OK);
    FillContestPC();
    EXPECT_EQ(JohtoBugContest_TransferSelected(), JOHTO_BUG_CONTEST_TRANSFER_FAILED);
    EXPECT_EQ(JohtoBugContest_GetReward(), reward);
    EXPECT(JohtoBugContest_IsSerializationBlocked());
    ZeroBoxMonAt(0, 0);
    EXPECT_EQ(JohtoBugContest_TransferSelected(), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(GetBoxMonDataAt(0, 0, MON_DATA_SPECIES), selectedSpecies);
    EXPECT_EQ(GetBoxMonDataAt(0, 0, MON_DATA_PERSONALITY), selectedPersonality);
    GetAndCopyBoxMonDataAt(0, 0, MON_DATA_NICKNAME, boxedNickname);
    EXPECT_EQ(memcmp(selectedNickname, boxedNickname, sizeof(selectedNickname)), 0);
    EXPECT_EQ(JohtoBugContest_TransferSelected(), JOHTO_BUG_CONTEST_OK);

    FillContestPocket(POCKET_ITEMS, ITEM_POTION);
    EXPECT_EQ(JohtoBugContest_ClaimReward(), JOHTO_BUG_CONTEST_REWARD_FAILED);
    EXPECT_EQ(JohtoBugContest_GetReward(), reward);
    BagPocket_SetSlotItemIdAndCount(&gBagPockets[POCKET_ITEMS], 0, ITEM_NONE, 0);
    EXPECT_EQ(JohtoBugContest_ClaimReward(), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(JohtoBugContest_ClaimReward(), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(CountTotalItemQuantityInBag(reward), 1);
    EXPECT_EQ(JohtoBugContest_Exit(), JOHTO_BUG_CONTEST_OK);
    EXPECT(!JohtoBugContest_IsSerializationBlocked());
}

TEST("Prepared settlement abort preserves legitimate edits and releases the fence")
{
    struct Mail editedMail[MAIL_COUNT];

    MakeContestParty();
    EXPECT_EQ(JohtoBugContest_Begin(0), JOHTO_BUG_CONTEST_OK);
    AddContestCatch();
    EXPECT_EQ(JohtoBugContest_RequestEnd(JOHTO_BUG_CONTEST_END_RETIRE), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(JohtoBugContest_Judge(1), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(JohtoBugContest_PrepareSettlement(), JOHTO_BUG_CONTEST_OK);
    SetMonData(&gPlayerParty[0], MON_DATA_HP, &(u16){2});
    ZeroMonData(&gPlayerParty[5]);
    gPlayerPartyCount = PARTY_SIZE - 1;
    ClearAllMail();
    memcpy(editedMail, gSaveBlock1Ptr->mail, sizeof(editedMail));
    EXPECT(AddBagItem(ITEM_SAFARI_BALL, 2));
    EXPECT(AddBagItem(ITEM_POTION, 1));
    EXPECT_EQ(JohtoBugContest_PrepareSettlement(), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(GetMonData(&gPlayerParty[0], MON_DATA_HP), 2);
    EXPECT_EQ(gPlayerPartyCount, PARTY_SIZE - 1);
    EXPECT_EQ(memcmp(editedMail, gSaveBlock1Ptr->mail, sizeof(editedMail)), 0);
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_SAFARI_BALL), 2);
    EXPECT_EQ(JohtoBugContest_Abort(), JOHTO_BUG_CONTEST_OK);
    EXPECT(!JohtoBugContest_IsSerializationBlocked());
    EXPECT_EQ(GetMonData(&gPlayerParty[0], MON_DATA_HP), 2);
    EXPECT_EQ(gPlayerPartyCount, PARTY_SIZE - 1);
    EXPECT_EQ(memcmp(editedMail, gSaveBlock1Ptr->mail, sizeof(editedMail)), 0);
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_SAFARI_BALL), 2);
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_POTION), 1);
}
