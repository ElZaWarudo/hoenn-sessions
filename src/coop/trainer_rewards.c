#include "global.h"
#include "coop/trainer_rewards.h"
#include "coop/battle_consent.h"
#include "coop/battle_runtime.h"
#include "battle.h"
#include "battle_controllers.h"
#include "battle_script_commands.h"
#include "battle_setup.h"
#include "battle_util.h"
#include "caps.h"
#include "data.h"
#include "evolution_scene.h"
#include "event_data.h"
#include "item.h"
#include "main.h"
#include "mastery.h"
#include "money.h"
#include "overworld.h"
#include "pokemon.h"
#include "window.h"
#include "constants/battle.h"
#include "constants/hold_effects.h"
#include "constants/trainers.h"

_Static_assert(COOP_BATTLE_MULTI_PARTY_SIZE <= 3,
               "a faint record holds three staged-slot bits per field");

/* 10 bytes of EWRAM: everything else is derived after the battle. */
struct CoopTrainerRewardState
{
    u8 faints[PARTY_SIZE]; // per opponent party slot, see COOP_TRAINER_REWARD_*
    u8 sent[2];            // per opponent flank: local staged slots sent in
    bool8 armed;
    u8 money_multiplier;
};

static EWRAM_DATA struct CoopTrainerRewardState sRewards = {0};

static u32 CountBits(u32 bits)
{
    u32 count = 0;

    for (; bits != 0; bits &= bits - 1)
        count++;
    return count;
}

/* The local member always fights from the left player position, with its
 * staged mons in B_TRAINER_0 (member 1's ROM translates targets instead). */
static u32 GetLocalSentBit(bool32 requirePresent)
{
    enum BattlerId local = GetBattlerAtPosition(B_POSITION_PLAYER_LEFT);

    if (local >= gBattlersCount || gBattlerPartyIndexes[local] >= COOP_BATTLE_MULTI_PARTY_SIZE
     || (requirePresent && (gAbsentBattlerFlags & (1u << local))))
        return 0;
    return 1u << gBattlerPartyIndexes[local];
}

void CoopTrainerRewards_Begin(void)
{
    memset(&sRewards, 0, sizeof(sRewards));
    sRewards.money_multiplier = 1;
    sRewards.armed = TRUE;
}

void CoopTrainerRewards_OnSentPokesReset(void)
{
    if (!sRewards.armed)
        return;
    sRewards.sent[0] = sRewards.sent[1] = GetLocalSentBit(FALSE);
}

void CoopTrainerRewards_OnOpponentSwitchIn(u8 battler)
{
    if (!sRewards.armed || battler >= MAX_BATTLERS_COUNT || IsOnPlayerSide(battler))
        return;
    sRewards.sent[(battler & BIT_FLANK) >> 1] = GetLocalSentBit(TRUE);
}

void CoopTrainerRewards_OnPlayerSwitchIn(u8 battler)
{
    if (!sRewards.armed || battler != GetBattlerAtPosition(B_POSITION_PLAYER_LEFT))
        return;
    sRewards.sent[0] |= GetLocalSentBit(FALSE);
    sRewards.sent[1] |= GetLocalSentBit(FALSE);
}

/* Reads the battle, never writes it: the vanilla Cmd_getexp decisions that
 * depend on the moment of the faint (who is alive, who fought it, who holds
 * an EXP Share) are frozen here and replayed after the battle. */
void CoopTrainerRewards_RecordFaint(u8 battler)
{
    u32 slot;
    u32 i;
    u32 valid = 0;
    u32 share = 0;

    if (!sRewards.armed || battler >= gBattlersCount || IsOnPlayerSide(battler))
        return;
    slot = gBattlerPartyIndexes[battler];
    if (slot >= PARTY_SIZE || (sRewards.faints[slot] & COOP_TRAINER_REWARD_FAINTED))
        return;
    for (i = 0; i < COOP_BATTLE_MULTI_PARTY_SIZE; i++)
    {
        struct Pokemon *mon = &gParties[B_TRAINER_0][i];

        if (!IsValidForBattle(mon))
            continue;
        valid |= 1u << i;
        if (GetItemHoldEffect(GetMonData(mon, MON_DATA_HELD_ITEM)) == HOLD_EFFECT_EXP_SHARE
         || IsGen6ExpShareEnabled())
            share |= 1u << i;
    }
    sRewards.faints[slot] = COOP_TRAINER_REWARD_FAINTED
                          | (sRewards.sent[(battler & BIT_FLANK) >> 1] & valid)
                          | (share << COOP_TRAINER_REWARD_SHARE_SHIFT);
}

void CoopTrainerRewards_OnBattleWon(u8 moneyMultiplier)
{
    if (sRewards.armed)
        sRewards.money_multiplier = moneyMultiplier != 0 ? moneyMultiplier : 1;
}

u32 CoopTrainerRewards_GetPrizeMoney(u16 trainerId, u8 moneyMultiplier)
{
    return GetTrainerPrizeMoney(trainerId, moneyMultiplier != 0 ? moneyMultiplier : 1,
                                GetTrainerBattleType(trainerId) == TRAINER_BATTLE_TYPE_DOUBLES);
}

static void LearnLevelUpMovesSilently(struct Pokemon *mon)
{
    enum Move move;

    /* GiveMoveToMon fills an empty slot; a full moveset returns
     * MON_HAS_MAX_MOVES and the move is skipped (no replace prompt). */
    for (move = MonTryLearningNewMove(mon, TRUE); move != MOVE_NONE;
         move = MonTryLearningNewMove(mon, FALSE))
        ;
}

/* Level by level, so every level's moves and friendship are applied as the
 * battle controller would have done. */
static void GiveMonExperience(struct Pokemon *mon, u32 amount, u32 partySlot)
{
    enum Species species = GetMonData(mon, MON_DATA_SPECIES);
    const u32 *table = gExperienceTables[gSpeciesInfo[species].growthRate];
    u32 experience = GetMonData(mon, MON_DATA_EXP);
    u32 maximum = GetMaxMonExperience(species);
    u32 level = GetMonData(mon, MON_DATA_LEVEL);
    u32 target;

    if (amount == 0 || experience >= maximum)
        return;
    target = experience + min(amount, maximum - experience);
    while (level < MAX_LEVEL && target >= table[level + 1])
    {
        u32 step = table[level + 1];
        u32 newLevel;

        SetMonData(mon, MON_DATA_EXP, &step);
        CalculateMonStats(mon);
        newLevel = GetMonData(mon, MON_DATA_LEVEL);
        if (newLevel <= level)
            break;
        level = newLevel;
        AdjustFriendship(mon, FRIENDSHIP_EVENT_GROW_LEVEL);
        LearnLevelUpMovesSilently(mon);
        gLeveledUpInBattle |= 1u << partySlot;
    }
    SetMonData(mon, MON_DATA_EXP, &target);
    CalculateMonStats(mon);
}

/* Cmd_getexp for one fainted opponent, from its faint record. */
static void ApplyOpponentExperience(struct Pokemon *opponent, u8 record,
                                    const u8 *stagedSlots, u8 stagedCount)
{
    enum Species fainted = GetMonData(opponent, MON_DATA_SPECIES);
    u32 faintedLevel = GetMonData(opponent, MON_DATA_LEVEL);
    u32 sentBits = record & COOP_TRAINER_REWARD_STAGED_MASK;
    u32 shareBits = (record >> COOP_TRAINER_REWARD_SHARE_SHIFT) & COOP_TRAINER_REWARD_STAGED_MASK;
    u32 viaSentIn = CountBits(sentBits);
    u32 viaExpShare = CountBits(shareBits);
    u32 calculatedExp;
    u32 expValue;
    u32 shareValue;
    u32 i;

    if (fainted == SPECIES_NONE || (sentBits | shareBits) == 0)
        return;
    calculatedExp = gSpeciesInfo[fainted].expYield * faintedLevel;
    if (B_SCALED_EXP >= GEN_5 && B_SCALED_EXP != GEN_6)
        calculatedExp /= 5;
    else
        calculatedExp /= 7;
    if (B_TRAINER_EXP_MULTIPLIER <= GEN_7)
        calculatedExp = (calculatedExp * 150) / 100;

    if (B_SPLIT_EXP < GEN_6)
    {
        if (viaExpShare)
        {
            expValue = SAFE_DIV(calculatedExp / 2, viaSentIn);
            shareValue = calculatedExp / 2 / viaExpShare;
            if (shareValue == 0)
                shareValue = 1;
        }
        else
        {
            expValue = SAFE_DIV(calculatedExp, viaSentIn);
            shareValue = 0;
        }
        if (expValue == 0)
            expValue = 1;
    }
    else
    {
        expValue = calculatedExp;
        shareValue = calculatedExp / 2;
        if (shareValue == 0)
            shareValue = 1;
    }

    for (i = 0; i < stagedCount; i++)
    {
        struct Pokemon *mon;
        bool32 wasSentOut = (sentBits & (1u << i)) != 0;
        bool32 shared = (shareBits & (1u << i)) != 0;
        s32 reward;
        u32 level;

        if ((!wasSentOut && !shared) || stagedSlots[i] >= PARTY_SIZE)
            continue;
        mon = &gParties[B_TRAINER_0][stagedSlots[i]];
        if (GetMonData(mon, MON_DATA_SPECIES_OR_EGG) == SPECIES_NONE
         || GetMonData(mon, MON_DATA_SPECIES_OR_EGG) == SPECIES_EGG)
            continue;
        if (!CanMonGainExperience(mon))
        {
            if (B_MAX_LEVEL_EV_GAINS >= GEN_5)
                MonGainEVs(mon, fainted);
            continue;
        }
        level = GetMonData(mon, MON_DATA_LEVEL);
        reward = wasSentOut ? GetSoftLevelCapExpValue(level, expValue) : 0;
        if (shared && (B_SPLIT_EXP < GEN_6 || reward == 0))
            reward += GetSoftLevelCapExpValue(level, shareValue);
        ApplyMonExperienceMultipliers(&reward, mon, faintedLevel);
        if (B_EXP_CAP_TYPE == EXP_CAP_HARD && reward != 0)
        {
            const u32 *table = gExperienceTables[gSpeciesInfo[GetMonData(mon, MON_DATA_SPECIES)].growthRate];
            u32 current = GetMonData(mon, MON_DATA_EXP);
            u32 cap = GetCurrentLevelCap();

            if (level >= cap)
                reward = 0;
            else if (table[cap] < current + reward)
                reward = table[cap] - current;
        }
        MonGainEVs(mon, fainted);
        if (reward > 0)
            GiveMonExperience(mon, reward, stagedSlots[i]);
    }
}

enum CoopTrainerRewardRole CoopTrainerRewards_Apply(bool8 won, u16 trainerId,
                                                    const u8 *stagedSlots, u8 stagedCount)
{
    struct CoopTrainerRewardState state = sRewards;
    enum CoopTrainerRewardRole role;
    u32 i;

    /* Disarm first: whatever happens below, this battle is settled. */
    memset(&sRewards, 0, sizeof(sRewards));
    if (!state.armed || !won || trainerId == TRAINER_NONE || trainerId >= TRAINERS_COUNT
     || stagedSlots == NULL || stagedCount == 0
     || stagedCount > COOP_BATTLE_MULTI_PARTY_SIZE
     || !CoopTrainerEncounter_IsPhaseOneClass(GetTrainerClassFromId(trainerId)))
        return COOP_TRAINER_REWARD_NONE;

    if (HasTrainerBeenFought(trainerId))
    {
        role = COOP_TRAINER_REWARD_HELPER;
    }
    else
    {
        role = COOP_TRAINER_REWARD_PARTICIPANT;
        AddMoney(&gSaveBlock1Ptr->money,
                 CoopTrainerRewards_GetPrizeMoney(trainerId, state.money_multiplier));
        SetTrainerFlag(trainerId);
    }

    gLeveledUpInBattle = 0;
    gTriedEvolving = 0;
    for (i = 0; i < PARTY_SIZE; i++)
        if (state.faints[i] & COOP_TRAINER_REWARD_FAINTED)
            ApplyOpponentExperience(&gParties[B_TRAINER_1][i], state.faints[i],
                                    stagedSlots, stagedCount);
    return role;
}

bool8 CoopTrainerRewards_HasPendingEvolutions(void)
{
    return gLeveledUpInBattle != 0;
}

/* TryEvolvePokemon's level-up pass, outside the battle. */
void CB2_CoopTrainerRewardEvolutions(void)
{
    u32 i;

    for (i = 0; i < PARTY_SIZE; i++)
    {
        struct Pokemon *mon = &gParties[B_TRAINER_0][i];
        bool32 canStopEvo = TRUE;
        enum Species species;

        if (!(gLeveledUpInBattle & (1u << i)))
            continue;
        gLeveledUpInBattle &= ~(1u << i);
        species = GetEvolutionTargetSpecies(mon, EVO_MODE_BATTLE_ONLY, gLeveledUpInBattle,
                                            NULL, &canStopEvo, CHECK_EVO);
        if (species == SPECIES_NONE)
            continue;
        GetEvolutionTargetSpecies(mon, EVO_MODE_BATTLE_ONLY, gLeveledUpInBattle,
                                  NULL, &canStopEvo, DO_EVO);
        gCB2_AfterEvolution = CB2_CoopTrainerRewardEvolutions;
        FreeAllWindowBuffers();
        EvolutionScene(mon, species, canStopEvo, i);
        return;
    }
    gLeveledUpInBattle = 0;
    gTriedEvolving = 0;
    SetMainCallback2(CB2_ReturnToFieldContinueScriptPlayMapMusic);
}

#if TESTING
bool8 CoopTrainerRewards_TestIsArmed(void)
{
    return sRewards.armed;
}

u8 CoopTrainerRewards_TestGetFaintRecord(u8 opponentSlot)
{
    return opponentSlot < PARTY_SIZE ? sRewards.faints[opponentSlot] : 0;
}
#endif
