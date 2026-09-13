#include "global.h"
#include "item.h"
#include "load_save.h"
#include "mail.h"
#include "pokemon.h"
#include "pokemon_storage_system.h"
#include "constants/maps.h"
#include "johto/bug_contest.h"
#include "test/test.h"
#include "constants/items.h"
#include "constants/species.h"

static EWRAM_DATA struct Pokemon sOriginalParty[PARTY_SIZE];
static EWRAM_DATA struct Mail sOriginalMail[MAIL_COUNT];

static void ResetContestFixture(void)
{
    JohtoBugContest_TestReset();
    SetSaveBlocksPointers(0);
    SetBagItemsPointers();
    ClearBag();
    ResetPokemonStorageSystem();
    ClearAllMail();
    memset(gPlayerParty, 0, sizeof(gPlayerParty));
    gPlayerPartyCount = 0;
    gSaveBlock1Ptr->location.mapGroup = MAP_GROUP(MAP_LITTLEROOT_TOWN);
    gSaveBlock1Ptr->location.mapNum = MAP_NUM(MAP_LITTLEROOT_TOWN);
}

static void MakeContestParty(void)
{
    u32 i;
    ResetContestFixture();
    for (i = 0; i < PARTY_SIZE; i++)
        CreateMonWithIVs(&gPlayerParty[i], SPECIES_RATTATA, 20, i + 1, OTID_STRUCT_PLAYER_ID, 10);
    CalculatePlayerPartyCount();
    EXPECT(GetMonData(&gPlayerParty[0], MON_DATA_HP) > 0);
    GiveMailToMonByItemId(&gPlayerParty[3], ITEM_RETRO_MAIL);
    memcpy(sOriginalParty, gPlayerParty, sizeof(sOriginalParty));
    memcpy(sOriginalMail, gSaveBlock1Ptr->mail, sizeof(sOriginalMail));
}

static void ExpectOriginalParty(void)
{
    EXPECT_EQ(gPlayerPartyCount, PARTY_SIZE);
    EXPECT_EQ(memcmp(sOriginalParty, gPlayerParty, sizeof(sOriginalParty)), 0);
    EXPECT_EQ(memcmp(sOriginalMail, gSaveBlock1Ptr->mail, sizeof(sOriginalMail)), 0);
}

static void AddContestCatch(void)
{
    CreateMonWithIVs(&gPlayerParty[1], SPECIES_SCYTHER, 20, 77, OTID_STRUCT_PLAYER_ID, 10);
    SetMonData(&gPlayerParty[1], MON_DATA_MAX_HP, &(u16){60});
    CalculatePlayerPartyCount();
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

TEST("Contest admission and abort restore six original mons and mail with a partial ball loan")
{
    MakeContestParty();
    EXPECT(AddBagItem(ITEM_SAFARI_BALL, 7));
    EXPECT_EQ(JohtoBugContest_Begin(100), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(gPlayerPartyCount, 1);
    EXPECT_EQ(memcmp(&gPlayerParty[0], &sOriginalParty[0], sizeof(struct Pokemon)), 0);
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_SAFARI_BALL), 37);
    EXPECT_EQ(JohtoBugContest_Begin(101), JOHTO_BUG_CONTEST_ALREADY_ACTIVE);
    EXPECT(RemoveBagItem(ITEM_SAFARI_BALL, 12));
    SetMonData(&gPlayerParty[0], MON_DATA_HP, &(u16){1});
    AddContestCatch();
    EXPECT_EQ(JohtoBugContest_Abort(), JOHTO_BUG_CONTEST_OK);
    ExpectOriginalParty();
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_SAFARI_BALL), 7);
    EXPECT_EQ(JohtoBugContest_Abort(), JOHTO_BUG_CONTEST_NO_CONTEST);
    ExpectOriginalParty();
}

TEST("Active contest timer expires once at the threshold including unsigned wrap")
{
    static const u32 starts[] = {100, 0xFFFFF000};
    u32 i;
    for (i = 0; i < ARRAY_COUNT(starts); i++)
    {
        MakeContestParty();
        EXPECT_EQ(JohtoBugContest_Begin(starts[i]), JOHTO_BUG_CONTEST_OK);
        EXPECT(!JohtoBugContest_CheckTime(starts[i] + 28799));
        EXPECT(JohtoBugContest_IsActive());
        EXPECT(JohtoBugContest_CheckTime(starts[i] + 28800));
        EXPECT(JohtoBugContest_IsEnding());
        EXPECT(!JohtoBugContest_CheckTime(starts[i] + 28801));
        EXPECT_EQ(JohtoBugContest_RequestEnd(JOHTO_BUG_CONTEST_END_RETIRE), JOHTO_BUG_CONTEST_OK);
        EXPECT_EQ(JohtoBugContest_Abort(), JOHTO_BUG_CONTEST_OK);
        ExpectOriginalParty();
    }
}

TEST("Contest admission rejects eggs fainted mons full storage and full bag atomically")
{
    MakeContestParty();
    SetMonData(&gPlayerParty[0], MON_DATA_IS_EGG, &(bool8){TRUE});
    EXPECT_EQ(JohtoBugContest_Begin(0), JOHTO_BUG_CONTEST_INVALID_PARTY);
    EXPECT_EQ(gPlayerPartyCount, PARTY_SIZE);
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_SAFARI_BALL), 0);
    SetMonData(&gPlayerParty[0], MON_DATA_IS_EGG, &(bool8){FALSE});
    SetMonData(&gPlayerParty[0], MON_DATA_HP, &(u16){0});
    EXPECT_EQ(JohtoBugContest_Begin(0), JOHTO_BUG_CONTEST_INVALID_PARTY);
    EXPECT_EQ(gPlayerPartyCount, PARTY_SIZE);

    MakeContestParty();
    FillContestPC();
    EXPECT_EQ(JohtoBugContest_Begin(0), JOHTO_BUG_CONTEST_PC_FULL);
    ExpectOriginalParty();
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_SAFARI_BALL), 0);
    ResetPokemonStorageSystem();
    FillContestPocket(POCKET_POKE_BALLS, ITEM_POKE_BALL);
    EXPECT_EQ(JohtoBugContest_Begin(0), JOHTO_BUG_CONTEST_BAG_FULL);
    ExpectOriginalParty();
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_SAFARI_BALL), 0);
}

TEST("Contest judgement freezes placement and selection and grants each catch and reward once")
{
    u16 reward;
    MakeContestParty();
    EXPECT_EQ(JohtoBugContest_Begin(0), JOHTO_BUG_CONTEST_OK);
    AddContestCatch();
    EXPECT_EQ(JohtoBugContest_Judge(1), JOHTO_BUG_CONTEST_NOT_ENDING);
    EXPECT_EQ(JohtoBugContest_RequestEnd(JOHTO_BUG_CONTEST_END_RETIRE), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(JohtoBugContest_Judge(0), JOHTO_BUG_CONTEST_INVALID_SELECTION);
    EXPECT_EQ(JohtoBugContest_Judge(0x0101), JOHTO_BUG_CONTEST_INVALID_SELECTION);
    EXPECT_EQ(JohtoBugContest_Judge(0xFFFF), JOHTO_BUG_CONTEST_INVALID_SELECTION);
    EXPECT_EQ(JohtoBugContest_Judge(1), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(JohtoBugContest_GetSelectedPlacement(), 1);
    reward = JohtoBugContest_GetReward();
    EXPECT_EQ(JohtoBugContest_Judge(1), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(JohtoBugContest_GetReward(), reward);
    EXPECT_EQ(JohtoBugContest_GetSelectedPlacement(), 1);
    EXPECT_EQ(JohtoBugContest_Judge(2), JOHTO_BUG_CONTEST_SELECTION_LOCKED);
    EXPECT_EQ(JohtoBugContest_Exit(), JOHTO_BUG_CONTEST_EXIT_BLOCKED);
    EXPECT_EQ(JohtoBugContest_TransferSelected(), JOHTO_BUG_CONTEST_NOT_PREPARED);
    EXPECT_EQ(JohtoBugContest_PrepareSettlement(), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(JohtoBugContest_TransferSelected(), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(JohtoBugContest_TransferSelected(), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(CountMonsInBox(0), 1);
    EXPECT_EQ(GetBoxMonDataAt(0, 0, MON_DATA_SPECIES), SPECIES_SCYTHER);
    EXPECT_EQ(JohtoBugContest_Abort(), JOHTO_BUG_CONTEST_EXIT_BLOCKED);
    EXPECT_EQ(JohtoBugContest_ClaimReward(), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(JohtoBugContest_ClaimReward(), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(CountTotalItemQuantityInBag(reward), 1);
    EXPECT(JohtoBugContest_IsSerializationBlocked());
    EXPECT_EQ(JohtoBugContest_Exit(), JOHTO_BUG_CONTEST_OK);
    ExpectOriginalParty();
    EXPECT_EQ(JohtoBugContest_Exit(), JOHTO_BUG_CONTEST_NO_CONTEST);
    EXPECT_EQ(JohtoBugContest_GetSelectedPlacement(), 0);
}

TEST("Contest full PC and reward bag failures retain recoverable state")
{
    u16 reward;
    MakeContestParty();
    EXPECT_EQ(JohtoBugContest_Begin(0), JOHTO_BUG_CONTEST_OK);
    AddContestCatch();
    EXPECT_EQ(JohtoBugContest_RequestEnd(JOHTO_BUG_CONTEST_END_FULL_PARTY), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(JohtoBugContest_Judge(1), JOHTO_BUG_CONTEST_OK);
    reward = JohtoBugContest_GetReward();
    EXPECT_EQ(JohtoBugContest_PrepareSettlement(), JOHTO_BUG_CONTEST_OK);
    FillContestPC();
    EXPECT_EQ(JohtoBugContest_TransferSelected(), JOHTO_BUG_CONTEST_TRANSFER_FAILED);
    EXPECT_EQ(JohtoBugContest_ClaimReward(), JOHTO_BUG_CONTEST_TRANSFER_FAILED);
    EXPECT_EQ(JohtoBugContest_Exit(), JOHTO_BUG_CONTEST_EXIT_BLOCKED);
    EXPECT_EQ(JohtoBugContest_GetSelectedSpecies(), SPECIES_SCYTHER);
    ZeroBoxMonAt(0, 0);
    EXPECT_EQ(JohtoBugContest_TransferSelected(), JOHTO_BUG_CONTEST_OK);
    FillContestPocket(POCKET_ITEMS, ITEM_POTION);
    EXPECT_EQ(JohtoBugContest_ClaimReward(), JOHTO_BUG_CONTEST_REWARD_FAILED);
    EXPECT_EQ(JohtoBugContest_Exit(), JOHTO_BUG_CONTEST_EXIT_BLOCKED);
    EXPECT_EQ(JohtoBugContest_GetReward(), reward);
    BagPocket_SetSlotItemIdAndCount(&gBagPockets[POCKET_ITEMS], 0, ITEM_NONE, 0);
    EXPECT_EQ(JohtoBugContest_ClaimReward(), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(CountTotalItemQuantityInBag(reward), 1);
    EXPECT_EQ(JohtoBugContest_Exit(), JOHTO_BUG_CONTEST_OK);
    ExpectOriginalParty();
}

TEST("Contest settlement rejects wrong order without mutation")
{
    struct Pokemon party[PARTY_SIZE];
    struct Mail mail[MAIL_COUNT];
    u16 balls;

    MakeContestParty();
    EXPECT_EQ(JohtoBugContest_Begin(0), JOHTO_BUG_CONTEST_OK);
    AddContestCatch();
    memcpy(party, gPlayerParty, sizeof(party));
    memcpy(mail, gSaveBlock1Ptr->mail, sizeof(mail));
    balls = CountTotalItemQuantityInBag(ITEM_SAFARI_BALL);
    EXPECT_EQ(JohtoBugContest_TransferSelected(), JOHTO_BUG_CONTEST_NOT_ENDING);
    EXPECT_EQ(JohtoBugContest_RequestEnd(JOHTO_BUG_CONTEST_END_RETIRE), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(JohtoBugContest_TransferSelected(), JOHTO_BUG_CONTEST_NOT_JUDGED);
    EXPECT_EQ(JohtoBugContest_Judge(1), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(JohtoBugContest_TransferSelected(), JOHTO_BUG_CONTEST_NOT_PREPARED);
    EXPECT_EQ(memcmp(party, gPlayerParty, sizeof(party)), 0);
    EXPECT_EQ(memcmp(mail, gSaveBlock1Ptr->mail, sizeof(mail)), 0);
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_SAFARI_BALL), balls);
    EXPECT_EQ(CountMonsInBox(0), 0);
    EXPECT(JohtoBugContest_IsSerializationBlocked());
    EXPECT_EQ(JohtoBugContest_Abort(), JOHTO_BUG_CONTEST_OK);
}

TEST("Contest reward can be explicitly forfeited without implicit bag mutation")
{
    u16 reward;
    u16 rewardCount;

    MakeContestParty();
    EXPECT_EQ(JohtoBugContest_Begin(0), JOHTO_BUG_CONTEST_OK);
    AddContestCatch();
    EXPECT_EQ(JohtoBugContest_RequestEnd(JOHTO_BUG_CONTEST_END_RETIRE), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(JohtoBugContest_Judge(1), JOHTO_BUG_CONTEST_OK);
    reward = JohtoBugContest_GetReward();
    rewardCount = CountTotalItemQuantityInBag(reward);
    EXPECT_EQ(JohtoBugContest_PrepareSettlement(), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(JohtoBugContest_ForfeitReward(), JOHTO_BUG_CONTEST_TRANSFER_FAILED);
    EXPECT_EQ(JohtoBugContest_TransferSelected(), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(JohtoBugContest_ForfeitReward(), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(JohtoBugContest_ForfeitReward(), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(JohtoBugContest_ClaimReward(), JOHTO_BUG_CONTEST_REWARD_FORFEITED);
    EXPECT_EQ(CountTotalItemQuantityInBag(reward), rewardCount);
    EXPECT(JohtoBugContest_IsSerializationBlocked());
    EXPECT_EQ(JohtoBugContest_Exit(), JOHTO_BUG_CONTEST_OK);
    EXPECT(!JohtoBugContest_IsSerializationBlocked());
}

TEST("Contest transfer is exactly once and preserves held item and checksum")
{
    u32 checksum;

    MakeContestParty();
    EXPECT_EQ(JohtoBugContest_Begin(0), JOHTO_BUG_CONTEST_OK);
    AddContestCatch();
    SetMonData(&gPlayerParty[1], MON_DATA_HELD_ITEM, &(u16){ITEM_ORAN_BERRY});
    checksum = GetMonData(&gPlayerParty[1], MON_DATA_CHECKSUM);
    EXPECT_EQ(JohtoBugContest_RequestEnd(JOHTO_BUG_CONTEST_END_RETIRE), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(JohtoBugContest_Judge(1), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(JohtoBugContest_PrepareSettlement(), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(JohtoBugContest_TransferSelected(), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(JohtoBugContest_TransferSelected(), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(CountMonsInBox(0), 1);
    EXPECT_EQ(GetBoxMonDataAt(0, 0, MON_DATA_HELD_ITEM), ITEM_ORAN_BERRY);
    EXPECT_EQ(GetBoxMonDataAt(0, 0, MON_DATA_CHECKSUM), checksum);
    EXPECT_EQ(JohtoBugContest_ForfeitReward(), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(JohtoBugContest_Exit(), JOHTO_BUG_CONTEST_OK);
    ExpectOriginalParty();
}

TEST("Johto bug contest keeps the eight minute unsigned timer contract")
{
    EXPECT_EQ(JOHTO_BUG_CONTEST_TIME_LIMIT_FRAMES, 28800);
    JohtoBugContest_TestReset();
    EXPECT(!JohtoBugContest_IsActive());
    EXPECT(!JohtoBugContest_IsEnding());
    EXPECT_EQ(JohtoBugContest_CheckTime(0xFFFFFFFF), FALSE);
    EXPECT_EQ(JohtoBugContest_RequestEnd(JOHTO_BUG_CONTEST_END_TIMEOUT), JOHTO_BUG_CONTEST_NO_CONTEST);
}

TEST("Johto bug contest preserves source judging thresholds and reward tables")
{
    EXPECT_EQ(JohtoBugContest_GetPlacement(40, 99), 3);
    EXPECT_EQ(JohtoBugContest_GetPlacement(41, 0), 2);
    EXPECT_EQ(JohtoBugContest_GetPlacement(46, 49), 2);
    EXPECT_EQ(JohtoBugContest_GetPlacement(46, 50), 3);
    EXPECT_EQ(JohtoBugContest_GetPlacement(47, 74), 1);
    EXPECT_EQ(JohtoBugContest_GetPlacement(47, 75), 2);
    EXPECT_EQ(JohtoBugContest_GetPlacement(0xFFFF, 99), 1);

    EXPECT_EQ(JohtoBugContest_GetRewardForPlacement(1, 0), ITEM_MOON_STONE);
    EXPECT_EQ(JohtoBugContest_GetRewardForPlacement(1, 2), ITEM_LEAF_STONE);
    EXPECT_EQ(JohtoBugContest_GetRewardForPlacement(2, 1), ITEM_THUNDER_STONE);
    EXPECT_EQ(JohtoBugContest_GetRewardForPlacement(3, 6), ITEM_CHESTO_BERRY);
    EXPECT_EQ(JohtoBugContest_GetRewardForPlacement(0, 0), ITEM_NONE);
}

TEST("Johto bug contest validates all ten source species and selection metadata")
{
    static const u16 species[] =
    {
        SPECIES_CATERPIE, SPECIES_WEEDLE, SPECIES_METAPOD, SPECIES_KAKUNA,
        SPECIES_PARAS, SPECIES_VENONAT, SPECIES_BUTTERFREE, SPECIES_BEEDRILL,
        SPECIES_SCYTHER, SPECIES_PINSIR,
    };
    u8 i;
    for (i = 0; i < ARRAY_COUNT(species); i++)
        EXPECT(JohtoBugContest_IsContestSpecies(species[i]));
    EXPECT(!JohtoBugContest_IsContestSpecies(SPECIES_RATTATA));
    EXPECT_EQ(JohtoBugContest_GetSelectedDisplayIndex(), 0);
    EXPECT_EQ(JohtoBugContest_GetSelectedSpecies(), SPECIES_NONE);
    EXPECT_EQ(JohtoBugContest_GetReward(), ITEM_NONE);
}

TEST("Empty admission fails before touching the temporary contest state")
{
    struct Pokemon party[PARTY_SIZE];
    u8 partyCount;
    memcpy(party, gPlayerParty, sizeof(party));
    partyCount = gPlayerPartyCount;
    JohtoBugContest_TestReset();
    ZeroMonData(&gPlayerParty[0]);
    gPlayerPartyCount = 0;
    EXPECT_EQ(JohtoBugContest_Begin(1234), JOHTO_BUG_CONTEST_INVALID_PARTY);
    EXPECT(!JohtoBugContest_IsActive());
    EXPECT(!JohtoBugContest_IsEnding());
    memcpy(gPlayerParty, party, sizeof(party));
    gPlayerPartyCount = partyCount;
}
