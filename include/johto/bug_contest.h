#ifndef GUARD_JOHTO_BUG_CONTEST_H
#define GUARD_JOHTO_BUG_CONTEST_H

#include "global.h"

#define JOHTO_BUG_CONTEST_TIME_LIMIT_FRAMES (60u * 60u * 8u)
#define JOHTO_BUG_CONTEST_MAX_CATCHES 5

enum JohtoBugContestStatus
{
    JOHTO_BUG_CONTEST_OK,
    JOHTO_BUG_CONTEST_NO_CONTEST,
    JOHTO_BUG_CONTEST_ALREADY_ACTIVE,
    JOHTO_BUG_CONTEST_INVALID_PARTY,
    JOHTO_BUG_CONTEST_PC_FULL,
    JOHTO_BUG_CONTEST_BAG_FULL,
    JOHTO_BUG_CONTEST_NOT_ENDING,
    JOHTO_BUG_CONTEST_INVALID_SELECTION,
    JOHTO_BUG_CONTEST_SELECTION_LOCKED,
    JOHTO_BUG_CONTEST_NOT_JUDGED,
    JOHTO_BUG_CONTEST_TRANSFER_FAILED,
    JOHTO_BUG_CONTEST_REWARD_FAILED,
    JOHTO_BUG_CONTEST_EXIT_BLOCKED,
};

enum JohtoBugContestEndReason
{
    JOHTO_BUG_CONTEST_END_TIMEOUT,
    JOHTO_BUG_CONTEST_END_RETIRE,
    JOHTO_BUG_CONTEST_END_FULL_PARTY,
    JOHTO_BUG_CONTEST_END_DEFEAT,
};

/* Begin takes an unsigned VBlank counter and owns a temporary contest party. */
enum JohtoBugContestStatus JohtoBugContest_Begin(u32 now);
bool32 JohtoBugContest_IsActive(void);
bool32 JohtoBugContest_IsEnding(void);
/* TRUE while the contest owns the player's original party and mail. */
bool32 JohtoBugContest_IsSerializationBlocked(void);
/* Returns TRUE only when this call changes an active contest to ending. */
bool32 JohtoBugContest_CheckTime(u32 now);
enum JohtoBugContestStatus JohtoBugContest_RequestEnd(enum JohtoBugContestEndReason reason);
enum JohtoBugContestStatus JohtoBugContest_Judge(u16 slot);
enum JohtoBugContestStatus JohtoBugContest_TransferSelected(void);
enum JohtoBugContestStatus JohtoBugContest_ClaimReward(void);
enum JohtoBugContestStatus JohtoBugContest_Exit(void);
enum JohtoBugContestStatus JohtoBugContest_Abort(void);

/* These helpers are deterministic and are also used by the production judge. */
u8 JohtoBugContest_GetPlacement(u32 maxHp, u16 draw);
u16 JohtoBugContest_GetRewardForPlacement(u8 placement, u16 draw);
bool32 JohtoBugContest_IsContestSpecies(u16 species);
u8 JohtoBugContest_GetSelectedDisplayIndex(void);
u16 JohtoBugContest_GetSelectedSpecies(void);
const u8 *JohtoBugContest_GetSelectedName(void);
u16 JohtoBugContest_GetReward(void);
/* Zero before judging; the original randomized placement after judging. */
u8 JohtoBugContest_GetSelectedPlacement(void);

#if TESTING
void JohtoBugContest_TestReset(void);
#endif

#endif // GUARD_JOHTO_BUG_CONTEST_H
