#ifndef GUARD_COOP_TRAINER_REWARDS_H
#define GUARD_COOP_TRAINER_REWARDS_H

#include "global.h"

/* Local rewards for co-op trainer encounters ("option A": until the server
 * ledger covers battles, each ROM applies the vanilla rewards to its own save).
 *
 * During the battle the engine suppresses EXP, because a level-up would change
 * the parties that the per-turn digest hashes. Instead this module records,
 * per fainted opponent party slot, which of the LOCAL staged mons (party slots
 * 0..2 of B_TRAINER_0 during the battle) would have shared its EXP. It never
 * touches a party or any hashed battle state while the battle runs.
 *
 * After the battle, once the full party has been restored, Apply turns the
 * records into vanilla EXP for the local mons only (partner mons are the other
 * ROM's business), and on a win sets the trainer flag and pays the prize to a
 * participant. A player whose flag was already set is a helper: no money.
 * Apply runs at most once per armed battle, so a resumed post-battle script or
 * a re-entered end callback can never pay twice. */

/* One byte per opponent party slot. */
#define COOP_TRAINER_REWARD_FAINTED     0x80 // the opponent fainted; record taken
#define COOP_TRAINER_REWARD_SHARE_SHIFT 3    // bits 3..5: staged mons on EXP Share
#define COOP_TRAINER_REWARD_STAGED_MASK 0x07 // bits 0..2: staged mons that fought it

enum CoopTrainerRewardRole
{
    COOP_TRAINER_REWARD_NONE,        // loss, abort, not armed or not eligible
    COOP_TRAINER_REWARD_PARTICIPANT, // flag newly set, prize paid, EXP applied
    COOP_TRAINER_REWARD_HELPER,      // flag was already set; EXP applied only
};

/* Arms the recorder when a co-op trainer battle is entered. */
void CoopTrainerRewards_Begin(void);
/* Mirrors of the vanilla sent-in bookkeeping (gSentPokesToOpponent), limited
 * to the local battler. The vanilla bits cannot be used directly because the
 * partner's party indices share the same bit positions. */
void CoopTrainerRewards_OnSentPokesReset(void);
void CoopTrainerRewards_OnOpponentSwitchIn(u8 battler);
void CoopTrainerRewards_OnPlayerSwitchIn(u8 battler);
/* Called where vanilla would run BattleScript_GiveExp for a fainted battler. */
void CoopTrainerRewards_RecordFaint(u8 battler);
/* Captures the battle's money multiplier (Amulet Coin, Happy Hour). */
void CoopTrainerRewards_OnBattleWon(u8 moneyMultiplier);
/* Applies the rewards once and disarms. stagedSlots maps staged slot i to the
 * original party slot the restored mon sits in. Level-ups are left in
 * gLeveledUpInBattle for CB2_CoopTrainerRewardEvolutions. */
enum CoopTrainerRewardRole CoopTrainerRewards_Apply(bool8 won, u16 trainerId,
                                                    const u8 *stagedSlots, u8 stagedCount);
/* The vanilla solo prize for this trainer (a doubles trainer pays double). */
u32 CoopTrainerRewards_GetPrizeMoney(u16 trainerId, u8 moneyMultiplier);
bool8 CoopTrainerRewards_HasPendingEvolutions(void);
/* Runs the post-battle evolutions for every mon that levelled up, then
 * returns to the field and resumes the parked trainer script. */
void CB2_CoopTrainerRewardEvolutions(void);

#if TESTING
bool8 CoopTrainerRewards_TestIsArmed(void);
u8 CoopTrainerRewards_TestGetFaintRecord(u8 opponentSlot);
#endif

#endif // GUARD_COOP_TRAINER_REWARDS_H
