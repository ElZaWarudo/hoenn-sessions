#ifndef GUARD_COOP_FRIENDLY_BATTLE_H
#define GUARD_COOP_FRIENDLY_BATTLE_H

#include "gba/types.h"
#include "coop/battle_runtime.h"

/*
 * Friendly battles between the two grouped players (item 4.6).
 *
 *   challenger: ONLINE > Battle partner > format, levels, count > Send
 *               -> TRAINER_BATTLE_RESERVE with the rules -> picks its team
 *               while the partner decides -> waits for the start.
 *   partner:    BATTLE_JOIN_OFFER with the rules -> Yes/No when the field
 *               is free -> Yes: picks the same number of Pokemon -> waits.
 *
 * Each side's snapshot is its picked team in pick order, so the manifest's
 * party digests name exactly the Pokemon that battle. Nothing in the real
 * party changes: the battle uses copies (scaled to Lv. 50 when asked) and
 * the whole party is restored afterwards.
 */

enum CoopFriendlyPhase
{
    COOP_FRIENDLY_IDLE,
    COOP_FRIENDLY_PICKING,
    COOP_FRIENDLY_WAITING,
    COOP_FRIENDLY_IN_BATTLE,
    COOP_FRIENDLY_DONE,
};

enum CoopFriendlyResult
{
    COOP_FRIENDLY_RESULT_NONE,
    COOP_FRIENDLY_RESULT_CANCELLED,
    COOP_FRIENDLY_RESULT_DECLINED,
    COOP_FRIENDLY_RESULT_NO_ANSWER,
    COOP_FRIENDLY_RESULT_WITHDRAWN,
    COOP_FRIENDLY_RESULT_UNAVAILABLE,
    COOP_FRIENDLY_RESULT_TIMED_OUT,
    COOP_FRIENDLY_RESULT_WON,
    COOP_FRIENDLY_RESULT_LOST,
    COOP_FRIENDLY_RESULT_DRAW,
    COOP_FRIENDLY_RESULT_NO_CONTEST,
};

/* CoopFriendly_PickMon results. */
enum
{
    COOP_FRIENDLY_PICK_OK,
    COOP_FRIENDLY_PICK_CANCELLED,
    COOP_FRIENDLY_PICK_REFUSED,
    COOP_FRIENDLY_PICK_ENDED,
};

/* The partner's Yes/No closes itself after this long (the server keeps an
 * unanswered reservation for 30 s). The team is picked after the Yes. */
#define COOP_FRIENDLY_OFFER_FRAMES (30 * 60)
/* From an acceptance to the battle start: both players pick their team,
 * then the snapshot, ready and start exchange runs. */
#define COOP_FRIENDLY_START_FRAMES (90 * 60)

void CoopFriendly_Init(void);
enum CoopFriendlyPhase CoopFriendly_GetPhase(void);
u8 CoopFriendly_GetResult(void);
/* The rules of the flow in progress, or the default single, as-is, one
 * Pokemon rules while idle. */
void CoopFriendly_GetRules(struct CoopBattleFriendlyRules *rules);
/* Usable (non-egg, HP above zero) Pokemon in the party. */
u8 CoopFriendly_CountUsableMons(void);
/* ONLINE menu gating: grouped, co-op session idle, nothing else in flight. */
bool8 CoopFriendly_CanBegin(void);

/* Sends the challenge; the challenger then picks its team. */
bool8 CoopFriendly_BeginChallenge(const struct CoopBattleFriendlyRules *rules);
void CoopFriendly_StartChallengeScript(const struct CoopBattleFriendlyRules *rules);
/* The partner accepted the offer shown with these rules. */
bool8 CoopFriendly_BeginResponderPicks(const struct CoopBattleFriendlyRules *rules);
u8 CoopFriendly_PickMon(u8 slot);
u8 CoopFriendly_PickedCount(void);
bool8 CoopFriendly_IsTeamComplete(void);
/* Copies the picked team in pick order; returns its size (0 if it is no
 * longer battle-ready). */
u8 CoopFriendly_BuildTeam(struct Pokemon *team);
/* The field waits (locked, script parked) for the battle to start. */
bool8 CoopFriendly_IsWaiting(void);
void CoopFriendly_BeginWaiting(void);
/* The consent flow ended before a battle; the waiting script shows why. */
void CoopFriendly_End(u8 result);
void CoopFriendly_OnBattleStarted(void);
void CoopFriendly_OnBattleEnded(u8 result);
const u8 *CoopFriendly_GetResultText(void);
void CoopFriendly_Finish(void);
/* The copy each ROM applies to both sides when the rules ask for Lv. 50. */
void CoopFriendly_ScaleTeam(struct Pokemon *team, u8 count, u8 level_mode);
/* The secret-base opponent record shown for the partner: the name, gender
 * and ID of the OT of the partner's lead Pokemon. Display only. */
struct SecretBase;
void CoopFriendly_FillOpponentDisplay(struct SecretBase *base);
const u8 *CoopFriendly_FormatName(u8 format);
const u8 *CoopFriendly_LevelModeName(u8 level_mode);

void Special_CoopFriendlyBufferRules(void);
void Special_CoopFriendlyBeginResponderPicks(void);
void Special_CoopFriendlyBufferPick(void);
void Special_CoopFriendlyPickMon(void);
void Special_CoopFriendlyWait(void);
void Special_CoopFriendlyBufferResult(void);
void Special_CoopFriendlyFinish(void);

#endif // GUARD_COOP_FRIENDLY_BATTLE_H
