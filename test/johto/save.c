#include "global.h"
#include "agb_flash.h"
#include "coop/net_bridge.h"
#include "coop/save.h"
#include "gba/flash_internal.h"
#include "johto/save.h"
#include "load_save.h"
#include "malloc.h"
#include "config_changes.h"
#include "save.h"
#include "test/test.h"

extern struct ConfigChanges *gConfigChangesTestOverride;

typedef void (*SaveTestReadFlashCallback)(u16, u32, u8 *, u32);
typedef u16 (*SaveTestProgramFlashSectorCallback)(u16, u8 *);

extern void Save_TestSetFlashReadCallback(SaveTestReadFlashCallback callback);
extern void Save_TestSetFlashProgramCallback(SaveTestProgramFlashSectorCallback callback);

struct TestFlashSectorMeta
{
    bool8 valid;
    u16 id;
    u16 checksum;
    u32 signature;
    u32 counter;
    u8 saveBlock3Chunk[SAVE_BLOCK_3_CHUNK_SIZE];
};

static EWRAM_DATA u8 sFlashSaveBlock1Tail[NUM_SAVE_SLOTS][JOHTO_SAVE_SERIALIZED_TAIL_SIZE];
static EWRAM_DATA struct TestFlashSectorMeta sFlashMeta[SECTORS_COUNT];
static EWRAM_DATA struct SaveSector sPartialWriteBuffer;
static u16 sPartialSector;
static u32 sFullWriteCount;
static u16 sFirstFullWriteSector[NUM_SAVE_SLOTS];
static u32 sPartialByteWrites;
static u32 sPartialSectorCommits;
static u32 sEraseCalls;
static u32 sProgramSectorCalls;
static u32 sProgramByteCalls;

static void TestFlashReset(void)
{
    u16 sector;

    memset(sFlashSaveBlock1Tail, 0, sizeof(sFlashSaveBlock1Tail));
    memset(sFlashMeta, 0, sizeof(sFlashMeta));
    memset(&sPartialWriteBuffer, 0xFF, sizeof(sPartialWriteBuffer));
    sPartialSector = 0xFFFF;
    sFullWriteCount = 0;
    sFirstFullWriteSector[0] = 0xFFFF;
    sFirstFullWriteSector[1] = 0xFFFF;
    sPartialByteWrites = 0;
    sPartialSectorCommits = 0;
    sEraseCalls = 0;
    sProgramSectorCalls = 0;
    sProgramByteCalls = 0;

    /* An old save has a valid footer and zero data.  In particular, the old
     * sector-5 tail and its newly appended zero record have the same
     * production checksum of zero. */
    for (sector = 0; sector < NUM_SAVE_SLOTS * NUM_SECTORS_PER_SLOT; sector++)
    {
        sFlashMeta[sector].valid = TRUE;
        sFlashMeta[sector].id = sector % NUM_SECTORS_PER_SLOT;
        sFlashMeta[sector].checksum = 0;
        sFlashMeta[sector].signature = SECTOR_SIGNATURE;
        sFlashMeta[sector].counter = 0;
    }
}

static void TestFlashRead(u16 sectorNum, u32 offset, u8 *dest, u32 size)
{
    struct SaveSector *sector = (struct SaveSector *)dest;
    struct TestFlashSectorMeta *meta;
    u8 slot;

    (void)offset;
    memset(dest, 0, size);
    if (sectorNum >= NUM_SAVE_SLOTS * NUM_SECTORS_PER_SLOT || size < sizeof(*sector))
        return;

    meta = &sFlashMeta[sectorNum];
    if (!meta->valid)
        return;

    slot = sectorNum / NUM_SECTORS_PER_SLOT;
    if (meta->id == SECTOR_ID_SAVEBLOCK1_END)
        memcpy(sector->data, sFlashSaveBlock1Tail[slot], JOHTO_SAVE_SERIALIZED_TAIL_SIZE);
    memcpy(sector->saveBlock3Chunk, meta->saveBlock3Chunk, sizeof(sector->saveBlock3Chunk));
    sector->id = meta->id;
    sector->checksum = meta->checksum;
    sector->signature = meta->signature;
    sector->counter = meta->counter;
}

static void TestFlashStoreSector(u16 sectorNum, const struct SaveSector *sector)
{
    struct TestFlashSectorMeta *meta;
    u8 slot;

    if (sectorNum >= NUM_SAVE_SLOTS * NUM_SECTORS_PER_SLOT
     || sector->id >= NUM_SECTORS_PER_SLOT)
        return;

    slot = sectorNum / NUM_SECTORS_PER_SLOT;
    meta = &sFlashMeta[sectorNum];
    if (sector->id == SECTOR_ID_SAVEBLOCK1_END)
        memcpy(sFlashSaveBlock1Tail[slot], sector->data, JOHTO_SAVE_SERIALIZED_TAIL_SIZE);
    memcpy(meta->saveBlock3Chunk, sector->saveBlock3Chunk, sizeof(meta->saveBlock3Chunk));
    meta->id = sector->id;
    /* The fixture stores the production sector-5 bytes.  Other sectors are
     * intentionally zero-filled and use their zero-data checksum because
     * this test only needs their valid slot/footer plumbing. */
    meta->checksum = sector->id == SECTOR_ID_SAVEBLOCK1_END ? sector->checksum : 0;
    meta->signature = sector->signature;
    meta->counter = sector->counter;
    meta->valid = TRUE;
}

static u16 TestFlashProgramSector(u16 sectorNum, u8 *data)
{
    const struct SaveSector *sector = (const struct SaveSector *)data;
    u8 slot = sectorNum / NUM_SECTORS_PER_SLOT;

    if (sectorNum >= NUM_SAVE_SLOTS * NUM_SECTORS_PER_SLOT
     || sector->id >= NUM_SECTORS_PER_SLOT)
        return 1;

    if (sFullWriteCount < NUM_SECTORS_PER_SLOT * NUM_SAVE_SLOTS)
    {
        if (sFirstFullWriteSector[slot] == 0xFFFF)
            sFirstFullWriteSector[slot] = sectorNum;
        sFullWriteCount++;
    }
    TestFlashStoreSector(sectorNum, sector);
    return 0;
}

static u16 TestFlashEraseSector(u16 sectorNum)
{
    if (sectorNum >= SECTORS_COUNT)
        return 1;

    sEraseCalls++;
    sPartialSector = sectorNum;
    memset(&sPartialWriteBuffer, 0xFF, sizeof(sPartialWriteBuffer));
    sFlashMeta[sectorNum].valid = FALSE;
    return 0;
}

static u16 TestFlashProgramByte(u16 sectorNum, u32 offset, u8 data)
{
    if (sectorNum >= NUM_SAVE_SLOTS * NUM_SECTORS_PER_SLOT || offset >= SECTOR_SIZE)
        return 1;

    sPartialByteWrites++;
    if (offset == SECTOR_SIGNATURE_OFFSET)
    {
        /* SAVE_LINK writes all sector bodies before committing their first
         * signature bytes. Keep each pending footer across that second pass. */
        if (!sFlashMeta[sectorNum].valid)
            return 1;
        sFlashMeta[sectorNum].signature =
            (sFlashMeta[sectorNum].signature & 0xFFFFFF00u) | data;
        sPartialSectorCommits++;
        return 0;
    }
    if (sectorNum != sPartialSector)
        return 1;
    ((u8 *)&sPartialWriteBuffer)[offset] = data;
    if (offset == SECTOR_SIZE - 1)
        TestFlashStoreSector(sectorNum, &sPartialWriteBuffer);
    return 0;
}

static u16 CountFlashErase(u16 sectorNum)
{
    (void)sectorNum;
    sEraseCalls++;
    return 0;
}

static u16 CountFlashProgramSector(u16 sectorNum, u8 *data)
{
    (void)sectorNum;
    (void)data;
    sProgramSectorCalls++;
    return 0;
}

static u16 CountFlashProgramByte(u16 sectorNum, u32 offset, u8 data)
{
    (void)sectorNum;
    (void)offset;
    (void)data;
    sProgramByteCalls++;
    return 0;
}

static void EstablishTestCloudSession(void)
{
    struct CoopBridgeMessage message;

    CoopNetBridge_Init();
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  1,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT_EQ(CoopNetBridge_GetCheckpointState(), COOP_CHECKPOINT_STATE_IDLE);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ROM_READY);
}

static void AuthorizeTestCloudSave(u32 sequence)
{
    struct CoopBridgeMessage message;

    EXPECT_EQ(CoopNetBridge_RequestCheckpoint(), COOP_CHECKPOINT_REQUEST_STARTED);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_CHECKPOINT_READY);
    EXPECT(CoopBridgeMessage_Seal(&message,
                                  COOP_BRIDGE_MESSAGE_CHECKPOINT_GRANTED,
                                  sequence,
                                  17,
                                  NULL,
                                  0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(CoopNetBridge_ConsumeCheckpointGrant());
}

static u16 ChecksumWords(const u8 *data, u16 size)
{
    u16 i;
    u32 checksum = 0;

    for (i = 0; i < size; i += sizeof(u32))
    {
        u32 word = (u32)data[i]
                   | ((u32)data[i + 1] << 8)
                   | ((u32)data[i + 2] << 16)
                   | ((u32)data[i + 3] << 24);
        checksum += word;
    }
    return (checksum >> 16) + checksum;
}

TEST("Johto save extension has a bounded append-only layout")
{
    EXPECT_EQ(sizeof(struct JohtoSaveV1), JOHTO_SAVE_V1_SIZE);
    EXPECT_EQ(offsetof(struct SaveBlock1ASLR, johto), sizeof(struct SaveBlock1));
    EXPECT_EQ(offsetof(struct SaveBlock1ASLR, aslr),
              sizeof(struct SaveBlock1) + sizeof(struct JohtoSaveV1));
    EXPECT_EQ(JOHTO_SAVE_LEGACY_TAIL_SIZE + sizeof(struct JohtoSaveV1),
              JOHTO_SAVE_SERIALIZED_TAIL_SIZE);
    EXPECT_EQ(offsetof(struct JohtoSaveV1, flag_bits), 0x08);
    EXPECT_EQ(offsetof(struct JohtoSaveV1, trainer_bits), 0x68);
    EXPECT_EQ(offsetof(struct JohtoSaveV1, variables), 0xA8);
    EXPECT_EQ(offsetof(struct JohtoSaveV1, rival_name), 0x168);
    EXPECT_EQ(offsetof(struct JohtoSaveV1, crc32), 0x170);
}

TEST("Johto save preserves old zero tails in both rotating slots")
{
    static EWRAM_DATA u8 sLegacyTail[NUM_SAVE_SLOTS][JOHTO_SAVE_SERIALIZED_TAIL_SIZE];
    u16 slot;

    for (slot = 0; slot < NUM_SAVE_SLOTS; slot++)
    {
        memset(sLegacyTail[slot], 0, sizeof(sLegacyTail[slot]));
        memset(sLegacyTail[slot], 0xA5, JOHTO_SAVE_LEGACY_TAIL_SIZE);
        EXPECT_EQ(ChecksumWords(sLegacyTail[slot], JOHTO_SAVE_LEGACY_TAIL_SIZE),
                  ChecksumWords(sLegacyTail[slot], JOHTO_SAVE_SERIALIZED_TAIL_SIZE));
    }
}

TEST("Johto zero tail initializes without changing the old SaveBlock1 prefix")
{
    u8 *prefix = (u8 *)&gSaveblock1.block;
    u32 i;

    memset(&gSaveblock1, 0x5A, sizeof(gSaveblock1));
    memset(&gSaveblock1.johto, 0, sizeof(gSaveblock1.johto));
    EXPECT_EQ(JohtoSave_Load(), JOHTO_SAVE_LOAD_INITIALIZED_LEGACY);
    EXPECT(JohtoSave_Validate(&gSaveblock1.johto));
    for (i = 0; i < sizeof(struct SaveBlock1); i++)
        EXPECT_EQ(prefix[i], 0x5A);
}

TEST("Johto save accessors round-trip valid flags trainers and variables")
{
    JohtoSave_InitializeCurrent();

    EXPECT(JohtoSave_SetFlag(0, TRUE));
    EXPECT(JohtoSave_GetFlag(0));
    EXPECT(JohtoSave_SetFlag(767, TRUE));
    EXPECT(JohtoSave_GetFlag(767));
    EXPECT(JohtoSave_SetTrainerDefeated(511, TRUE));
    EXPECT(JohtoSave_GetTrainerDefeated(511));
    EXPECT(JohtoSave_SetVariable(95, 0xBEEF));
    EXPECT_EQ(JohtoSave_GetVariable(95), 0xBEEF);
    EXPECT(JohtoSave_Validate(&gSaveblock1.johto));
    EXPECT_EQ(JohtoSave_Load(), JOHTO_SAVE_LOAD_READY);
    EXPECT(JohtoSave_GetFlag(0));
    EXPECT(JohtoSave_GetTrainerDefeated(511));
    EXPECT_EQ(JohtoSave_GetVariable(95), 0xBEEF);
}

TEST("Johto accessors reject out of bounds ordinals")
{
    JohtoSave_InitializeCurrent();

    EXPECT(!JohtoSave_GetFlag(768));
    EXPECT(!JohtoSave_SetFlag(768, TRUE));
    EXPECT(!JohtoSave_GetTrainerDefeated(512));
    EXPECT(!JohtoSave_SetTrainerDefeated(512, TRUE));
    EXPECT_EQ(JohtoSave_GetVariable(96), 0);
    EXPECT(!JohtoSave_SetVariable(96, 1));
    EXPECT(JohtoSave_Validate(&gSaveblock1.johto));
}

TEST("Johto unknown and corrupt records stay untouched")
{
    struct JohtoSaveV1 snapshot;

    JohtoSave_InitializeCurrent();
    gSaveblock1.johto.schema_version++;
    snapshot = gSaveblock1.johto;
    EXPECT_EQ(JohtoSave_Load(), JOHTO_SAVE_LOAD_INCOMPATIBLE);
    EXPECT_EQ(memcmp(&snapshot, &gSaveblock1.johto, sizeof(snapshot)), 0);

    JohtoSave_InitializeCurrent();
    gSaveblock1.johto.flag_bits[3] ^= 0x40;
    snapshot = gSaveblock1.johto;
    EXPECT_EQ(JohtoSave_Load(), JOHTO_SAVE_LOAD_CORRUPT);
    EXPECT_EQ(memcmp(&snapshot, &gSaveblock1.johto, sizeof(snapshot)), 0);
    EXPECT(!JohtoSave_SetFlag(3, TRUE));
    EXPECT_EQ(memcmp(&snapshot, &gSaveblock1.johto, sizeof(snapshot)), 0);
}

TEST("Johto cloud save validates records before erasing special sectors")
{
    u16 (*programFlashSector)(u16, u8 *) = ProgramFlashSector;
    u16 (*programFlashByte)(u16, u32, u8) = ProgramFlashByte;
    u16 (*eraseFlashSector)(u16) = EraseFlashSector;
    bool32 flashMemoryPresent = gFlashMemoryPresent;
    struct JohtoSaveV1 johtoSnapshot;
    u32 generation;
    u32 saveCounter;

    Save_TestSetFlashReadCallback(NULL);
    Save_TestSetFlashProgramCallback(NULL);
    SetSaveBlocksPointers(0);
    CoopSave_InitializeCurrent();
    JohtoSave_InitializeCurrent();
    EstablishTestCloudSession();
    AuthorizeTestCloudSave(2);

    generation = CoopSave_GetGeneration();
    saveCounter = gSaveCounter;
    gSaveblock1.johto.schema_version++;
    johtoSnapshot = gSaveblock1.johto;
    sEraseCalls = 0;
    sProgramSectorCalls = 0;
    sProgramByteCalls = 0;
    EraseFlashSector = CountFlashErase;
    ProgramFlashSector = CountFlashProgramSector;
    ProgramFlashByte = CountFlashProgramByte;
    gFlashMemoryPresent = TRUE;
    HandleSavingData(SAVE_OVERWRITE_DIFFERENT_FILE);

    EXPECT_EQ(sEraseCalls, 0);
    EXPECT_EQ(sProgramSectorCalls, 0);
    EXPECT_EQ(sProgramByteCalls, 0);
    EXPECT_EQ(CoopSave_GetGeneration(), generation);
    EXPECT_EQ(gSaveCounter, saveCounter);
    EXPECT_EQ(memcmp(&johtoSnapshot, &gSaveblock1.johto, sizeof(johtoSnapshot)), 0);

    /* The unused HOF erase-before branch is fenced by the same preflight. */
    CoopNetBridge_NotifySaveResult(FALSE);
    AuthorizeTestCloudSave(3);
    JohtoSave_InitializeCurrent();
    gSaveblock1.johto.flag_bits[0] ^= 1;
    johtoSnapshot = gSaveblock1.johto;
    sEraseCalls = 0;
    sProgramSectorCalls = 0;
    sProgramByteCalls = 0;
    HandleSavingData(SAVE_HALL_OF_FAME_ERASE_BEFORE);

    EXPECT_EQ(sEraseCalls, 0);
    EXPECT_EQ(sProgramSectorCalls, 0);
    EXPECT_EQ(sProgramByteCalls, 0);
    EXPECT_EQ(CoopSave_GetGeneration(), generation);
    EXPECT_EQ(gSaveCounter, saveCounter);
    EXPECT_EQ(memcmp(&johtoSnapshot, &gSaveblock1.johto, sizeof(johtoSnapshot)), 0);

    CoopNetBridge_NotifySaveResult(FALSE);
    Save_TestSetFlashReadCallback(NULL);
    Save_TestSetFlashProgramCallback(NULL);
    ProgramFlashSector = programFlashSector;
    ProgramFlashByte = programFlashByte;
    EraseFlashSector = eraseFlashSector;
    gFlashMemoryPresent = flashMemoryPresent;
    Save_ResetSaveCounters();
}

static EWRAM_DATA struct JohtoSaveV1 sJohtoSnapshot;
static EWRAM_DATA u8 sLegacyMarkers[2];

TEST("Johto legacy saves load from either rotated slot without losing tail bytes")
{
    u16 slot;
    bool32 flashMemoryPresent = gFlashMemoryPresent;

    SetSaveBlocksPointers(0);
    CoopNetBridge_Init();
    gFlashMemoryPresent = TRUE;
    Save_TestSetFlashReadCallback(TestFlashRead);
    for (slot = 0; slot < NUM_SAVE_SLOTS; slot++)
    {
        u16 index;
        u16 rotation = slot + 3;
        u16 markerSector = slot * NUM_SECTORS_PER_SLOT
                        + (SECTOR_ID_SAVEBLOCK1_END + rotation) % NUM_SECTORS_PER_SLOT;

        TestFlashReset();
        Save_ResetSaveCounters();
        memset(&gSaveblock1, 0, sizeof(gSaveblock1));
        memset(&gSaveblock2, 0, sizeof(gSaveblock2));
        memset(&gSaveblock3, 0, sizeof(gSaveblock3));
        for (index = 0; index < NUM_SECTORS_PER_SLOT; index++)
        {
            struct TestFlashSectorMeta *meta =
                &sFlashMeta[slot * NUM_SECTORS_PER_SLOT + index];
            meta->id = (index + NUM_SECTORS_PER_SLOT - rotation) % NUM_SECTORS_PER_SLOT;
            meta->counter = 8 + slot;
        }
        sFlashSaveBlock1Tail[slot][0] = 0x42;
        /* Old checksum over 264 bytes containing only this one nonzero byte. */
        sFlashMeta[markerSector].checksum = 0x42;
        EXPECT_EQ(LoadGameSave(SAVE_NORMAL), SAVE_STATUS_OK);
        EXPECT_EQ(gLastWrittenSector, rotation);
        EXPECT_EQ(gSaveCounter, 8 + slot);
        EXPECT_EQ(((u8 *)&gSaveblock1.block)[4 * SECTOR_DATA_SIZE], 0x42);
        EXPECT(JohtoSave_Validate(&gSaveblock1.johto));
        EXPECT_EQ(JohtoSave_GetVariable(0), 0);
    }
    Save_TestSetFlashReadCallback(NULL);
    gFlashMemoryPresent = flashMemoryPresent;
    Save_ResetSaveCounters();
}

TEST("Johto extension persists through production full partial saves and heap moves")
{
    u16 (*programFlashSector)(u16, u8 *) = ProgramFlashSector;
    u16 (*programFlashByte)(u16, u32, u8) = ProgramFlashByte;
    u16 (*eraseFlashSector)(u16) = EraseFlashSector;
    bool32 flashMemoryPresent = gFlashMemoryPresent;

    SetSaveBlocksPointers(0);
    CoopNetBridge_Init();
    TestFlashReset();
    Save_ResetSaveCounters();
    memset(&gSaveblock1, 0, sizeof(gSaveblock1));
    memset(&gSaveblock2, 0, sizeof(gSaveblock2));
    memset(&gPokemonStorage, 0, sizeof(gPokemonStorage));
    memset(&gSaveblock3, 0, sizeof(gSaveblock3));
    CoopSave_InitializeCurrent();
    JohtoSave_InitializeCurrent();
    gFlashMemoryPresent = TRUE;
    Save_TestSetFlashReadCallback(TestFlashRead);
    Save_TestSetFlashProgramCallback(TestFlashProgramSector);
    ProgramFlashByte = TestFlashProgramByte;
    EraseFlashSector = TestFlashEraseSector;

    /* Read the seeded legacy sectors through LoadGameSave.  The zero tail is
     * initialized by the production Johto loader before the first write. */
    EXPECT_EQ(LoadGameSave(SAVE_NORMAL), SAVE_STATUS_OK);
    EXPECT(JohtoSave_Validate(&gSaveblock1.johto));

    ((u8 *)&gSaveblock1.block)[sizeof(struct SaveBlock1) - 2] = 0x53;
    ((u8 *)&gSaveblock1.block)[sizeof(struct SaveBlock1) - 1] = 0x4A;
    EXPECT(JohtoSave_SetFlag(0, TRUE));
    EXPECT(JohtoSave_SetFlag(767, TRUE));
    EXPECT(JohtoSave_SetTrainerDefeated(511, TRUE));
    EXPECT(JohtoSave_SetVariable(95, 0xBEEF));

    HandleSavingData(SAVE_NORMAL);
    EXPECT_EQ(sFullWriteCount, NUM_SECTORS_PER_SLOT);
    EXPECT_EQ(sFirstFullWriteSector[1], NUM_SECTORS_PER_SLOT + 1);
    sLegacyMarkers[0] = ((u8 *)&gSaveblock1.block)[sizeof(struct SaveBlock1) - 2];
    sLegacyMarkers[1] = ((u8 *)&gSaveblock1.block)[sizeof(struct SaveBlock1) - 1];
    sJohtoSnapshot = gSaveblock1.johto;

    memset(&gSaveblock1, 0, sizeof(gSaveblock1));
    memset(&gSaveblock2, 0, sizeof(gSaveblock2));
    memset(&gPokemonStorage, 0, sizeof(gPokemonStorage));
    memset(&gSaveblock3, 0, sizeof(gSaveblock3));
    EXPECT_EQ(LoadGameSave(SAVE_NORMAL), SAVE_STATUS_OK);
    EXPECT_EQ(((u8 *)&gSaveblock1.block)[sizeof(struct SaveBlock1) - 2], sLegacyMarkers[0]);
    EXPECT_EQ(((u8 *)&gSaveblock1.block)[sizeof(struct SaveBlock1) - 1], sLegacyMarkers[1]);
    EXPECT_EQ(memcmp(&sJohtoSnapshot, &gSaveblock1.johto, sizeof(sJohtoSnapshot)), 0);

    EXPECT(JohtoSave_SetVariable(0, 0xC0DE));
    EXPECT(JohtoSave_SetFlag(31, TRUE));
    HandleSavingData(SAVE_NORMAL);
    EXPECT_EQ(sFullWriteCount, NUM_SECTORS_PER_SLOT * NUM_SAVE_SLOTS);
    EXPECT_EQ(sFirstFullWriteSector[0], 2);
    sLegacyMarkers[0] = ((u8 *)&gSaveblock1.block)[sizeof(struct SaveBlock1) - 2];
    sLegacyMarkers[1] = ((u8 *)&gSaveblock1.block)[sizeof(struct SaveBlock1) - 1];
    sJohtoSnapshot = gSaveblock1.johto;

    memset(&gSaveblock1, 0, sizeof(gSaveblock1));
    memset(&gSaveblock2, 0, sizeof(gSaveblock2));
    memset(&gPokemonStorage, 0, sizeof(gPokemonStorage));
    memset(&gSaveblock3, 0, sizeof(gSaveblock3));
    EXPECT_EQ(LoadGameSave(SAVE_NORMAL), SAVE_STATUS_OK);
    EXPECT_EQ(((u8 *)&gSaveblock1.block)[sizeof(struct SaveBlock1) - 2], sLegacyMarkers[0]);
    EXPECT_EQ(((u8 *)&gSaveblock1.block)[sizeof(struct SaveBlock1) - 1], sLegacyMarkers[1]);
    EXPECT_EQ(memcmp(&sJohtoSnapshot, &gSaveblock1.johto, sizeof(sJohtoSnapshot)), 0);

    /* SAVE_LINK uses the production byte-at-a-time replacement path for the
     * same six sectors and must carry the appended record with it. */
    EXPECT(JohtoSave_SetVariable(1, 0xFACE));
    sPartialByteWrites = 0;
    sPartialSectorCommits = 0;
    HandleSavingData(SAVE_LINK);
    EXPECT_GT(sPartialByteWrites, 0);
    EXPECT_EQ(sPartialSectorCommits, SECTOR_ID_SAVEBLOCK1_END + 1);
    sLegacyMarkers[0] = ((u8 *)&gSaveblock1.block)[sizeof(struct SaveBlock1) - 2];
    sLegacyMarkers[1] = ((u8 *)&gSaveblock1.block)[sizeof(struct SaveBlock1) - 1];
    sJohtoSnapshot = gSaveblock1.johto;

    memset(&gSaveblock1, 0, sizeof(gSaveblock1));
    memset(&gSaveblock2, 0, sizeof(gSaveblock2));
    memset(&gPokemonStorage, 0, sizeof(gPokemonStorage));
    memset(&gSaveblock3, 0, sizeof(gSaveblock3));
    EXPECT_EQ(LoadGameSave(SAVE_NORMAL), SAVE_STATUS_OK);
    EXPECT_EQ(((u8 *)&gSaveblock1.block)[sizeof(struct SaveBlock1) - 2], sLegacyMarkers[0]);
    EXPECT_EQ(((u8 *)&gSaveblock1.block)[sizeof(struct SaveBlock1) - 1], sLegacyMarkers[1]);
    EXPECT_EQ(memcmp(&sJohtoSnapshot, &gSaveblock1.johto, sizeof(sJohtoSnapshot)), 0);
    EXPECT_EQ(JohtoSave_GetVariable(1), 0xFACE);

    /* The production operation destroys the heap, including the test harness
     * allocations. Keep its state off-heap during the call and rebuild those
     * allocations before returning control to the runner. */
    {
        struct FunctionTestRunnerState runner = *gFunctionTestRunnerState;
        struct ConfigChanges config = *gConfigChangesTestOverride;
        gFunctionTestRunnerState = &runner;
        gConfigChangesTestOverride = &config;
        MoveSaveBlocks_ResetHeap();
        gFunctionTestRunnerState = Alloc(sizeof(runner));
        *gFunctionTestRunnerState = runner;
        gConfigChangesTestOverride = Alloc(sizeof(config));
        *gConfigChangesTestOverride = config;
    }
    EXPECT_EQ(memcmp(&sJohtoSnapshot, &gSaveblock1.johto, sizeof(sJohtoSnapshot)), 0);

    Save_TestSetFlashReadCallback(NULL);
    Save_TestSetFlashProgramCallback(NULL);
    ProgramFlashSector = programFlashSector;
    ProgramFlashByte = programFlashByte;
    EraseFlashSector = eraseFlashSector;
    gFlashMemoryPresent = flashMemoryPresent;
    Save_ResetSaveCounters();
}
