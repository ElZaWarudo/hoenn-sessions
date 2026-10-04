#include "global.h"
#include "battle.h"
#include "battle_main.h"
#include "battle_util.h"
#include "coop/battle_items.h"
#include "coop/battle_runtime.h"
#include "coop/generated_regional_identities.h"
#include "coop/net_bridge.h"
#include "coop/region.h"
#include "fieldmap.h"
#include "item.h"
#include "main.h"
#include "malloc.h"
#include "pokemon.h"
#include "random.h"
#include "constants/battle.h"
#include "constants/items.h"
#include "constants/moves.h"
#include "constants/region_map_sections.h"
#include "test/battle.h"

/* Bag items in co-op trainer battles. Both ROMs are simulated as in the
 * other co-op tests: each member's view arms the engine from its own
 * manifest and stages its own party at B_TRAINER_0 / PLAYER_LEFT and the
 * partner's at B_TRAINER_2 / PLAYER_RIGHT. */

/* The native commands the item battle scripts run (battle_script_commands.c). */
void BS_ItemRestoreHP(void);
void BS_ItemCureStatus(void);
void BS_ItemIncreaseStat(void);
void BS_ItemRestorePP(void);

#define MEMBER_MONS 3
#define ITEMS_BATTLE_ID 71

static void SetItemsRegion(void)
{
    gMapHeader.engineRegion = COOP_MAP_ENGINE_REGION_HOENN;
    gMapHeader.regionMapSectionId = MAPSEC_ROUTE_102;
}

static void CreateMemberMon(struct Pokemon *mon, enum Species species, u8 level, u32 personality)
{
    CreateMon(mon, species, level, personality, OTID_STRUCT_PRESET(1));
    CalculateMonStats(mon);
}

/* Each member: a lead, a benched mon and a third one. */
static void BuildMembers(struct Pokemon *member0, struct Pokemon *member1)
{
    CreateMemberMon(&member0[0], SPECIES_TORCHIC, 12, 101);
    CreateMemberMon(&member0[1], SPECIES_WINGULL, 14, 102);
    CreateMemberMon(&member0[2], SPECIES_RALTS, 9, 103);
    CreateMemberMon(&member1[0], SPECIES_MUDKIP, 13, 201);
    CreateMemberMon(&member1[1], SPECIES_ZIGZAGOON, 11, 202);
    CreateMemberMon(&member1[2], SPECIES_SEEDOT, 10, 203);
    /* The benched mons know one move (for the PP item). */
    SetMonMoveSlot(&member0[1], MOVE_GUST, 0);
    SetMonMoveSlot(&member1[1], MOVE_TACKLE, 0);
}

/* A few turns in: poisoned leads, a hurt benched mon with a spent move, a
 * fainted third mon. The same for both members. */
static void WearMembers(struct Pokemon *member0, struct Pokemon *member1)
{
    struct Pokemon *members[2] = {member0, member1};
    u32 status = STATUS1_POISON;
    u16 hp = 5;
    u8 pp = 1;
    u8 i;

    for (i = 0; i < 2; i++)
    {
        SetMonData(&members[i][0], MON_DATA_STATUS, &status);
        SetMonData(&members[i][1], MON_DATA_HP, &hp);
        SetMonData(&members[i][1], MON_DATA_PP1, &pp);
        hp = 0;
        SetMonData(&members[i][2], MON_DATA_HP, &hp);
        hp = 5;
    }
}

static void ReceiveItemsManifest(u8 slot, const struct Pokemon *local)
{
    u8 manifest[COOP_BATTLE_MANIFEST_SIZE] = {0};

    manifest[0] = ITEMS_BATTLE_ID;
    manifest[COOP_BATTLE_MANIFEST_KIND_OFFSET] = COOP_BATTLE_MANIFEST_KIND_TRAINER;
    manifest[COOP_BATTLE_MANIFEST_MEMBER_SLOT_OFFSET] = slot;
    manifest[COOP_BATTLE_MANIFEST_REGION_OFFSET] = COOP_REGION_HOENN;
    manifest[COOP_BATTLE_MANIFEST_TRAINER_ORDINAL_OFFSET] = (u8)COOP_TRAINER_HOENN_TRAINER_CALVIN_1_ORDINAL;
    manifest[COOP_BATTLE_MANIFEST_TRAINER_ORDINAL_OFFSET + 1] = COOP_TRAINER_HOENN_TRAINER_CALVIN_1_ORDINAL >> 8;
    EXPECT(CoopBattleRuntime_ComputePartyDigest(local, MEMBER_MONS,
                                                 &manifest[50 + slot * COOP_BATTLE_DIGEST_SIZE],
                                                 COOP_BATTLE_DIGEST_SIZE));
    EXPECT_EQ(CoopBattleRuntime_ReceiveManifest(manifest, sizeof(manifest)),
              COOP_BATTLE_INBOUND_ACCEPTED);
}

/* One member's ROM with the trainer engine armed (the pre-battle records
 * are the snapshots the members exchanged). */
static void ArmTrainerView(u8 slot, const struct Pokemon *member0, const struct Pokemon *member1)
{
    const struct Pokemon *peer = slot == 0 ? member1 : member0;
    u8 chunk[COOP_BATTLE_PARTY_SNAPSHOT_SIZE];
    u8 id[COOP_BATTLE_ID_SIZE] = {ITEMS_BATTLE_ID};
    u8 i;

    SetItemsRegion();
    CoopBattleRuntime_Init();
    CoopBattleRuntime_OnSessionReady(ITEMS_BATTLE_ID);
    ReceiveItemsManifest(slot, slot == 0 ? member0 : member1);
    for (i = 0; i < MEMBER_MONS; i++)
    {
        memset(chunk, 0, sizeof(chunk));
        chunk[0] = ITEMS_BATTLE_ID;
        chunk[16] = i;
        chunk[17] = i;
        chunk[18] = MEMBER_MONS;
        chunk[19] = COOP_BATTLE_PARTY_MON_SIZE;
        memcpy(chunk + 20, &peer[i], sizeof(peer[i]));
        EXPECT_EQ(CoopBattleRuntime_ReceivePeerPartyChunk(chunk, sizeof(chunk)),
                  COOP_BATTLE_INBOUND_ACCEPTED);
    }
    EXPECT(CoopBattleRuntime_ArmEngine(id));
    CoopBattleItems_Begin();
}

/* Both ROMs run the battle from the same seed. */
static rng_value_t sViewRng;
static rng_value_t sViewRng2;

/* The battle state one member's ROM holds: its own mons at B_TRAINER_0 and
 * battler 0, the partner's at B_TRAINER_2 and battler 2. */
static void StageTrainerView(u8 slot, struct Pokemon *member0, struct Pokemon *member1)
{
    struct Pokemon *local = slot == 0 ? member0 : member1;
    struct Pokemon *peer = slot == 0 ? member1 : member0;
    u8 i;

    for (i = 0; i < PARTY_SIZE; i++)
    {
        ZeroMonData(&gParties[B_TRAINER_0][i]);
        ZeroMonData(&gParties[B_TRAINER_1][i]);
        ZeroMonData(&gParties[B_TRAINER_2][i]);
        ZeroMonData(&gParties[B_TRAINER_3][i]);
    }
    for (i = 0; i < MEMBER_MONS; i++)
    {
        gParties[B_TRAINER_0][i] = local[i];
        gParties[B_TRAINER_2][i] = peer[i];
    }
    gPartiesCount[B_TRAINER_0] = MEMBER_MONS;
    gPartiesCount[B_TRAINER_1] = 0;
    gPartiesCount[B_TRAINER_2] = MEMBER_MONS;
    gPartiesCount[B_TRAINER_3] = 0;
    gBattleTypeFlags = BATTLE_TYPE_MULTI | BATTLE_TYPE_INGAME_PARTNER
        | BATTLE_TYPE_DOUBLE | BATTLE_TYPE_TRAINER;
    gBattlersCount = MAX_BATTLERS_COUNT;
    for (i = 0; i < MAX_BATTLERS_COUNT; i++)
    {
        gBattlerPositions[i] = i;
        gBattlerPartyIndexes[i] = 0;
    }
    memset(gBattleMons, 0, sizeof(gBattleMons));
    PokemonToBattleMon(&gParties[B_TRAINER_0][0], &gBattleMons[0]);
    PokemonToBattleMon(&gParties[B_TRAINER_2][0], &gBattleMons[2]);
    memset(gBattleStruct, 0, sizeof(*gBattleStruct));
    memset(gBattleResources->bufferB, 0, sizeof(gBattleResources->bufferB));
    memset(gProtectStructs, 0, sizeof(gProtectStructs));
    memset(gSpecialStatuses, 0, sizeof(gSpecialStatuses));
    memset(gSideStatuses, 0, sizeof(gSideStatuses));
    memset(gSideTimers, 0, sizeof(gSideTimers));
    memset(gLastMoves, 0, sizeof(gLastMoves));
    gAbsentBattlerFlags = 0;
    gBattleOutcome = 0;
    gHitMarker = 0;
    gRngValue = sViewRng;
    gRng2Value = sViewRng2;
}

static bool8 PeekActionIntent(void)
{
    struct CoopBridgeMessage message;
    bool8 found = FALSE;

    while (CoopNetBridge_DequeueGameToNetwork(&message))
        if (message.type == COOP_BRIDGE_MESSAGE_ACTION_INTENT)
            found = TRUE;
    return found;
}

TEST("Cloud Coop item action encodes item, party slot and move slot and rejects invalid items")
{
    struct CoopBattleAction potion = {COOP_BATTLE_ACTION_ITEM, 2, 0, ITEM_POTION};
    struct CoopBattleAction ether = {COOP_BATTLE_ACTION_ITEM, 1, 3, ITEM_ETHER};
    struct CoopBattleAction bad;
    struct CoopBattleAction decoded;
    u8 bytes[COOP_BATTLE_ACTION_SIZE];
    static const u16 refused[] = {
        ITEM_NONE, ITEM_POKE_BALL, ITEM_POKE_DOLL, ITEM_ESCAPE_ROPE,
        ITEM_POKE_FLUTE, ITEM_ENIGMA_BERRY_E_READER, ITEMS_COUNT,
    };
    u8 i;

    /* Four bytes: kind, item (u16 LE), slot | move << 4. No protocol bump. */
    EXPECT(CoopBattleRuntime_EncodeAction(&potion, bytes, sizeof(bytes)));
    EXPECT_EQ(bytes[0], COOP_BATTLE_ACTION_ITEM);
    EXPECT_EQ(bytes[1], ITEM_POTION & 0xFF);
    EXPECT_EQ(bytes[2], ITEM_POTION >> 8);
    EXPECT_EQ(bytes[3], 2);
    EXPECT(CoopBattleRuntime_DecodeAction(bytes, sizeof(bytes), &decoded));
    EXPECT_EQ(decoded.kind, COOP_BATTLE_ACTION_ITEM);
    EXPECT_EQ(decoded.item, ITEM_POTION);
    EXPECT_EQ(decoded.index, 2);
    EXPECT_EQ(decoded.target, 0);
    EXPECT(CoopBattleRuntime_EncodeAction(&ether, bytes, sizeof(bytes)));
    EXPECT_EQ(bytes[3], 0x31);
    EXPECT(CoopBattleRuntime_DecodeAction(bytes, sizeof(bytes), &decoded));
    EXPECT_EQ(decoded.item, ITEM_ETHER);
    EXPECT_EQ(decoded.index, 1);
    EXPECT_EQ(decoded.target, 3);

    /* Balls, escape items, field-only items, the Poke Flute and the
     * e-Reader berry are never an item action. */
    for (i = 0; i < ARRAY_COUNT(refused); i++)
    {
        bad = potion;
        bad.item = refused[i];
        EXPECT(!CoopBattleRuntime_IsSharedBattleItem(refused[i]));
        EXPECT(!CoopBattleRuntime_EncodeAction(&bad, bytes, sizeof(bytes)));
        bytes[0] = COOP_BATTLE_ACTION_ITEM;
        bytes[1] = refused[i] & 0xFF;
        bytes[2] = refused[i] >> 8;
        bytes[3] = 0;
        EXPECT(!CoopBattleRuntime_DecodeAction(bytes, sizeof(bytes), &decoded));
    }
    EXPECT(CoopBattleRuntime_IsSharedBattleItem(ITEM_FULL_RESTORE));
    EXPECT(CoopBattleRuntime_IsSharedBattleItem(ITEM_X_ATTACK));
    EXPECT(CoopBattleRuntime_IsSharedBattleItem(ITEM_GUARD_SPEC));
    EXPECT(CoopBattleRuntime_IsSharedBattleItem(ITEM_MAX_REVIVE));

    /* A trainer battle side holds at most three Pokemon; only a one-move
     * PP item names a move, and only moves 0-3. */
    bytes[0] = COOP_BATTLE_ACTION_ITEM;
    bytes[1] = ITEM_POTION & 0xFF;
    bytes[2] = ITEM_POTION >> 8;
    bytes[3] = COOP_BATTLE_MULTI_PARTY_SIZE;
    EXPECT(!CoopBattleRuntime_DecodeAction(bytes, sizeof(bytes), &decoded));
    bytes[3] = 0x10;
    EXPECT(!CoopBattleRuntime_DecodeAction(bytes, sizeof(bytes), &decoded));
    bytes[1] = ITEM_ETHER & 0xFF;
    bytes[2] = ITEM_ETHER >> 8;
    bytes[3] = 0x30;
    EXPECT(CoopBattleRuntime_DecodeAction(bytes, sizeof(bytes), &decoded));
    bytes[3] = 0x40;
    EXPECT(!CoopBattleRuntime_DecodeAction(bytes, sizeof(bytes), &decoded));
    bad = potion;
    bad.target = 1;
    EXPECT(!CoopBattleRuntime_EncodeAction(&bad, bytes, sizeof(bytes)));
    bad = potion;
    bad.index = COOP_BATTLE_MULTI_PARTY_SIZE;
    EXPECT(!CoopBattleRuntime_EncodeAction(&bad, bytes, sizeof(bytes)));

    /* The other kinds still carry a zero fourth byte and no item. */
    bytes[0] = COOP_BATTLE_ACTION_SWITCH;
    bytes[1] = 1;
    bytes[2] = 0;
    bytes[3] = 0;
    EXPECT(CoopBattleRuntime_DecodeAction(bytes, sizeof(bytes), &decoded));
    EXPECT_EQ(decoded.item, ITEM_NONE);
    bytes[3] = 2;
    EXPECT(!CoopBattleRuntime_DecodeAction(bytes, sizeof(bytes), &decoded));
}

TEST("Cloud Coop friendly battles keep refusing item actions")
{
    struct Pokemon team[1], other[1];
    struct CoopBattleAction item = {COOP_BATTLE_ACTION_ITEM, 0, 0, ITEM_POTION};
    struct CoopBattleAction move = {COOP_BATTLE_ACTION_MOVE, 0, B_POSITION_OPPONENT_LEFT, ITEM_NONE};
    u8 manifest[COOP_BATTLE_MANIFEST_SIZE] = {0};
    u8 chunk[COOP_BATTLE_PARTY_SNAPSHOT_SIZE] = {0};
    u8 bundle[20 + 2 * COOP_BATTLE_ACTION_SIZE] = {0};
    u8 id[COOP_BATTLE_ID_SIZE] = {72};

    CreateMemberMon(&team[0], SPECIES_TORCHIC, 12, 1);
    CreateMemberMon(&other[0], SPECIES_MUDKIP, 13, 2);
    CoopBattleRuntime_Init();
    CoopBridgeQueue_Init(&gCoopNetBridge.game_to_network);
    CoopBattleRuntime_OnSessionReady(72);
    manifest[0] = 72;
    manifest[COOP_BATTLE_MANIFEST_KIND_OFFSET] = COOP_BATTLE_MANIFEST_KIND_FRIENDLY;
    manifest[COOP_BATTLE_MANIFEST_RULES_OFFSET] = COOP_BATTLE_FRIENDLY_SINGLES;
    manifest[COOP_BATTLE_MANIFEST_RULES_OFFSET + 2] = 1;
    EXPECT(CoopBattleRuntime_ComputePartyDigest(team, 1, &manifest[50], COOP_BATTLE_DIGEST_SIZE));
    EXPECT(CoopBattleRuntime_ComputePartyDigest(other, 1, &manifest[82], COOP_BATTLE_DIGEST_SIZE));
    EXPECT_EQ(CoopBattleRuntime_ReceiveManifest(manifest, sizeof(manifest)), COOP_BATTLE_INBOUND_ACCEPTED);
    chunk[0] = 72;
    chunk[18] = 1;
    chunk[19] = COOP_BATTLE_PARTY_MON_SIZE;
    memcpy(chunk + 20, &other[0], sizeof(other[0]));
    EXPECT_EQ(CoopBattleRuntime_ReceivePeerPartyChunk(chunk, sizeof(chunk)), COOP_BATTLE_INBOUND_ACCEPTED);
    EXPECT(CoopBattleRuntime_ArmEngine(id));
    EXPECT(CoopBattleRuntime_IsFriendlyEngine());
    /* The bag stays shut: no co-op item path, no item action sent. */
    EXPECT(!CoopBattleItems_IsBagOpen());
    EXPECT(!CoopBattleRuntime_SubmitLocalActions(&item, 1));
    EXPECT(!CoopBattleRuntime_IsLocalActionSubmitted());
    EXPECT(!PeekActionIntent());
    CoopBattleRuntime_DisarmEngine();

    /* A peer's item action in a friendly bundle is malformed; the same
     * bundle with a move is merely stale here. */
    CoopBattleRuntime_OnTransportLost();
    bundle[0] = 72;
    bundle[16] = 1;
    bundle[18] = COOP_BATTLE_ACTION_SIZE;
    bundle[19] = COOP_BATTLE_ACTION_SIZE;
    EXPECT(CoopBattleRuntime_EncodeAction(&move, bundle + 20, COOP_BATTLE_ACTION_SIZE));
    EXPECT(CoopBattleRuntime_EncodeAction(&item, bundle + 24, COOP_BATTLE_ACTION_SIZE));
    EXPECT_EQ(CoopBattleRuntime_ReceiveTurnBundle(bundle, sizeof(bundle)), COOP_BATTLE_INBOUND_MALFORMED);
    EXPECT(CoopBattleRuntime_EncodeAction(&move, bundle + 24, COOP_BATTLE_ACTION_SIZE));
    EXPECT_EQ(CoopBattleRuntime_ReceiveTurnBundle(bundle, sizeof(bundle)), COOP_BATTLE_INBOUND_IGNORED);
    CoopBattleRuntime_Init();
}

struct ItemCase
{
    u16 item;
    u8 slot;
    u8 move;
    void (*native)(void);
};

static const struct ItemCase sItemCases[] =
{
    {ITEM_POTION, 1, 0, BS_ItemRestoreHP},     // the benched, hurt mon
    {ITEM_ANTIDOTE, 0, 0, BS_ItemCureStatus},  // the poisoned lead
    {ITEM_X_ATTACK, 0, 0, BS_ItemIncreaseStat},
    {ITEM_REVIVE, 2, 0, BS_ItemRestoreHP},     // the fainted third mon
    {ITEM_ETHER, 1, 0, BS_ItemRestorePP},      // the benched mon's first move
};

static void CheckItemEffect(const struct ItemCase *c, enum BattlerId battler)
{
    struct Pokemon *mon = &GetBattlerParty(battler)[c->slot];

    switch (c->item)
    {
    case ITEM_POTION:
        EXPECT_EQ(GetMonData(mon, MON_DATA_HP), 5 + GetItemHoldEffectParam(ITEM_POTION));
        break;
    case ITEM_ANTIDOTE:
        EXPECT_EQ(GetMonData(mon, MON_DATA_STATUS), STATUS1_NONE);
        EXPECT_EQ(gBattleMons[battler].status1, STATUS1_NONE);
        /* The partner's poisoned lead is left alone. */
        EXPECT_EQ(gBattleMons[BATTLE_PARTNER(battler)].status1, STATUS1_POISON);
        break;
    case ITEM_X_ATTACK:
        /* The stat change applies to the acting member's own battler. */
        EXPECT_EQ(gBattlerAttacker, battler);
        EXPECT_EQ(gBattleScripting.statChanger & 0x7, STAT_ATK);
        EXPECT_EQ(gBattleScripting.statChanger >> 3 & 0xF, GetItemHoldEffectParam(ITEM_X_ATTACK));
        /* The script's statbuffchange (run by the engine in the multi
         * battle tests below) raises that battler's stage. */
        gBattleMons[gBattlerAttacker].statStages[STAT_ATK] += GetItemHoldEffectParam(ITEM_X_ATTACK);
        break;
    case ITEM_REVIVE:
        EXPECT_EQ(GetMonData(mon, MON_DATA_HP), GetMonData(mon, MON_DATA_MAX_HP) / 2);
        break;
    case ITEM_ETHER:
        EXPECT_GT(GetMonData(mon, MON_DATA_PP1), 1);
        break;
    }
}

TEST("Cloud Coop item actions run identically on both ROMs and hash one battle")
{
    struct Pokemon base0[MEMBER_MONS], base1[MEMBER_MONS];
    struct Pokemon member0[MEMBER_MONS], member1[MEMBER_MONS];
    struct BattleStruct *savedBattleStruct = gBattleStruct;
    struct BattleResources *savedResources = gBattleResources;
    bool8 savedInBattle = gMain.inBattle;
    struct CoopBattleAction action, decoded;
    u8 bytes[COOP_BATTLE_ACTION_SIZE];
    u8 digests[2][COOP_BATTLE_DIGEST_SIZE];
    u8 before[COOP_BATTLE_DIGEST_SIZE];
    u8 scratch[16] = {0};
    u16 held[ARRAY_COUNT(sItemCases)];
    u16 bagBefore;
    u8 c, acting, n, view;
    enum BattlerId battler;

    gBattleStruct = AllocZeroed(sizeof(*gBattleStruct));
    gBattleResources = AllocZeroed(sizeof(*gBattleResources));
    gMain.inBattle = TRUE;
    BuildMembers(base0, base1);
    sViewRng = gRngValue;
    sViewRng2 = gRng2Value;
    for (c = 0; c < ARRAY_COUNT(sItemCases); c++)
    {
        held[c] = CountTotalItemQuantityInBag(sItemCases[c].item);
        EXPECT(AddBagItem(sItemCases[c].item, 1));
    }
    for (c = 0; c < ARRAY_COUNT(sItemCases); c++)
    {
        for (acting = 0; acting < 2; acting++)
        {
            /* The acting member's ROM first: its choice becomes the action
             * that the lockstep bundle hands the partner's ROM. */
            for (n = 0; n < 2; n++)
            {
                view = n == 0 ? acting : acting ^ 1;
                ArmTrainerView(view, base0, base1);
                memcpy(member0, base0, sizeof(member0));
                memcpy(member1, base1, sizeof(member1));
                WearMembers(member0, member1);
                StageTrainerView(view, member0, member1);
                battler = view == acting ? 0 : 2;
                if (view == acting)
                {
                    EXPECT(CoopBattleItems_IsBagOpen());
                    EXPECT_EQ(CoopBattleItems_RefuseItem(sItemCases[c].item), NULL);
                    /* The engine's STATE_WAIT_ACTION_CASE_CHOSEN buffer and the
                     * Pokemon the party menu picked; nothing is used yet. */
                    bagBefore = CountTotalItemQuantityInBag(sItemCases[c].item);
                    gBattleResources->bufferB[0][1] = sItemCases[c].item & 0xFF;
                    gBattleResources->bufferB[0][2] = sItemCases[c].item >> 8;
                    CoopBattleItems_ChooseTarget(&gParties[B_TRAINER_0][sItemCases[c].slot]);
                    gBattleStruct->itemMoveIndex[0] = sItemCases[c].move;
                    EXPECT(CoopBattleItems_MakeAction(0, &action));
                    EXPECT_EQ(action.kind, COOP_BATTLE_ACTION_ITEM);
                    EXPECT_EQ(action.item, sItemCases[c].item);
                    EXPECT_EQ(action.index, sItemCases[c].slot);
                    EXPECT_EQ(action.target, sItemCases[c].move);
                    EXPECT(CoopBattleRuntime_EncodeAction(&action, bytes, sizeof(bytes)));
                    EXPECT_EQ(CountTotalItemQuantityInBag(sItemCases[c].item), bagBefore);
                }
                else
                {
                    /* The partner controller binds the decoded action and
                     * returns the item to the engine. */
                    EXPECT(CoopBattleRuntime_DecodeAction(bytes, sizeof(bytes), &decoded));
                    EXPECT(CoopBattleItems_BindAction(2, &decoded));
                    gBattleResources->bufferB[2][1] = decoded.item & 0xFF;
                    gBattleResources->bufferB[2][2] = decoded.item >> 8;
                }
                EXPECT_EQ(gBattleStruct->itemPartyIndex[battler], sItemCases[c].slot);
                EXPECT_EQ(gBattleStruct->itemMoveIndex[battler], sItemCases[c].move);
                EXPECT(CoopBattleRuntime_ComputeBattleDigest(before, sizeof(before)));

                /* The resolved turn: the vanilla item action and the item
                 * script's own effect command, on the same canonical mon. */
                bagBefore = CountTotalItemQuantityInBag(sItemCases[c].item);
                gCurrentTurnActionNumber = 0;
                gBattlerByTurnOrder[0] = battler;
                HandleAction_UseItem();
                EXPECT_EQ(gLastUsedItem, sItemCases[c].item);
                EXPECT_EQ(gBattlerAttacker, battler);
                /* Only the acting ROM's bag gives up the item, once. */
                EXPECT_EQ(CountTotalItemQuantityInBag(sItemCases[c].item),
                          bagBefore - (view == acting ? 1 : 0));
                gBattlescriptCurrInstr = scratch;
                sItemCases[c].native();
                CheckItemEffect(&sItemCases[c], battler);
                EXPECT(CoopBattleRuntime_ComputeBattleDigest(digests[n], sizeof(digests[n])));
                EXPECT_NE(memcmp(before, digests[n], sizeof(before)), 0);
                /* The acting ROM's battle ends unfinished: the item returns. */
                CoopBattleItems_Settle(FALSE);
                EXPECT_EQ(CountTotalItemQuantityInBag(sItemCases[c].item), bagBefore);
                CoopBattleRuntime_DisarmEngine();
            }
            EXPECT_EQ(memcmp(digests[0], digests[1], sizeof(digests[0])), 0);
        }
    }
    for (c = 0; c < ARRAY_COUNT(sItemCases); c++)
    {
        EXPECT(RemoveBagItem(sItemCases[c].item, 1));
        EXPECT_EQ(CountTotalItemQuantityInBag(sItemCases[c].item), held[c]);
    }
    gMain.inBattle = savedInBattle;
    gBattleControllerExecFlags = 0;
    Free(gBattleResources);
    Free(gBattleStruct);
    gBattleResources = savedResources;
    gBattleStruct = savedBattleStruct;
    CoopBattleRuntime_Init();
}

TEST("Cloud Coop item removal follows the party: kept on a completed battle, returned otherwise")
{
    struct BattleStruct *savedBattleStruct = gBattleStruct;
    u16 heldPotion = CountTotalItemQuantityInBag(ITEM_POTION);
    u16 heldFlute = CountTotalItemQuantityInBag(ITEM_BLUE_FLUTE);

    gBattleStruct = AllocZeroed(sizeof(*gBattleStruct));
    EXPECT(AddBagItem(ITEM_POTION, 3));
    EXPECT(AddBagItem(ITEM_BLUE_FLUTE, 1));
    {
        struct Pokemon member0[MEMBER_MONS], member1[MEMBER_MONS];
        struct BattleResources *savedResources = gBattleResources;

        gBattleResources = AllocZeroed(sizeof(*gBattleResources));
        BuildMembers(member0, member1);
        ArmTrainerView(1, member0, member1);
        StageTrainerView(1, member0, member1);
        Free(gBattleResources);
        gBattleResources = savedResources;
    }
    /* The acting battler is always this ROM's PLAYER_LEFT; the partner's
     * item at PLAYER_RIGHT never touches this bag. */
    CoopBattleItems_OnItemUsed(2, ITEM_POTION);
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_POTION), heldPotion + 3);
    CoopBattleItems_OnItemUsed(0, ITEM_POTION);
    CoopBattleItems_OnItemUsed(0, ITEM_POTION);
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_POTION), heldPotion + 1);
    /* A flute is not consumed. */
    EXPECT(!CoopBattleItems_IsConsumed(ITEM_BLUE_FLUTE));
    CoopBattleItems_OnItemUsed(0, ITEM_BLUE_FLUTE);
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_BLUE_FLUTE), heldFlute + 1);
    /* Completed: the uses stand, like the damage. */
    CoopBattleItems_Settle(TRUE);
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_POTION), heldPotion + 1);
    CoopBattleItems_Settle(FALSE);
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_POTION), heldPotion + 1);
    /* Not completed (abort, desync, no contest): every use is returned. */
    CoopBattleItems_Begin();
    CoopBattleItems_OnItemUsed(0, ITEM_POTION);
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_POTION), heldPotion);
    CoopBattleItems_Settle(FALSE);
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_POTION), heldPotion + 1);
    CoopBattleRuntime_DisarmEngine();
    /* No engine: the vanilla bag is untouched by this path. */
    CoopBattleItems_OnItemUsed(0, ITEM_POTION);
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_POTION), heldPotion + 1);
    EXPECT(RemoveBagItem(ITEM_POTION, 1));
    EXPECT(RemoveBagItem(ITEM_BLUE_FLUTE, 1));
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_POTION), heldPotion);
    EXPECT_EQ(CountTotalItemQuantityInBag(ITEM_BLUE_FLUTE), heldFlute);
    Free(gBattleStruct);
    gBattleStruct = savedBattleStruct;
    CoopBattleRuntime_Init();
}

TEST("Cloud Coop refused items and targets send nothing")
{
    struct Pokemon member0[MEMBER_MONS], member1[MEMBER_MONS];
    struct BattleStruct *savedBattleStruct = gBattleStruct;
    struct BattleResources *savedResources = gBattleResources;
    struct CoopBattleAction action;
    static const u16 kinds[COOP_BATTLE_ITEM_LEDGER_SIZE] = {
        ITEM_POTION, ITEM_SUPER_POTION, ITEM_HYPER_POTION,
        ITEM_ANTIDOTE, ITEM_FULL_HEAL, ITEM_X_DEFENSE,
    };
    u16 held[COOP_BATTLE_ITEM_LEDGER_SIZE];
    u8 i;

    gBattleStruct = AllocZeroed(sizeof(*gBattleStruct));
    gBattleResources = AllocZeroed(sizeof(*gBattleResources));
    CoopBridgeQueue_Init(&gCoopNetBridge.game_to_network);
    BuildMembers(member0, member1);
    ArmTrainerView(0, member0, member1);
    WearMembers(member0, member1);
    StageTrainerView(0, member0, member1);

    /* The bag's "Use" gate: balls (a trainer battle), escape and field-only
     * items, the Poke Flute and the e-Reader berry. */
    EXPECT_NE(CoopBattleItems_RefuseItem(ITEM_POKE_BALL), NULL);
    EXPECT_NE(CoopBattleItems_RefuseItem(ITEM_POKE_DOLL), NULL);
    EXPECT_NE(CoopBattleItems_RefuseItem(ITEM_ESCAPE_ROPE), NULL);
    EXPECT_NE(CoopBattleItems_RefuseItem(ITEM_POKE_FLUTE), NULL);
    EXPECT_NE(CoopBattleItems_RefuseItem(ITEM_ENIGMA_BERRY_E_READER), NULL);
    EXPECT_EQ(CoopBattleItems_RefuseItem(ITEM_REVIVE), NULL);

    /* The party menu gate: the partner's Pokemon (its fainted one too). */
    EXPECT_NE(CoopBattleItems_RefuseTarget(gParties[B_TRAINER_2]), NULL);
    EXPECT_EQ(CoopBattleItems_RefuseTarget(gParties[B_TRAINER_0]), NULL);
    /* Past the gates, a target outside this member's own party never
     * becomes an action. */
    gBattleResources->bufferB[0][1] = ITEM_REVIVE & 0xFF;
    gBattleResources->bufferB[0][2] = ITEM_REVIVE >> 8;
    CoopBattleItems_ChooseTarget(&gParties[B_TRAINER_2][2]);
    EXPECT(!CoopBattleItems_MakeAction(0, &action));
    /* An X item only acts on the member's own battler, not a benched mon. */
    gBattleResources->bufferB[0][1] = ITEM_X_ATTACK & 0xFF;
    gBattleResources->bufferB[0][2] = ITEM_X_ATTACK >> 8;
    CoopBattleItems_ChooseTarget(&gParties[B_TRAINER_0][1]);
    EXPECT(!CoopBattleItems_MakeAction(0, &action));
    /* The partner's ROM refuses an action naming an empty record. */
    action = (struct CoopBattleAction){COOP_BATTLE_ACTION_ITEM, 2, 0, ITEM_POTION};
    ZeroMonData(&gParties[B_TRAINER_2][2]);
    EXPECT(!CoopBattleItems_BindAction(2, &action));

    /* Six kinds can be tracked for a refund; a seventh is refused up front,
     * an already tracked kind or an unconsumed flute is not. */
    for (i = 0; i < COOP_BATTLE_ITEM_LEDGER_SIZE; i++)
    {
        held[i] = CountTotalItemQuantityInBag(kinds[i]);
        EXPECT(AddBagItem(kinds[i], 1));
        CoopBattleItems_OnItemUsed(0, kinds[i]);
    }
    EXPECT_NE(CoopBattleItems_RefuseItem(ITEM_X_SPEED), NULL);
    EXPECT_EQ(CoopBattleItems_RefuseItem(ITEM_POTION), NULL);
    EXPECT_EQ(CoopBattleItems_RefuseItem(ITEM_BLUE_FLUTE), NULL);
    CoopBattleItems_Settle(FALSE);
    for (i = 0; i < COOP_BATTLE_ITEM_LEDGER_SIZE; i++)
    {
        EXPECT(RemoveBagItem(kinds[i], 1));
        EXPECT_EQ(CountTotalItemQuantityInBag(kinds[i]), held[i]);
    }
    EXPECT(!CoopBattleRuntime_IsLocalActionSubmitted());
    EXPECT(!PeekActionIntent());
    CoopBattleRuntime_DisarmEngine();
    Free(gBattleResources);
    Free(gBattleStruct);
    gBattleResources = savedResources;
    gBattleStruct = savedBattleStruct;
    CoopBattleRuntime_Init();
}

TEST("Cloud Coop item and switch actions run in the canonical order on both ROMs")
{
    struct Pokemon member0[MEMBER_MONS], member1[MEMBER_MONS];
    struct BattleStruct *savedBattleStruct = gBattleStruct;
    struct BattleResources *savedResources = gBattleResources;
    void (*savedMainFunc)(void) = gBattleMainFunc;
    u8 view, i;

    gBattleStruct = AllocZeroed(sizeof(*gBattleStruct));
    gBattleResources = AllocZeroed(sizeof(*gBattleResources));
    for (view = 0; view < 2; view++)
    {
        BuildMembers(member0, member1);
        ArmTrainerView(view, member0, member1);
        StageTrainerView(view, member0, member1);
        /* Both members use an item; the opponents switch and use an item. */
        gChosenActionByBattler[0] = B_ACTION_USE_ITEM;
        gChosenActionByBattler[1] = B_ACTION_SWITCH;
        gChosenActionByBattler[2] = B_ACTION_USE_ITEM;
        gChosenActionByBattler[3] = B_ACTION_USE_ITEM;
        CoopBattle_TestSetTurnOrder();
        /* Member 0 first, then opponent left, member 1, opponent right, on
         * either ROM (member 1's ROM numbers member 0's battler 2). */
        for (i = 0; i < MAX_BATTLERS_COUNT; i++)
        {
            EXPECT_EQ(CoopBattleRuntime_CanonicalBattler(gBattlerByTurnOrder[i]), i);
            EXPECT_EQ(gActionsByTurnOrder[i], gChosenActionByBattler[gBattlerByTurnOrder[i]]);
        }
        EXPECT_EQ(gBattlerByTurnOrder[0], view == 0 ? 0 : 2);
        CoopBattleRuntime_DisarmEngine();
    }
    gBattleMainFunc = savedMainFunc;
    Free(gBattleResources);
    Free(gBattleStruct);
    gBattleResources = savedResources;
    gBattleStruct = savedBattleStruct;
    CoopBattleRuntime_Init();
}

/* ------------------------------------------------------------------------
 * The whole vanilla item path in a multi battle with the co-op party layout
 * (B_TRAINER_0 at playerLeft, B_TRAINER_2 at playerRight). Seat 0 is the
 * acting member's own ROM (it is playerLeft); seat 1 the partner's ROM
 * (the acting member is playerRight). Both must end the same way.
 */

static u16 SpeciesHp(enum BattleTrainer trainer, enum Species species)
{
    u8 i;

    for (i = 0; i < PARTY_SIZE; i++)
        if (GetMonData(&gParties[trainer][i], MON_DATA_SPECIES) == species)
            return GetMonData(&gParties[trainer][i], MON_DATA_HP);
    return 0xFFFF;
}

MULTI_BATTLE_TEST("Cloud Coop item: a Potion heals the acting member's benched Pokemon from either seat")
{
    u32 seat;
    PARAMETRIZE { seat = 0; }
    PARAMETRIZE { seat = 1; }
    GIVEN {
        ASSUME(GetItemBattleUsage(ITEM_POTION) == EFFECT_ITEM_RESTORE_HP);
        if (seat == 0)
        {
            PLAYER(SPECIES_WOBBUFFET) { HP(40); MaxHP(100); Moves(MOVE_CELEBRATE); }
            PLAYER(SPECIES_WYNAUT) { Moves(MOVE_CELEBRATE); }
            PARTNER(SPECIES_ZIGZAGOON) { HP(10); MaxHP(100); Moves(MOVE_CELEBRATE); }
        }
        else
        {
            PLAYER(SPECIES_ZIGZAGOON) { HP(10); MaxHP(100); Moves(MOVE_CELEBRATE); }
            PARTNER(SPECIES_WOBBUFFET) { HP(40); MaxHP(100); Moves(MOVE_CELEBRATE); }
            PARTNER(SPECIES_WYNAUT) { Moves(MOVE_CELEBRATE); }
        }
        OPPONENT_A(SPECIES_WOBBUFFET) { Moves(MOVE_CELEBRATE); }
        OPPONENT_B(SPECIES_WOBBUFFET) { Moves(MOVE_CELEBRATE); }
    } WHEN {
        /* The acting member's lead goes to the bench, so its party index
         * (0) equals the other member's active index. */
        TURN {
            SWITCH(seat == 0 ? playerLeft : playerRight, 1);
            MOVE(seat == 0 ? playerRight : playerLeft, MOVE_CELEBRATE);
            MOVE(opponentLeft, MOVE_CELEBRATE);
            MOVE(opponentRight, MOVE_CELEBRATE);
        }
        TURN {
            USE_ITEM(seat == 0 ? playerLeft : playerRight, ITEM_POTION, partyIndex: 0);
            MOVE(seat == 0 ? playerRight : playerLeft, MOVE_CELEBRATE);
            MOVE(opponentLeft, MOVE_CELEBRATE);
            MOVE(opponentRight, MOVE_CELEBRATE);
        }
    } THEN {
        EXPECT_EQ(SpeciesHp(seat == 0 ? B_TRAINER_0 : B_TRAINER_2, SPECIES_WOBBUFFET),
                  40 + GetItemHoldEffectParam(ITEM_POTION));
        /* The other member's active Pokemon is not healed instead. */
        EXPECT_EQ(SpeciesHp(seat == 0 ? B_TRAINER_2 : B_TRAINER_0, SPECIES_ZIGZAGOON), 10);
        EXPECT_EQ((seat == 0 ? playerRight : playerLeft)->hp, 10);
    }
}

MULTI_BATTLE_TEST("Cloud Coop item: a status heal cures only the acting member's Pokemon from either seat")
{
    u32 seat;
    PARAMETRIZE { seat = 0; }
    PARAMETRIZE { seat = 1; }
    GIVEN {
        ASSUME(GetItemBattleUsage(ITEM_ANTIDOTE) == EFFECT_ITEM_CURE_STATUS);
        if (seat == 0)
        {
            PLAYER(SPECIES_WOBBUFFET) { Status1(STATUS1_POISON); Moves(MOVE_CELEBRATE); }
            PARTNER(SPECIES_ZIGZAGOON) { Status1(STATUS1_POISON); Moves(MOVE_CELEBRATE); }
        }
        else
        {
            PLAYER(SPECIES_ZIGZAGOON) { Status1(STATUS1_POISON); Moves(MOVE_CELEBRATE); }
            PARTNER(SPECIES_WOBBUFFET) { Status1(STATUS1_POISON); Moves(MOVE_CELEBRATE); }
        }
        OPPONENT_A(SPECIES_WOBBUFFET) { Moves(MOVE_CELEBRATE); }
        OPPONENT_B(SPECIES_WOBBUFFET) { Moves(MOVE_CELEBRATE); }
    } WHEN {
        TURN {
            USE_ITEM(seat == 0 ? playerLeft : playerRight, ITEM_ANTIDOTE, partyIndex: 0);
            MOVE(seat == 0 ? playerRight : playerLeft, MOVE_CELEBRATE);
            MOVE(opponentLeft, MOVE_CELEBRATE);
            MOVE(opponentRight, MOVE_CELEBRATE);
        }
    } THEN {
        EXPECT_EQ((seat == 0 ? playerLeft : playerRight)->status1, STATUS1_NONE);
        EXPECT_EQ((seat == 0 ? playerRight : playerLeft)->status1, STATUS1_POISON);
    }
}

MULTI_BATTLE_TEST("Cloud Coop item: an X item raises only the acting member's battler from either seat")
{
    u32 seat;
    PARAMETRIZE { seat = 0; }
    PARAMETRIZE { seat = 1; }
    GIVEN {
        ASSUME(GetItemBattleUsage(ITEM_X_ATTACK) == EFFECT_ITEM_INCREASE_STAT);
        if (seat == 0)
        {
            PLAYER(SPECIES_WOBBUFFET) { Moves(MOVE_CELEBRATE); }
            PARTNER(SPECIES_ZIGZAGOON) { Moves(MOVE_CELEBRATE); }
        }
        else
        {
            PLAYER(SPECIES_ZIGZAGOON) { Moves(MOVE_CELEBRATE); }
            PARTNER(SPECIES_WOBBUFFET) { Moves(MOVE_CELEBRATE); }
        }
        OPPONENT_A(SPECIES_WOBBUFFET) { Moves(MOVE_CELEBRATE); }
        OPPONENT_B(SPECIES_WOBBUFFET) { Moves(MOVE_CELEBRATE); }
    } WHEN {
        TURN {
            USE_ITEM(seat == 0 ? playerLeft : playerRight, ITEM_X_ATTACK, partyIndex: 0);
            MOVE(seat == 0 ? playerRight : playerLeft, MOVE_CELEBRATE);
            MOVE(opponentLeft, MOVE_CELEBRATE);
            MOVE(opponentRight, MOVE_CELEBRATE);
        }
    } THEN {
        EXPECT_EQ((seat == 0 ? playerLeft : playerRight)->statStages[STAT_ATK],
                  DEFAULT_STAT_STAGE + GetItemHoldEffectParam(ITEM_X_ATTACK));
        EXPECT_EQ((seat == 0 ? playerRight : playerLeft)->statStages[STAT_ATK], DEFAULT_STAT_STAGE);
    }
}

MULTI_BATTLE_TEST("Cloud Coop item: a Revive restores the acting member's fainted Pokemon from either seat")
{
    u32 seat;
    PARAMETRIZE { seat = 0; }
    PARAMETRIZE { seat = 1; }
    GIVEN {
        ASSUME(GetItemBattleUsage(ITEM_REVIVE) == EFFECT_ITEM_REVIVE);
        if (seat == 0)
        {
            PLAYER(SPECIES_WOBBUFFET) { Moves(MOVE_CELEBRATE); }
            PLAYER(SPECIES_WYNAUT) { HP(0); MaxHP(100); Moves(MOVE_CELEBRATE); }
            PARTNER(SPECIES_ZIGZAGOON) { Moves(MOVE_CELEBRATE); }
            PARTNER(SPECIES_WYNAUT) { HP(0); MaxHP(100); Moves(MOVE_CELEBRATE); }
        }
        else
        {
            PLAYER(SPECIES_ZIGZAGOON) { Moves(MOVE_CELEBRATE); }
            PLAYER(SPECIES_WYNAUT) { HP(0); MaxHP(100); Moves(MOVE_CELEBRATE); }
            PARTNER(SPECIES_WOBBUFFET) { Moves(MOVE_CELEBRATE); }
            PARTNER(SPECIES_WYNAUT) { HP(0); MaxHP(100); Moves(MOVE_CELEBRATE); }
        }
        OPPONENT_A(SPECIES_WOBBUFFET) { Moves(MOVE_CELEBRATE); }
        OPPONENT_B(SPECIES_WOBBUFFET) { Moves(MOVE_CELEBRATE); }
    } WHEN {
        TURN {
            USE_ITEM(seat == 0 ? playerLeft : playerRight, ITEM_REVIVE, partyIndex: 1);
            MOVE(seat == 0 ? playerRight : playerLeft, MOVE_CELEBRATE);
            MOVE(opponentLeft, MOVE_CELEBRATE);
            MOVE(opponentRight, MOVE_CELEBRATE);
        }
    } THEN {
        EXPECT_EQ(SpeciesHp(seat == 0 ? B_TRAINER_0 : B_TRAINER_2, SPECIES_WYNAUT), 50);
        /* The other member's fainted Pokemon stays fainted. */
        EXPECT_EQ(SpeciesHp(seat == 0 ? B_TRAINER_2 : B_TRAINER_0, SPECIES_WYNAUT), 0);
    }
}
