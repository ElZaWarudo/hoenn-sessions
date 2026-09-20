#include "global.h"
#include "item.h"
#include "johto/bug_contest.h"
#include "johto/save.h"
#include "load_save.h"
#include "pokemon.h"
#include "pokemon_storage_system.h"
#include "save.h"
#include "coop/net_bridge.h"
#include "coop/save.h"
#include "test/test.h"
#include "constants/items.h"
#include "constants/species.h"

typedef u16 (*SaveTestProgramFlashSectorCallback)(u16, u8 *);
extern void Save_TestSetFlashProgramCallback(SaveTestProgramFlashSectorCallback callback);

static EWRAM_DATA struct Pokemon sPartyBeforeSave[PARTY_SIZE];
static EWRAM_DATA struct Pokemon sSavedPartyBeforeSave[PARTY_SIZE];
static EWRAM_DATA u32 sProgramCalls;

static u16 CountProgrammedSectors(u16 sector, u8 *data)
{
    (void)sector;
    (void)data;
    sProgramCalls++;
    return 0;
}

static void ResetContestSaveFixture(void)
{
    JohtoBugContest_TestReset();
    SetSaveBlocksPointers(0);
    SetBagItemsPointers();
    ClearBag();
    ResetPokemonStorageSystem();
    memset(gPlayerParty, 0, sizeof(gPlayerParty));
    gPlayerPartyCount = 0;
    JohtoSave_InitializeCurrent();
    CoopSave_InitializeCurrent();
    CoopNetBridge_Init();
}

static void PrepareContestFixture(void)
{
    u8 i;

    ResetContestSaveFixture();
    for (i = 0; i < PARTY_SIZE; i++)
        CreateMonWithIVs(&gPlayerParty[i], SPECIES_RATTATA, 20, i + 1,
                         OTID_STRUCT_PLAYER_ID, 10);
    CalculatePlayerPartyCount();
    EXPECT(AddBagItem(ITEM_SAFARI_BALL, 7));
}

static void StartContestAndSnapshot(void)
{
    EXPECT_EQ(JohtoBugContest_Begin(100), JOHTO_BUG_CONTEST_OK);
    memcpy(sPartyBeforeSave, gPlayerParty, sizeof(sPartyBeforeSave));
    memcpy(sSavedPartyBeforeSave, gSaveBlock1Ptr->playerParty,
           sizeof(sSavedPartyBeforeSave));
}

static void BeginContestFixture(void)
{
    PrepareContestFixture();
    StartContestAndSnapshot();
}

static void ExpectPartyAndSavePartyUnchanged(void)
{
    EXPECT_EQ(memcmp(sPartyBeforeSave, gPlayerParty, sizeof(sPartyBeforeSave)), 0);
    EXPECT_EQ(memcmp(sSavedPartyBeforeSave, gSaveBlock1Ptr->playerParty,
                     sizeof(sSavedPartyBeforeSave)), 0);
}

static void ExpectMenuSaveCanceled(void)
{
    CoopStartMenu_TestSetSaveDryRun(TRUE);
    CoopStartMenu_TestSetCheckpointRequired(TRUE);
    EXPECT_EQ(CoopStartMenu_TestRunSaveSavingMessageCallback(),
              COOP_START_MENU_TEST_SAVE_IN_PROGRESS);
    EXPECT_EQ(CoopStartMenu_TestRunSaveDoSaveCallback(),
              COOP_START_MENU_TEST_SAVE_IN_PROGRESS);
    EXPECT_EQ(CoopStartMenu_TestRunCheckpointAbortCallback(),
              COOP_START_MENU_TEST_SAVE_CANCELED);

    CoopStartMenu_TestSetCheckpointRequired(FALSE);
    EXPECT_EQ(CoopStartMenu_TestRunSaveSavingMessageCallback(),
              COOP_START_MENU_TEST_SAVE_IN_PROGRESS);
    EXPECT_EQ(CoopStartMenu_TestRunAuthorizedSaveCallback(),
              COOP_START_MENU_TEST_SAVE_IN_PROGRESS);
    EXPECT_EQ(CoopStartMenu_TestRunCheckpointAbortCallback(),
              COOP_START_MENU_TEST_SAVE_CANCELED);
    CoopStartMenu_TestSetSaveDryRun(FALSE);
}

TEST("Bug Contest fence blocks active and ending saves before party copy")
{
    bool32 flashMemoryPresent = gFlashMemoryPresent;

    BeginContestFixture();
    EXPECT(JohtoBugContest_IsSerializationBlocked());
    sProgramCalls = 0;
    Save_TestSetFlashProgramCallback(CountProgrammedSectors);
    gFlashMemoryPresent = TRUE;

    EXPECT_EQ(CoopNetBridge_RequestCheckpoint(), COOP_CHECKPOINT_REQUEST_REJECTED);
    HandleSavingData(SAVE_NORMAL);
    EXPECT_EQ(TrySavingData(SAVE_NORMAL), SAVE_STATUS_ERROR);
    EXPECT(LinkFullSave_Init());
    EXPECT_EQ(sProgramCalls, 0);
    ExpectPartyAndSavePartyUnchanged();

    EXPECT_EQ(JohtoBugContest_RequestEnd(JOHTO_BUG_CONTEST_END_RETIRE),
              JOHTO_BUG_CONTEST_OK);
    EXPECT(JohtoBugContest_IsEnding());
    EXPECT_EQ(CoopNetBridge_RequestCheckpoint(), COOP_CHECKPOINT_REQUEST_REJECTED);
    HandleSavingData(SAVE_LINK);
    EXPECT_EQ(TrySavingData(SAVE_LINK), SAVE_STATUS_ERROR);
    EXPECT(LinkFullSave_Init());
    EXPECT_EQ(sProgramCalls, 0);
    ExpectPartyAndSavePartyUnchanged();

    ExpectMenuSaveCanceled();
    EXPECT(JohtoBugContest_Abort() == JOHTO_BUG_CONTEST_OK);
    EXPECT(!JohtoBugContest_IsSerializationBlocked());
    EXPECT_EQ(CoopNetBridge_RequestCheckpoint(), COOP_CHECKPOINT_REQUEST_OFFLINE);

    Save_TestSetFlashProgramCallback(NULL);
    gFlashMemoryPresent = flashMemoryPresent;
}

TEST("Bug Contest fence permits ordinary dry-run save after abort")
{
    BeginContestFixture();
    ExpectMenuSaveCanceled();
    EXPECT_EQ(JohtoBugContest_Abort(), JOHTO_BUG_CONTEST_OK);
    EXPECT(!JohtoBugContest_IsSerializationBlocked());

    CoopStartMenu_TestSetSaveDryRun(TRUE);
    CoopStartMenu_TestSetCheckpointRequired(FALSE);
    EXPECT_EQ(CoopStartMenu_TestRunSaveSavingMessageCallback(),
              COOP_START_MENU_TEST_SAVE_IN_PROGRESS);
    EXPECT_EQ(CoopStartMenu_TestRunAuthorizedSaveCallback(),
              COOP_START_MENU_TEST_SAVE_SUCCESS);
    CoopStartMenu_TestSetSaveDryRun(FALSE);
}

TEST("Contest cancellation unwinds each initialized full-save continuation immediately")
{
    u32 i;
    bool32 flashMemoryPresent = gFlashMemoryPresent;
    for (i = 0; i < 3; i++)
    {
        PrepareContestFixture();
        Save_ResetSaveCounters();
        gSaveCounter = 71;
        gLastWrittenSector = 3;
        gFlashMemoryPresent = TRUE;
        sProgramCalls = 0;
        Save_TestSetFlashProgramCallback(CountProgrammedSectors);
        EXPECT_EQ(gSaveBlock3Ptr->coop.save_generation, 0);
        EXPECT(!LinkFullSave_Init());
        EXPECT_EQ(gSaveCounter, 72);
        EXPECT_EQ(gSaveBlock3Ptr->coop.save_generation, 1);
        StartContestAndSnapshot();
        if (i == 1)
            EXPECT_EQ(JohtoBugContest_RequestEnd(JOHTO_BUG_CONTEST_END_RETIRE), JOHTO_BUG_CONTEST_OK);
        if (i == 0)
            EXPECT(LinkFullSave_WriteSector());
        else if (i == 1)
            EXPECT(!LinkFullSave_ReplaceLastSector());
        else
            EXPECT(!LinkFullSave_SetLastSectorSignature());
        EXPECT_EQ(sProgramCalls, 0);
        EXPECT_EQ(gSaveCounter, 71);
        EXPECT_EQ(gLastWrittenSector, 3);
        EXPECT_EQ(gSaveBlock3Ptr->coop.save_generation, 0);
        ExpectPartyAndSavePartyUnchanged();
        EXPECT_EQ(JohtoBugContest_Abort(), JOHTO_BUG_CONTEST_OK);
        EXPECT(JohtoSave_SetFlag(3, TRUE));
        EXPECT(!LinkFullSave_Init());
        EXPECT(JohtoSave_GetFlag(3));
        StartContestAndSnapshot();
        EXPECT(LinkFullSave_WriteSector());
        EXPECT(JohtoSave_GetFlag(3));
        EXPECT_EQ(JohtoBugContest_Abort(), JOHTO_BUG_CONTEST_OK);
    }
    Save_TestSetFlashProgramCallback(NULL);
    gFlashMemoryPresent = flashMemoryPresent;
    Save_ResetSaveCounters();
}

TEST("Contest guards preserve saved count counters and addresses on every blocked entry")
{
    u32 phase;
    u32 counter;
    u16 sector, incremental;
    u8 savedCount;
    struct SaveSectorLocation addresses[NUM_SECTORS_PER_SLOT];
    bool32 flashMemoryPresent = gFlashMemoryPresent;
    BeginContestFixture();
    gFlashMemoryPresent = TRUE;
    sProgramCalls = 0;
    Save_TestSetFlashProgramCallback(CountProgrammedSectors);
    counter = gSaveCounter;
    sector = gLastWrittenSector;
    incremental = gIncrementalSectorId;
    savedCount = gSaveBlock1Ptr->playerPartyCount;
    memcpy(addresses, gRamSaveSectorLocations, sizeof(addresses));
    for (phase = 0; phase < 2; phase++)
    {
        if (phase)
            EXPECT_EQ(JohtoBugContest_RequestEnd(JOHTO_BUG_CONTEST_END_RETIRE), JOHTO_BUG_CONTEST_OK);
        HandleSavingData(SAVE_NORMAL);
        EXPECT_EQ(TrySavingData(SAVE_NORMAL), SAVE_STATUS_ERROR);
        EXPECT(LinkFullSave_Init());
        EXPECT(LinkFullSave_WriteSector());
        EXPECT(!LinkFullSave_ReplaceLastSector());
        EXPECT(!LinkFullSave_SetLastSectorSignature());
        EXPECT(WriteSaveBlock2());
        EXPECT(WriteSaveBlock1Sector());
        EXPECT_EQ(gSaveCounter, counter);
        EXPECT_EQ(gLastWrittenSector, sector);
        EXPECT_EQ(gIncrementalSectorId, incremental);
        EXPECT_EQ(gSaveBlock1Ptr->playerPartyCount, savedCount);
        EXPECT_EQ(memcmp(addresses, gRamSaveSectorLocations, sizeof(addresses)), 0);
        EXPECT_EQ(sProgramCalls, 0);
        ExpectPartyAndSavePartyUnchanged();
    }
    EXPECT_EQ(JohtoBugContest_Abort(), JOHTO_BUG_CONTEST_OK);
    Save_TestSetFlashProgramCallback(NULL);
    gFlashMemoryPresent = flashMemoryPresent;
}

TEST("Contest menu cancellation releases a granted cloud checkpoint for a new save")
{
    struct CoopBridgeMessage message;
    PrepareContestFixture();
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_SESSION_READY, 1, 17, NULL, 0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(CoopNetBridge_RequestCheckpoint(), COOP_CHECKPOINT_REQUEST_STARTED);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_CHECKPOINT_GRANTED, 2, 17, NULL, 0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_GRANTED);
    StartContestAndSnapshot();
    CoopStartMenu_TestSetSaveDryRun(TRUE);
    EXPECT_EQ(CoopStartMenu_TestRunCheckpointWaitCallback(), COOP_START_MENU_TEST_SAVE_IN_PROGRESS);
    EXPECT_EQ(CoopStartMenu_TestRunCheckpointAbortCallback(), COOP_START_MENU_TEST_SAVE_CANCELED);
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
    EXPECT_EQ(JohtoBugContest_Abort(), JOHTO_BUG_CONTEST_OK);
    EXPECT_EQ(CoopNetBridge_RequestCheckpoint(), COOP_CHECKPOINT_REQUEST_STARTED);
    CoopNetBridge_NotifySaveResult(FALSE);
    CoopStartMenu_TestSetSaveDryRun(FALSE);
}
