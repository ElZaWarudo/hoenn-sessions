#include "global.h"
#include "coop/battle_runtime.h"
#include "coop/net_bridge.h"
#include "coop/generated_regional_identities.h"
#include "constants/battle.h"
#include "constants/opponents.h"
#include "pokemon.h"
#include "test/test.h"

TEST("Cloud Coop action codec keeps member targets canonical and rejects invalid actions")
{
    struct CoopBattleAction move = { COOP_BATTLE_ACTION_MOVE, 3, B_POSITION_PLAYER_RIGHT };
    struct CoopBattleAction decoded = {0};
    u8 bytes[COOP_BATTLE_ACTION_SIZE] = {0};

    EXPECT(CoopBattleRuntime_EncodeAction(&move, bytes, sizeof(bytes)));
    EXPECT_EQ(bytes[0], COOP_BATTLE_ACTION_MOVE);
    EXPECT_EQ(bytes[1], 3);
    EXPECT_EQ(bytes[2], B_POSITION_PLAYER_RIGHT);
    EXPECT_EQ(bytes[3], 0);
    EXPECT(CoopBattleRuntime_DecodeAction(bytes, sizeof(bytes), &decoded));
    EXPECT_EQ(decoded.kind, move.kind);
    EXPECT_EQ(decoded.index, move.index);
    EXPECT_EQ(decoded.target, move.target);
    EXPECT_EQ(CoopBattleRuntime_TranslateTarget(B_POSITION_PLAYER_LEFT, 1), B_POSITION_PLAYER_RIGHT);
    EXPECT_EQ(CoopBattleRuntime_TranslateTarget(B_POSITION_PLAYER_RIGHT, 1), B_POSITION_PLAYER_LEFT);
    EXPECT_EQ(CoopBattleRuntime_TranslateTarget(B_POSITION_OPPONENT_RIGHT, 1), B_POSITION_OPPONENT_RIGHT);
    EXPECT_EQ(CoopBattleRuntime_TranslateTarget(B_POSITION_PLAYER_RIGHT, 0), B_POSITION_PLAYER_RIGHT);
    EXPECT_EQ(CoopBattleRuntime_TranslateTarget(4, 0), 0xFF);

    bytes[3] = 1;
    EXPECT(!CoopBattleRuntime_DecodeAction(bytes, sizeof(bytes), &decoded));
    bytes[3] = 0;
    bytes[1] = MAX_MON_MOVES;
    EXPECT(!CoopBattleRuntime_DecodeAction(bytes, sizeof(bytes), &decoded));
    bytes[0] = COOP_BATTLE_ACTION_FORCED_SWITCH;
    bytes[1] = 2;
    bytes[2] = 0;
    EXPECT(CoopBattleRuntime_DecodeAction(bytes, sizeof(bytes), &decoded));
    bytes[1] = COOP_BATTLE_MULTI_PARTY_SIZE;
    EXPECT(!CoopBattleRuntime_DecodeAction(bytes, sizeof(bytes), &decoded));
    bytes[0] = COOP_BATTLE_ACTION_NO_ACTION;
    bytes[1] = 0;
    EXPECT(CoopBattleRuntime_DecodeAction(bytes, sizeof(bytes), &decoded));
    bytes[2] = 1;
    EXPECT(!CoopBattleRuntime_DecodeAction(bytes, sizeof(bytes), &decoded));
    bytes[0] = COOP_BATTLE_ACTION_AUTO_MOVE;
    bytes[2] = 0;
    EXPECT(CoopBattleRuntime_DecodeAction(bytes, sizeof(bytes), &decoded));
    EXPECT_EQ(decoded.kind, COOP_BATTLE_ACTION_AUTO_MOVE);
    bytes[1] = 1;
    EXPECT(!CoopBattleRuntime_DecodeAction(bytes, sizeof(bytes), &decoded));
    EXPECT(!CoopBattleRuntime_DecodeAction(bytes, sizeof(bytes) - 1, &decoded));
}

TEST("Cloud Coop turn bundle rejects noncanonical action bytes")
{
    u8 bundle[20 + 2 * COOP_BATTLE_ACTION_SIZE] = {0};

    bundle[0] = 1;
    bundle[16] = 1;
    bundle[18] = COOP_BATTLE_ACTION_SIZE;
    bundle[19] = COOP_BATTLE_ACTION_SIZE;
    bundle[20] = COOP_BATTLE_ACTION_MOVE;
    bundle[21] = 0;
    bundle[22] = B_POSITION_OPPONENT_LEFT;
    bundle[24] = COOP_BATTLE_ACTION_FORCED_SWITCH;
    bundle[25] = 1;
    CoopBattleRuntime_Init();
    EXPECT_EQ(CoopBattleRuntime_ReceiveTurnBundle(bundle, sizeof(bundle)), COOP_BATTLE_INBOUND_IGNORED);
    bundle[27] = 1;
    EXPECT_EQ(CoopBattleRuntime_ReceiveTurnBundle(bundle, sizeof(bundle)), COOP_BATTLE_INBOUND_MALFORMED);
}

static void ReceiveManifest(u8 id)
{
    u8 manifest[COOP_BATTLE_MANIFEST_SIZE] = {0};

    manifest[0] = id;
    manifest[COOP_BATTLE_MANIFEST_KIND_OFFSET] = COOP_BATTLE_MANIFEST_KIND_FRIENDLY;
    EXPECT_EQ(CoopBattleRuntime_ReceiveManifest(manifest, sizeof(manifest)),
              COOP_BATTLE_INBOUND_ACCEPTED);
}

TEST("Cloud Coop active battle reports transport readiness for its pause screen")
{
    CoopBattleRuntime_Init();
    EXPECT(!CoopBattleRuntime_IsSessionReady());
    CoopBattleRuntime_OnSessionReady(7);
    EXPECT(CoopBattleRuntime_IsSessionReady());
    CoopBattleRuntime_OnTransportLost();
    EXPECT(!CoopBattleRuntime_IsSessionReady());
    CoopBattleRuntime_OnSessionReady(7);
    EXPECT(CoopBattleRuntime_IsSessionReady());
}

TEST("Cloud Coop start release rejects malformed and premature records")
{
    u8 battle_id[COOP_BATTLE_START_SIZE] = {7};
    u8 zero[COOP_BATTLE_START_SIZE] = {0};

    CoopBattleRuntime_Init();
    CoopBattleRuntime_OnSessionReady(7);
    EXPECT_EQ(CoopBattleRuntime_ReceiveStart(NULL, sizeof(battle_id)),
              COOP_BATTLE_INBOUND_MALFORMED);
    EXPECT_EQ(CoopBattleRuntime_ReceiveStart(zero, sizeof(zero)),
              COOP_BATTLE_INBOUND_MALFORMED);
    EXPECT_EQ(CoopBattleRuntime_ReceiveStart(battle_id, sizeof(battle_id) - 1),
              COOP_BATTLE_INBOUND_MALFORMED);
    EXPECT_EQ(CoopBattleRuntime_ReceiveStart(battle_id, sizeof(battle_id)),
              COOP_BATTLE_INBOUND_IGNORED);
    ReceiveManifest(7);
    EXPECT_EQ(CoopBattleRuntime_ReceiveStart(battle_id, sizeof(battle_id)),
              COOP_BATTLE_INBOUND_IGNORED);
    EXPECT(!CoopBattleRuntime_IsStartReleased());
}

TEST("Cloud Coop same-epoch reconnect requires a fresh ready before start replay")
{
    u8 battle_id[COOP_BATTLE_START_SIZE] = {7};
    u8 old_ready[COOP_BATTLE_READY_SIZE] = {7};
    struct CoopBridgeMessage queued;

    CoopBattleRuntime_Init();
    CoopBattleRuntime_OnSessionReady(7);
    ReceiveManifest(7);
    CoopBattleRuntime_TestSetReadyForStart();
    EXPECT_EQ(CoopBattleRuntime_ReceiveStart(battle_id, sizeof(battle_id)),
              COOP_BATTLE_INBOUND_ACCEPTED);
    /* A newer SESSION_READY can replace the bridge before heartbeat loss. */
    CoopBridgeQueue_Init(&gCoopNetBridge.game_to_network);
    EXPECT(CoopBridgeMessage_Seal(&queued, COOP_BRIDGE_MESSAGE_BATTLE_READY,
                                  2, 7, old_ready, sizeof(old_ready)));
    EXPECT(CoopBridgeQueue_Push(&gCoopNetBridge.game_to_network, &queued));
    CoopBattleRuntime_PreserveOutbound();
    EXPECT(CoopBattleRuntime_HasPendingOutboundReplay());
    CoopBattleRuntime_OnSessionReady(7);
    EXPECT(!CoopBattleRuntime_HasPendingOutboundReplay());
    EXPECT(!CoopBattleRuntime_IsStartReleased());
    EXPECT_EQ(CoopBattleRuntime_ReceiveStart(battle_id, sizeof(battle_id)),
              COOP_BATTLE_INBOUND_IGNORED);
    CoopBattleRuntime_TestSetReadyForStart();
    EXPECT_EQ(CoopBattleRuntime_ReceiveStart(battle_id, sizeof(battle_id)),
              COOP_BATTLE_INBOUND_ACCEPTED);
    EXPECT(CoopBattleRuntime_IsStartReleased());
    CoopBattleRuntime_OnTransportLost();
    CoopBattleRuntime_OnSessionReady(7);
    EXPECT(!CoopBattleRuntime_IsStartReleased());
    EXPECT_EQ(CoopBattleRuntime_ReceiveStart(battle_id, sizeof(battle_id)),
              COOP_BATTLE_INBOUND_IGNORED);
    CoopBattleRuntime_TestSetReadyForStart();
    EXPECT_EQ(CoopBattleRuntime_ReceiveStart(battle_id, sizeof(battle_id)),
              COOP_BATTLE_INBOUND_ACCEPTED);
}

/* CreateMon builds the box record but leaves the battle HP fields at zero;
 * CalculateMonStats makes these fixtures represent usable party members. */
static void CreateUsableTestMon(struct Pokemon *mon, enum Species species,
                                u8 level, u32 personality,
                                struct OriginalTrainerId trainerId)
{
    CreateMon(mon, species, level, personality, trainerId);
    CalculateMonStats(mon);
}

static void ReceiveTrainerManifest(u8 id, u8 region, u16 ordinal, u8 slot,
                                   const struct Pokemon *local, u8 local_count)
{
    u8 manifest[COOP_BATTLE_MANIFEST_SIZE] = {0};

    manifest[0] = id;
    manifest[COOP_BATTLE_MANIFEST_KIND_OFFSET] = COOP_BATTLE_MANIFEST_KIND_TRAINER;
    manifest[COOP_BATTLE_MANIFEST_MEMBER_SLOT_OFFSET] = slot;
    manifest[COOP_BATTLE_MANIFEST_REGION_OFFSET] = region;
    manifest[COOP_BATTLE_MANIFEST_TRAINER_ORDINAL_OFFSET] = (u8)ordinal;
    manifest[COOP_BATTLE_MANIFEST_TRAINER_ORDINAL_OFFSET + 1] = ordinal >> 8;
    EXPECT(CoopBattleRuntime_ComputePartyDigest(local, local_count,
                                                 &manifest[50 + slot * COOP_BATTLE_DIGEST_SIZE],
                                                 COOP_BATTLE_DIGEST_SIZE));
    EXPECT_EQ(CoopBattleRuntime_ReceiveManifest(manifest, sizeof(manifest)),
              COOP_BATTLE_INBOUND_ACCEPTED);
}

TEST("Cloud Coop party digest matches the launcher snapshot format")
{
    struct Pokemon party[2];
    u8 digest[COOP_BATTLE_DIGEST_SIZE] = {0};
    u8 i;
    static const u8 expected[COOP_BATTLE_DIGEST_SIZE] = {
        0x29, 0x4b, 0x0d, 0x84, 0x44, 0x93, 0xfa, 0x58,
        0xd6, 0x6d, 0x41, 0xb0, 0x9f, 0x5d, 0x84, 0x40,
        0x4f, 0x88, 0x42, 0xa1, 0xb1, 0x2c, 0x87, 0xc1,
        0x51, 0x4a, 0xc3, 0xf8, 0x8f, 0x8a, 0xe8, 0xf8,
    };

    memset(&party[0], 0, sizeof(party[0]));
    memset(&party[1], 0xab, sizeof(party[1]));
    EXPECT(CoopBattleRuntime_ComputePartyDigest(party, 2, digest, sizeof(digest)));
    for (i = 0; i < sizeof(digest); i++)
        EXPECT_EQ(digest[i], expected[i]);
    EXPECT(!CoopBattleRuntime_ComputePartyDigest(party, 0, digest, sizeof(digest)));
    EXPECT(!CoopBattleRuntime_ComputePartyDigest(party, 2, digest, sizeof(digest) - 1));
}

TEST("Cloud Coop battle manifest rejects malformed identity and exact old size")
{
    u8 manifest[COOP_BATTLE_MANIFEST_SIZE] = {0};
    struct CoopBattleManifestIdentity identity;
    u8 id[COOP_BATTLE_ID_SIZE] = {42};

    CoopBattleRuntime_Init();
    CoopBattleRuntime_OnSessionReady(42);
    manifest[0] = 42;
    manifest[COOP_BATTLE_MANIFEST_KIND_OFFSET] = COOP_BATTLE_MANIFEST_KIND_FRIENDLY;
    EXPECT_EQ(CoopBattleRuntime_ReceiveManifest(manifest, 114), COOP_BATTLE_INBOUND_MALFORMED);
    manifest[COOP_BATTLE_MANIFEST_MEMBER_SLOT_OFFSET] = 2;
    EXPECT_EQ(CoopBattleRuntime_ReceiveManifest(manifest, sizeof(manifest)), COOP_BATTLE_INBOUND_MALFORMED);
    manifest[COOP_BATTLE_MANIFEST_MEMBER_SLOT_OFFSET] = 1;
    manifest[COOP_BATTLE_MANIFEST_REGION_OFFSET] = COOP_REGION_HOENN;
    EXPECT_EQ(CoopBattleRuntime_ReceiveManifest(manifest, sizeof(manifest)), COOP_BATTLE_INBOUND_MALFORMED);
    manifest[COOP_BATTLE_MANIFEST_REGION_OFFSET] = 0;
    manifest[COOP_BATTLE_MANIFEST_KIND_OFFSET] = COOP_BATTLE_MANIFEST_KIND_TRAINER;
    EXPECT_EQ(CoopBattleRuntime_ReceiveManifest(manifest, sizeof(manifest)), COOP_BATTLE_INBOUND_MALFORMED);
    manifest[COOP_BATTLE_MANIFEST_REGION_OFFSET] = COOP_REGION_KANTO;
    manifest[COOP_BATTLE_MANIFEST_TRAINER_ORDINAL_OFFSET] = (u8)COOP_TRAINER_HOENN_TRAINER_WALLY_1_ORDINAL;
    manifest[COOP_BATTLE_MANIFEST_TRAINER_ORDINAL_OFFSET + 1] = COOP_TRAINER_HOENN_TRAINER_WALLY_1_ORDINAL >> 8;
    EXPECT_EQ(CoopBattleRuntime_ReceiveManifest(manifest, sizeof(manifest)), COOP_BATTLE_INBOUND_MALFORMED);
    manifest[COOP_BATTLE_MANIFEST_KIND_OFFSET] = 3;
    EXPECT_EQ(CoopBattleRuntime_ReceiveManifest(manifest, sizeof(manifest)), COOP_BATTLE_INBOUND_MALFORMED);
    EXPECT(!CoopBattleRuntime_GetManifestIdentity(id, &identity));
    manifest[COOP_BATTLE_MANIFEST_KIND_OFFSET] = COOP_BATTLE_MANIFEST_KIND_FRIENDLY;
    manifest[COOP_BATTLE_MANIFEST_REGION_OFFSET] = 0;
    manifest[COOP_BATTLE_MANIFEST_TRAINER_ORDINAL_OFFSET] = 0;
    manifest[COOP_BATTLE_MANIFEST_TRAINER_ORDINAL_OFFSET + 1] = 0;
    EXPECT_EQ(CoopBattleRuntime_ReceiveManifest(manifest, sizeof(manifest)), COOP_BATTLE_INBOUND_ACCEPTED);
    EXPECT(CoopBattleRuntime_GetManifestIdentity(id, &identity));
    EXPECT_EQ(identity.kind, COOP_BATTLE_MANIFEST_KIND_FRIENDLY);
    EXPECT_EQ(identity.local_member_slot, 1);
    EXPECT_EQ(identity.trainer_region, 0);
    EXPECT_EQ(identity.trainer_ordinal, 0);
}

static enum CoopBattleInboundResult ReceivePeerMon(u8 id, u8 slot, u8 count,
                                                   const struct Pokemon *mon)
{
    u8 chunk[COOP_BATTLE_PARTY_SNAPSHOT_SIZE] = {0};

    chunk[0] = id;
    chunk[16] = slot;
    chunk[17] = slot;
    chunk[18] = count;
    chunk[19] = COOP_BATTLE_PARTY_MON_SIZE;
    memcpy(chunk + 20, mon, sizeof(*mon));
    return CoopBattleRuntime_ReceivePeerPartyChunk(chunk, sizeof(chunk));
}

TEST("Cloud Coop partner party preparation requires the complete current battle")
{
    struct Pokemon local[PARTY_SIZE] = {0};
    struct Pokemon peer[3];
    struct CoopBattlePreparedParty prepared;
    struct CoopBattlePreparedParty untouched;
    u8 id[COOP_BATTLE_ID_SIZE] = {7};
    u8 stale[COOP_BATTLE_ID_SIZE] = {8};
    u8 i;

    CreateUsableTestMon(&peer[0], SPECIES_BULBASAUR, 5, 1, OTID_STRUCT_PRESET(1));
    CreateUsableTestMon(&peer[1], SPECIES_CHARMANDER, 5, 2, OTID_STRUCT_PRESET(1));
    CreateUsableTestMon(&peer[2], SPECIES_SQUIRTLE, 5, 3, OTID_STRUCT_PRESET(1));
    for (i = 0; i < 3; i++)
        CreateUsableTestMon(&local[i], SPECIES_CHARMANDER, 5, i + 1, OTID_STRUCT_PRESET(1));
    memset(&prepared, 0xA5, sizeof(prepared));
    untouched = prepared;
    CoopBattleRuntime_Init();
    CoopBattleRuntime_OnSessionReady(17);
    EXPECT(!CoopBattleRuntime_PreparePartnerParty(id, local, PARTY_SIZE, &prepared));
    ReceiveManifest(7);
    EXPECT(!CoopBattleRuntime_PreparePartnerParty(id, local, PARTY_SIZE, &prepared));
    EXPECT_EQ(ReceivePeerMon(7, 1, 3, &peer[1]), COOP_BATTLE_INBOUND_IGNORED);
    EXPECT_EQ(ReceivePeerMon(7, 0, 3, &peer[0]), COOP_BATTLE_INBOUND_ACCEPTED);
    EXPECT_EQ(ReceivePeerMon(7, 0, 3, &peer[0]), COOP_BATTLE_INBOUND_IGNORED);
    EXPECT_EQ(ReceivePeerMon(7, 1, 3, &peer[1]), COOP_BATTLE_INBOUND_ACCEPTED);
    EXPECT(!CoopBattleRuntime_PreparePartnerParty(id, local, PARTY_SIZE, &prepared));
    EXPECT_EQ(memcmp(&prepared, &untouched, sizeof(prepared)), 0);
    EXPECT_EQ(ReceivePeerMon(7, 2, 3, &peer[2]), COOP_BATTLE_INBOUND_ACCEPTED);
    EXPECT(!CoopBattleRuntime_PreparePartnerParty(stale, local, PARTY_SIZE, &prepared));
    EXPECT_EQ(memcmp(&prepared, &untouched, sizeof(prepared)), 0);
    EXPECT(CoopBattleRuntime_PreparePartnerParty(id, local, PARTY_SIZE, &prepared));
    for (i = 0; i < 3; i++)
    {
        EXPECT_EQ(prepared.local_slots[i], i);
        EXPECT_EQ(prepared.peer_slots[i], i);
        EXPECT_EQ(memcmp(&prepared.local[i], &local[i], sizeof(peer[i])), 0);
        EXPECT_EQ(memcmp(&prepared.peer[i], &peer[i], sizeof(peer[i])), 0);
    }
    prepared.local[0].hp = 0;
    prepared.peer[0].hp = 0;
    EXPECT(GetMonData(&local[0], MON_DATA_HP) != 0);
    EXPECT(CoopBattleRuntime_PreparePartnerParty(id, local, PARTY_SIZE, &prepared));
    EXPECT(GetMonData(&prepared.peer[0], MON_DATA_HP) != 0);
    CoopBattleRuntime_OnTransportLost();
    EXPECT(!CoopBattleRuntime_PreparePartnerParty(id, local, PARTY_SIZE, &prepared));
    CoopBattleRuntime_OnSessionReady(18);
    EXPECT(!CoopBattleRuntime_PreparePartnerParty(id, local, PARTY_SIZE, &prepared));
}

TEST("Cloud Coop partner party preparation needs three usable mons on each side")
{
    struct Pokemon local[PARTY_SIZE] = {0};
    struct Pokemon peer;
    struct CoopBattlePreparedParty prepared;
    struct CoopBattlePreparedParty untouched;
    u8 id[COOP_BATTLE_ID_SIZE] = {9};
    u8 i;

    CreateUsableTestMon(&peer, SPECIES_BULBASAUR, 5, 1, OTID_STRUCT_PRESET(1));
    memset(&prepared, 0xA5, sizeof(prepared));
    untouched = prepared;
    CoopBattleRuntime_Init();
    CoopBattleRuntime_OnSessionReady(19);
    ReceiveManifest(9);
    for (i = 0; i < 3; i++)
        EXPECT_EQ(ReceivePeerMon(9, i, 3, &peer), COOP_BATTLE_INBOUND_ACCEPTED);
    for (i = 0; i < 3; i++)
    {
        EXPECT(!CoopBattleRuntime_PreparePartnerParty(id, local, PARTY_SIZE, &prepared));
        EXPECT_EQ(memcmp(&prepared, &untouched, sizeof(prepared)), 0);
        CreateUsableTestMon(&local[i * 2], SPECIES_CHARMANDER, 5, i + 1, OTID_STRUCT_PRESET(1));
    }
    EXPECT(CoopBattleRuntime_PreparePartnerParty(id, local, PARTY_SIZE, &prepared));
    EXPECT_EQ(prepared.local_slots[0], 0);
    EXPECT_EQ(prepared.local_slots[1], 2);
    EXPECT_EQ(prepared.local_slots[2], 4);
    EXPECT(!CoopBattleRuntime_PreparePartnerParty(id, local, 2, &prepared));
    EXPECT(!CoopBattleRuntime_PreparePartnerParty(id, local, PARTY_SIZE + 1, &prepared));
    EXPECT(!CoopBattleRuntime_PreparePartnerParty(NULL, local, PARTY_SIZE, &prepared));
}

TEST("Cloud Coop partner party preparation refuses fainted peer records")
{
    struct Pokemon local[PARTY_SIZE] = {0};
    struct Pokemon peer;
    struct CoopBattlePreparedParty prepared;
    u8 id[COOP_BATTLE_ID_SIZE] = {10};
    u8 i;
    u16 no_hp = 0;

    CreateUsableTestMon(&peer, SPECIES_BULBASAUR, 5, 1, OTID_STRUCT_PRESET(1));
    for (i = 0; i < 3; i++)
        CreateUsableTestMon(&local[i], SPECIES_CHARMANDER, 5, i + 1, OTID_STRUCT_PRESET(1));
    CoopBattleRuntime_Init();
    CoopBattleRuntime_OnSessionReady(20);
    ReceiveManifest(10);
    EXPECT_EQ(ReceivePeerMon(10, 0, 3, &peer), COOP_BATTLE_INBOUND_ACCEPTED);
    SetMonData(&peer, MON_DATA_HP, &no_hp);
    EXPECT_EQ(ReceivePeerMon(10, 1, 3, &peer), COOP_BATTLE_INBOUND_ACCEPTED);
    EXPECT_EQ(ReceivePeerMon(10, 2, 3, &peer), COOP_BATTLE_INBOUND_ACCEPTED);
    EXPECT(!CoopBattleRuntime_PreparePartnerParty(id, local, PARTY_SIZE, &prepared));
}

TEST("Cloud Coop partner party preparation selects usable slots from full peer parties")
{
    struct Pokemon local[PARTY_SIZE] = {0};
    struct Pokemon peer;
    struct CoopBattlePreparedParty prepared;
    struct CoopBattlePreparedParty untouched;
    u8 id[COOP_BATTLE_ID_SIZE] = {0};
    u8 count;
    u8 slot;
    u16 no_hp = 0;

    for (slot = 0; slot < 3; slot++)
        CreateUsableTestMon(&local[slot], SPECIES_CHARMANDER, 5, slot + 1, OTID_STRUCT_PRESET(1));
    memset(&prepared, 0xA5, sizeof(prepared));
    untouched = prepared;
    for (count = 1; count <= PARTY_SIZE; count++)
    {
        id[0] = count + 20;
        CoopBattleRuntime_Init();
        CoopBattleRuntime_OnSessionReady(count + 20);
        ReceiveManifest(id[0]);
        for (slot = 0; slot < count; slot++)
        {
            CreateUsableTestMon(&peer, SPECIES_BULBASAUR, 5, slot + 1, OTID_STRUCT_PRESET(1));
            if (slot == 1)
                SetMonData(&peer, MON_DATA_HP, &no_hp);
            EXPECT_EQ(ReceivePeerMon(id[0], slot, count, &peer),
                      COOP_BATTLE_INBOUND_ACCEPTED);
        }
        if (count < 4)
        {
            EXPECT(!CoopBattleRuntime_PreparePartnerParty(id, local, PARTY_SIZE, &prepared));
            EXPECT_EQ(memcmp(&prepared, &untouched, sizeof(prepared)), 0);
            continue;
        }
        EXPECT(CoopBattleRuntime_PreparePartnerParty(id, local, PARTY_SIZE, &prepared));
        EXPECT_EQ(prepared.peer_slots[0], 0);
        EXPECT_EQ(prepared.peer_slots[1], 2);
        EXPECT_EQ(prepared.peer_slots[2], 3);
        EXPECT_EQ(GetMonData(&prepared.peer[0], MON_DATA_PERSONALITY), 1);
        EXPECT_EQ(GetMonData(&prepared.peer[1], MON_DATA_PERSONALITY), 3);
        EXPECT_EQ(GetMonData(&prepared.peer[2], MON_DATA_PERSONALITY), 4);
        memset(&prepared, 0xA5, sizeof(prepared));
    }
}

TEST("Cloud Coop startup plan maps members and restores sparse player party")
{
    static EWRAM_DATA struct Pokemon local[PARTY_SIZE];
    static EWRAM_DATA struct Pokemon peer;
    static EWRAM_DATA struct Pokemon fought[COOP_BATTLE_MULTI_PARTY_SIZE];
    static EWRAM_DATA struct Pokemon restored[PARTY_SIZE];
    static EWRAM_DATA struct CoopBattleStartupPlan plan;
    static EWRAM_DATA struct CoopBattleStartupPlan untouched;
    struct CoopBattleManifestIdentity identity;
    u8 id[COOP_BATTLE_ID_SIZE] = {33};
    u8 stale[COOP_BATTLE_ID_SIZE] = {34};
    u8 i;
    u16 hp = 1;

    memset(local, 0, sizeof(local));
    for (i = 0; i < COOP_BATTLE_MULTI_PARTY_SIZE; i++)
        CreateUsableTestMon(&local[i * 2], SPECIES_CHARMANDER, 5, i + 1, OTID_STRUCT_PRESET(1));
    CoopBattleRuntime_Init();
    CoopBattleRuntime_OnSessionReady(33);
    ReceiveTrainerManifest(33, COOP_REGION_HOENN,
                           COOP_TRAINER_HOENN_TRAINER_WALLY_1_ORDINAL, 1,
                           local, PARTY_SIZE);
    for (i = 0; i < COOP_BATTLE_MULTI_PARTY_SIZE; i++)
    {
        CreateUsableTestMon(&peer, SPECIES_BULBASAUR, 5, i + 1, OTID_STRUCT_PRESET(1));
        EXPECT_EQ(ReceivePeerMon(33, i, COOP_BATTLE_MULTI_PARTY_SIZE, &peer),
                  COOP_BATTLE_INBOUND_ACCEPTED);
    }
    memset(&plan, 0xA5, sizeof(plan));
    untouched = plan;
    EXPECT(!CoopBattleRuntime_MakeStartupPlan(stale, local, PARTY_SIZE, &plan));
    EXPECT(!CoopBattleRuntime_GetManifestIdentity(stale, &identity));
    EXPECT(CoopBattleRuntime_GetManifestIdentity(id, &identity));
    EXPECT_EQ(identity.kind, COOP_BATTLE_MANIFEST_KIND_TRAINER);
    EXPECT_EQ(identity.trainer_region, COOP_REGION_HOENN);
    EXPECT_EQ(identity.trainer_ordinal, COOP_TRAINER_HOENN_TRAINER_WALLY_1_ORDINAL);
    EXPECT_EQ(identity.local_member_slot, 1);
    EXPECT_EQ(memcmp(&plan, &untouched, sizeof(plan)), 0);
    EXPECT(CoopBattleRuntime_MakeStartupPlan(id, local, PARTY_SIZE, &plan));
    ((u8 *)&local[5])[0]++;
    EXPECT(!CoopBattleRuntime_MakeStartupPlan(id, local, PARTY_SIZE, &plan));
    ((u8 *)&local[5])[0]--;
    EXPECT_EQ(plan.local_member_slot, 1);
    EXPECT_EQ(plan.member_battler_positions[0], B_POSITION_PLAYER_LEFT);
    EXPECT_EQ(plan.member_battler_positions[1], B_POSITION_PLAYER_RIGHT);
    EXPECT_EQ(plan.member_party_trainers[0], B_TRAINER_2);
    EXPECT_EQ(plan.member_party_trainers[1], B_TRAINER_0);
    EXPECT_EQ(plan.opponent_trainer_id, TRAINER_WALLY_VR_1);
    for (i = 0; i < COOP_BATTLE_MULTI_PARTY_SIZE; i++)
    {
        EXPECT_EQ(plan.local_slots[i], i * 2);
        EXPECT_EQ(plan.peer_slots[i], i);
        EXPECT_EQ(memcmp(&plan.staged_local[i], &local[i * 2], sizeof(peer)), 0);
        fought[i] = plan.staged_local[i];
    }
    SetMonData(&fought[1], MON_DATA_HP, &hp);
    memset(restored, 0xA5, sizeof(restored));
    EXPECT(!CoopBattleRuntime_RestoreLocalParty(&plan, stale, fought, restored));
    EXPECT(CoopBattleRuntime_RestoreLocalParty(&plan, id, fought, restored));
    for (i = 0; i < PARTY_SIZE; i++)
    {
        if (i == 2)
            EXPECT_EQ(GetMonData(&restored[i], MON_DATA_HP), hp);
        else
            EXPECT_EQ(memcmp(&restored[i], &local[i], sizeof(peer)), 0);
    }
    EXPECT_EQ(memcmp(local, plan.original_local, sizeof(local)), 0);
    EXPECT(!CoopBattleRuntime_IsEngineActive());
    EXPECT(!CoopBattleRuntime_ArmEngine(stale));
    EXPECT(CoopBattleRuntime_ArmEngine(id));
    EXPECT(!CoopBattleRuntime_ArmEngine(id));
    EXPECT(CoopBattleRuntime_IsEngineActive());
    EXPECT_EQ(CoopBattleRuntime_EngineLocalMemberSlot(), 1);
    EXPECT(!CoopBattleRuntime_IsLocalActionSubmitted());
    EXPECT(!CoopBattleRuntime_IsEngineTurnReady());
    EXPECT(!CoopBattleRuntime_ConfirmPeerAutomaticAction(COOP_BATTLE_ACTION_MOVE));
    CoopBattleRuntime_FailEngine();
    EXPECT(CoopBattleRuntime_IsEngineFaulted());
    {
        u8 abort[COOP_BATTLE_ABORT_SIZE] = {0};
        memcpy(abort, id, COOP_BATTLE_ID_SIZE);
        abort[COOP_BATTLE_ID_SIZE] = COOP_BATTLE_ABORT_DISCONNECTED;
        EXPECT_EQ(CoopBattleRuntime_ReceiveAbort(abort, sizeof(abort)),
                  COOP_BATTLE_INBOUND_ACCEPTED);
        EXPECT(CoopBattleRuntime_IsEngineActive());
        EXPECT(!CoopBattleRuntime_IsEngineTurnReady());
    }
    CoopBattleRuntime_DisarmEngine();
    EXPECT(!CoopBattleRuntime_IsEngineActive());
    EXPECT(!CoopBattleRuntime_IsEngineFaulted());
}

TEST("Cloud Coop pending abort survives terminal clear within the same epoch")
{
    struct CoopBridgeMessage message;
    u8 abort[COOP_BATTLE_ABORT_REQUEST_SIZE] = {0};

    CoopBattleRuntime_Init();
    CoopBridgeQueue_Init(&gCoopNetBridge.game_to_network);
    CoopBattleRuntime_OnSessionReady(42);
    ReceiveManifest(7);
    abort[0] = 7;
    abort[16] = COOP_BATTLE_ABORT_CANCELED;
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_BATTLE_ABORT_REQUEST,
                                  2, 42, abort, sizeof(abort)));
    EXPECT(CoopBridgeQueue_Push(&gCoopNetBridge.game_to_network, &message));
    CoopBattleRuntime_PreserveOutbound();
    EXPECT(CoopBattleRuntime_HasPendingOutboundReplay());
    EXPECT_EQ(CoopBattleRuntime_ReceiveAbort(abort, sizeof(abort)), COOP_BATTLE_INBOUND_ACCEPTED);
    EXPECT(CoopBattleRuntime_HasPendingOutboundReplay());
    CoopBattleRuntime_OnSessionReady(43);
    EXPECT(!CoopBattleRuntime_HasPendingOutboundReplay());
}

TEST("Cloud Coop abort is retained when battle cleanup loses transport")
{
    u8 abort[COOP_BATTLE_ABORT_SIZE] = {0};

    CoopBattleRuntime_Init();
    CoopBridgeQueue_Init(&gCoopNetBridge.game_to_network);
    CoopBattleRuntime_OnSessionReady(42);
    ReceiveManifest(7);
    CoopBattleRuntime_OnTransportLost();
    EXPECT(CoopBattleRuntime_RequestAbort(COOP_BATTLE_ABORT_CANCELED));
    EXPECT(CoopBattleRuntime_HasPendingOutboundReplay());
    CoopBattleRuntime_OnSessionReady(42);
    abort[0] = 7;
    abort[16] = COOP_BATTLE_ABORT_DISCONNECTED;
    EXPECT_EQ(CoopBattleRuntime_ReceiveAbort(abort, sizeof(abort)),
              COOP_BATTLE_INBOUND_ACCEPTED);
    EXPECT(CoopBattleRuntime_HasPendingOutboundReplay());
    EXPECT(CoopBattleRuntime_HasPendingOutboundReplay());
}

TEST("Cloud Coop startup plan accepts catalogued Kanto Brock")
{
    struct Pokemon local[PARTY_SIZE] = {0};
    struct Pokemon peer;
    struct CoopBattleStartupPlan plan;
    u8 id[COOP_BATTLE_ID_SIZE] = {35};
    u8 i;

    CoopBattleRuntime_Init();
    CoopBattleRuntime_OnSessionReady(35);
    for (i = 0; i < COOP_BATTLE_MULTI_PARTY_SIZE; i++)
        CreateUsableTestMon(&local[i], SPECIES_CHARMANDER, 5, i + 1, OTID_STRUCT_PRESET(1));
    ReceiveTrainerManifest(35, COOP_REGION_KANTO,
                           COOP_TRAINER_KANTO_TRAINER_BROCK_ORDINAL, 0,
                           local, PARTY_SIZE);
    for (i = 0; i < COOP_BATTLE_MULTI_PARTY_SIZE; i++)
    {
        CreateUsableTestMon(&peer, SPECIES_BULBASAUR, 5, i + 1, OTID_STRUCT_PRESET(1));
        EXPECT_EQ(ReceivePeerMon(35, i, COOP_BATTLE_MULTI_PARTY_SIZE, &peer),
                  COOP_BATTLE_INBOUND_ACCEPTED);
    }
    EXPECT(CoopBattleRuntime_MakeStartupPlan(id, local, PARTY_SIZE, &plan));
    EXPECT_EQ(plan.opponent_trainer_id, TRAINER_LEADER_BROCK);
    EXPECT_EQ(plan.local_member_slot, 0);
    EXPECT_EQ(plan.member_party_trainers[0], B_TRAINER_0);
    EXPECT_EQ(plan.member_party_trainers[1], B_TRAINER_2);
}
