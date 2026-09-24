#include "global.h"
#include "coop/arrival_proof.h"
#include "coop/net_bridge.h"
#include "coop/save.h"
#include "load_save.h"
#include "test/test.h"

static const u8 sEmptySha256[COOP_ARRIVAL_PROOF_HASH_SIZE] =
{
    0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14,
    0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f, 0xb9, 0x24,
    0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c,
    0xa4, 0x95, 0x99, 0x1b, 0x78, 0x52, 0xb8, 0x55,
};

static const u8 sAbcSha256[COOP_ARRIVAL_PROOF_HASH_SIZE] =
{
    0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea,
    0x41, 0x41, 0x40, 0xde, 0x5d, 0xae, 0x22, 0x23,
    0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c,
    0xb4, 0x10, 0xff, 0x61, 0xf2, 0x00, 0x15, 0xad,
};

static const u8 sFlashSha256[COOP_ARRIVAL_PROOF_HASH_SIZE] =
{
    0x3f, 0x2d, 0x4d, 0xb2, 0x8a, 0x65, 0xf2, 0x01,
    0x04, 0xb4, 0x52, 0xd7, 0x6d, 0x08, 0xe6, 0xbb,
    0x4f, 0xcb, 0x86, 0x26, 0x48, 0x7a, 0xdd, 0x92,
    0x10, 0xe5, 0x51, 0x62, 0xd6, 0x2c, 0x37, 0xf4,
};

static const u8 s56ByteSha256[COOP_ARRIVAL_PROOF_HASH_SIZE] =
{
    0xb3, 0x54, 0x39, 0xa4, 0xac, 0x6f, 0x09, 0x48,
    0xb6, 0xd6, 0xf9, 0xe3, 0xc6, 0xaf, 0x0f, 0x5f,
    0x59, 0x0c, 0xe2, 0x0f, 0x1b, 0xde, 0x70, 0x90,
    0xef, 0x79, 0x70, 0x68, 0x6e, 0xc6, 0x73, 0x8a,
};

static u16 sReadCount;

static void ReadPatternFlash(u16 sector, u32 offset, u8 *destination, u32 size)
{
    EXPECT_EQ(sector, sReadCount);
    EXPECT_EQ(offset, 0);
    EXPECT_EQ(size, COOP_ARRIVAL_FLASH_SECTOR_SIZE);
    memset(destination, 0xa5, size);
    sReadCount++;
}

static void QueueChallenge(const u8 nonce[COOP_ARRIVAL_CHALLENGE_NONCE_SIZE], u32 sequence)
{
    struct CoopBridgeMessage message;

    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_ARRIVAL_CHALLENGE,
                                  sequence, 0, nonce,
                                  COOP_ARRIVAL_CHALLENGE_NONCE_SIZE));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
}

TEST("Coop arrival proof SHA-256 matches independent empty and abc vectors")
{
    u8 digest[COOP_ARRIVAL_PROOF_HASH_SIZE];
    u8 boundary[56];

    CoopArrivalProof_TestSha256((const u8 *)"", 0, digest);
    EXPECT(memcmp(digest, sEmptySha256, sizeof(digest)) == 0);
    CoopArrivalProof_TestSha256((const u8 *)"abc", 3, digest);
    EXPECT(memcmp(digest, sAbcSha256, sizeof(digest)) == 0);
    memset(boundary, 'a', sizeof(boundary));
    CoopArrivalProof_TestSha256(boundary, sizeof(boundary), digest);
    EXPECT(memcmp(digest, s56ByteSha256, sizeof(digest)) == 0);
}

TEST("Coop arrival verifier hashes exactly one flash sector per frame and emits one proof")
{
    static const u8 nonce[COOP_ARRIVAL_CHALLENGE_NONCE_SIZE] =
        {1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16};
    struct CoopBridgeMessage frame;
    bool32 flashMemoryPresent = gFlashMemoryPresent;
    u16 sector;
    u16 index;

    CoopSave_InitializeCurrent();
    CoopNetBridge_Init();
    QueueChallenge(nonce, 1);
    EXPECT(CoopArrivalProof_IsVerifierMode());
    EXPECT(!CoopArrivalProof_IsProofReady());
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    EXPECT_EQ(CoopNetBridge_GetSessionEpoch(), 0);
    EXPECT(!CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_ROM_READY, NULL, 0));

    sReadCount = 0;
    gFlashMemoryPresent = TRUE;
    CoopArrivalProof_TestSetFlashReadCallback(ReadPatternFlash);
    CoopArrivalProof_OnContinueSelected();
    CoopArrivalProof_OnFieldEntered(TRUE, TRUE, ROM_WORLD_ID, 7, 3, 4);
    for (sector = 0; sector < COOP_ARRIVAL_FLASH_SECTOR_COUNT - 1; sector++)
    {
        CoopNetBridge_Poll();
        EXPECT_EQ(sReadCount, sector + 1);
        EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    }
    CoopNetBridge_Poll();
    EXPECT_EQ(sReadCount, COOP_ARRIVAL_FLASH_SECTOR_COUNT);
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&frame));
    EXPECT_EQ(frame.type, COOP_BRIDGE_MESSAGE_ARRIVAL_PROOF);
    EXPECT_EQ(frame.length, COOP_ARRIVAL_PROOF_PAYLOAD_SIZE);
    EXPECT_EQ(frame.session_epoch, 0);
    EXPECT(memcmp(&frame.payload[0], nonce, sizeof(nonce)) == 0);
    EXPECT(memcmp(&frame.payload[16], sFlashSha256, sizeof(sFlashSha256)) == 0);
    EXPECT_EQ(frame.payload[48], ROM_WORLD_ID);
    EXPECT_EQ(frame.payload[52], 7);
    EXPECT_EQ(frame.payload[56], 3);
    EXPECT_EQ(frame.payload[57], 4);
    for (index = 58; index < COOP_ARRIVAL_PROOF_PAYLOAD_SIZE; index++)
        EXPECT_EQ(frame.payload[index], 0);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));

    CoopNetBridge_Poll();
    EXPECT_EQ(sReadCount, COOP_ARRIVAL_FLASH_SECTOR_COUNT);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    EXPECT(CoopArrivalProof_IsVerifierMode());
    EXPECT(!CoopArrivalProof_IsProofReady());
    QueueChallenge(nonce, 2);
    EXPECT((gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR) != 0);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    EXPECT_EQ(CoopNetBridge_GetSessionEpoch(), 0);
    CoopArrivalProof_TestSetFlashReadCallback(NULL);
    gFlashMemoryPresent = flashMemoryPresent;
}

TEST("Coop arrival verifier rejects invalid Continue saves and never accepts SessionReady")
{
    static const u8 nonce[COOP_ARRIVAL_CHALLENGE_NONCE_SIZE] =
        {0xaa, 0xbb, 0xcc, 0xdd, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12};
    struct CoopBridgeMessage sessionReady;
    bool32 flashMemoryPresent = gFlashMemoryPresent;

    CoopSave_InitializeCurrent();
    CoopNetBridge_Init();
    QueueChallenge(nonce, 1);
    EXPECT(CoopBridgeMessage_Seal(&sessionReady, COOP_BRIDGE_MESSAGE_SESSION_READY,
                                  2, 1, NULL, 0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&sessionReady));
    CoopNetBridge_Poll();
    EXPECT_EQ(CoopNetBridge_GetSessionEpoch(), 0);
    EXPECT((gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_SESSION_READY) == 0);

    gFlashMemoryPresent = TRUE;
    CoopArrivalProof_OnContinueSelected();
    gSaveBlock3Ptr->coop.crc32 ^= 1;
    EXPECT(!CoopSave_Validate(&gSaveBlock3Ptr->coop));
    CoopArrivalProof_OnFieldEntered(TRUE,
                                    CoopSave_Validate(&gSaveBlock3Ptr->coop),
                                    ROM_WORLD_ID, 7, 3, 4);
    gSaveBlock3Ptr->coop.crc32 ^= 1;
    EXPECT(CoopArrivalProof_IsVerifierMode());
    EXPECT(!CoopArrivalProof_IsProofReady());
    EXPECT(!CoopArrivalProof_BeginChallenge(nonce, sizeof(nonce)));
    CoopNetBridge_Poll();
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    gFlashMemoryPresent = flashMemoryPresent;
}

TEST("Coop arrival challenge refuses zero nonce and field entry without Continue")
{
    static const u8 zeroNonce[COOP_ARRIVAL_CHALLENGE_NONCE_SIZE] = {0};
    static const u8 nonce[COOP_ARRIVAL_CHALLENGE_NONCE_SIZE] = {1};
    bool32 flashMemoryPresent = gFlashMemoryPresent;

    CoopArrivalProof_Reset();
    EXPECT(!CoopArrivalProof_BeginChallenge(zeroNonce, sizeof(zeroNonce)));
    EXPECT(!CoopArrivalProof_IsVerifierMode());
    EXPECT(CoopArrivalProof_BeginChallenge(nonce, sizeof(nonce)));
    gFlashMemoryPresent = TRUE;
    CoopArrivalProof_OnFieldEntered(TRUE, TRUE, ROM_WORLD_ID, 7, 3, 4);
    EXPECT(!CoopArrivalProof_IsProofReady());
    CoopArrivalProof_OnContinueSelected();
    CoopArrivalProof_OnFieldEntered(TRUE, TRUE, ROM_WORLD_ID, 0, 3, 4);
    EXPECT(CoopArrivalProof_IsVerifierMode());
    EXPECT(!CoopArrivalProof_IsProofReady());
    gFlashMemoryPresent = flashMemoryPresent;
}

TEST("Coop arrival challenge rejects nonzero frame padding without entering verifier mode")
{
    static const u8 nonce[COOP_ARRIVAL_CHALLENGE_NONCE_SIZE] = {1};
    struct CoopBridgeMessage message;

    CoopSave_InitializeCurrent();
    CoopNetBridge_Init();
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_ARRIVAL_CHALLENGE,
                                  1, 0, nonce, sizeof(nonce)));
    message.payload[sizeof(nonce)] = 1;
    message.checksum = CoopBridgeMessage_ComputeChecksum(&message);
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    EXPECT(!CoopArrivalProof_IsVerifierMode());
    EXPECT((gCoopNetBridge.status_flags & COOP_BRIDGE_STATUS_PROTOCOL_ERROR) != 0);
    EXPECT_EQ(CoopNetBridge_GetSessionEpoch(), 0);
}
