#ifndef GUARD_COOP_TRAINER_REWARDS_H
#define GUARD_COOP_TRAINER_REWARDS_H

#include "global.h"

struct ScriptContext;

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
 * ROM's business), and on a win settles the member's role:
 *
 * - Ordinary trainer: a player whose trainer flag was clear gets the flag and
 *   the prize (participant); otherwise a helper (EXP only).
 * - Match-call rematch (CALVIN_2 ... 5, ROXANNE_2 ... 5): a player who has
 *   beaten the table's first-battle trainer gets the rematch prize; on the
 *   requester's ROM the vanilla rematch-win bookkeeping is also recorded
 *   (BattleSetup_ApplyCoopRematchWin). A player who has not beaten the first
 *   battle is a helper: no money and no flags.
 * - Hoenn gym leader (first battle): the requester gets the trainer flag,
 *   match-call registration and the prize; the leader's own post-battle script
 *   then gives the badge, TM and story flags as in vanilla. The partner gets
 *   the same grants from sHoennGyms when it lacks this badge and holds every
 *   earlier one (COOP_TRAINER_REWARD_GYM_PARTNER); otherwise it helps.
 *
 * Apply runs at most once per armed battle, so a resumed post-battle script or
 * a re-entered end callback can never pay twice. */

/* One byte per opponent party slot. */
#define COOP_TRAINER_REWARD_FAINTED     0x80 // the opponent fainted; record taken
#define COOP_TRAINER_REWARD_SHARE_SHIFT 3    // bits 3..5: staged mons on EXP Share
#define COOP_TRAINER_REWARD_STAGED_MASK 0x07 // bits 0..2: staged mons that fought it

#define COOP_HOENN_GYM_COUNT 8
#define COOP_HOENN_GYM_NONE  0xFF

enum CoopTrainerRewardRole
{
    COOP_TRAINER_REWARD_NONE,        // loss, abort, not armed or not eligible
    COOP_TRAINER_REWARD_PARTICIPANT, // flag newly set, prize paid, EXP applied
    COOP_TRAINER_REWARD_HELPER,      // flag was already set; EXP applied only
    COOP_TRAINER_REWARD_GYM_PARTNER, // participant partner of a gym win: the
                                     // badge, TM and story grants are applied
                                     // and the notice script must be shown
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
 * original party slot the restored mon sits in. requester is TRUE only on the
 * ROM whose parked trainer script asked for the battle. Level-ups are left in
 * gLeveledUpInBattle for CB2_CoopTrainerRewardEvolutions. */
enum CoopTrainerRewardRole CoopTrainerRewards_Apply(bool8 won, u16 trainerId,
                                                    const u8 *stagedSlots, u8 stagedCount,
                                                    bool8 requester);
/* The vanilla solo prize for this trainer (a doubles trainer pays double). */
u32 CoopTrainerRewards_GetPrizeMoney(u16 trainerId, u8 moneyMultiplier);
bool8 CoopTrainerRewards_HasPendingEvolutions(void);
/* Runs the post-battle evolutions for every mon that levelled up, then
 * returns to the field and resumes the parked trainer script. */
void CB2_CoopTrainerRewardEvolutions(void);

/* Hoenn gyms in badge order (0 = Rustboro ... 7 = Sootopolis). */
u8 CoopTrainerRewards_GetHoennGym(u16 trainerId); // first-battle leader only
bool8 CoopTrainerRewards_IsHoennGymLeader(u16 trainerId); // leader or rematch
/* "Same story point": holds every earlier Hoenn badge and neither this
 * gym's nor any later one (its badge count equals the gym's index). */
bool8 CoopTrainerRewards_IsGymPartnerEligible(u8 gym);
/* The partner's badge/TM notice for a GYM_PARTNER result. */
const u8 *CoopTrainerRewards_GetGymNoticeScript(u16 trainerId);
/* callnative from the notice script: loads VAR_0x8000/1/7 with the TM, one,
 * and whether it fit in the bag (EventScript_ObtainItemMessage's inputs). */
void CoopTrainerRewards_LoadGymNoticeItem(struct ScriptContext *ctx);

#if TESTING
bool8 CoopTrainerRewards_TestIsArmed(void);
u8 CoopTrainerRewards_TestGetFaintRecord(u8 opponentSlot);
#endif

#endif // GUARD_COOP_TRAINER_REWARDS_H
