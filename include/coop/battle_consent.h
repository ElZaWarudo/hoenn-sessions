#ifndef GUARD_COOP_BATTLE_CONSENT_H
#define GUARD_COOP_BATTLE_CONSENT_H

#include "gba/types.h"
#include "coop/battle_protocol.h"
#include "coop/battle_runtime.h"

enum CoopBattleKind
{
    COOP_BATTLE_KIND_COOPERATIVE_TRAINER = 1,
    COOP_BATTLE_KIND_FRIENDLY = 2,
};

void CoopBattleConsent_Init(void);
void CoopBattleConsent_Poll(void);
/* Friendly: sends the reserve with the friendly battle rules currently held
 * by coop/friendly_battle.c (singles, as is, one Pokemon while idle). */
bool8 CoopBattleConsent_Begin(u8 kind);
bool8 CoopBattleConsent_BeginFriendly(const struct CoopBattleFriendlyRules *rules);
/* Nothing in flight: no request, offer, accepted battle or encounter. */
bool8 CoopBattleConsent_IsIdle(void);
/* The rules of the friendly offer being answered (responder). */
bool8 CoopBattleConsent_GetOfferRules(struct CoopBattleFriendlyRules *rules);
bool8 CoopBattleConsent_TakeFriendlyPromptLock(void);
/* Withdraws this ROM's friendly reservation before its battle starts. */
void CoopBattleConsent_CancelFriendly(void);
void CoopBattleConsent_OnFriendlyBattleEnded(void);
/* The trainer's engine ID is resolved in the active region before sending. */
bool8 CoopBattleConsent_BeginTrainer(u16 legacy_trainer_id);
bool8 CoopBattleConsent_ReceiveOffer(const u8 *payload, u16 length);
bool8 CoopBattleConsent_ReceiveOutcome(const u8 *payload, u16 length);
bool8 CoopBattleConsent_ReceiveReserveRejected(const u8 *payload, u16 length);
bool8 CoopBattleConsent_ReceiveAbort(const u8 *payload, u16 length);
/* The reservation/offer ID may be used for snapshots before a manifest. */
bool8 CoopBattleConsent_IsCurrentBattle(const u8 *battle_id);
/* Copies the current reservation ID (requester waiting or responder locally
 * accepted). This does not prove the server accepted a responder's decision. */
bool8 CoopBattleConsent_CopyCurrentBattleId(u8 *battle_id, u16 capacity);
u8 CoopBattleConsent_GetOutcome(void);
bool8 CoopBattleConsent_GetOutcomeRecord(u8 *battle_id, u32 *request_nonce, u8 *outcome);
void CoopBattleConsent_OnSessionReady(void);
void CoopBattleConsent_OnTransportLost(void);

void Special_CoopBattleConsentGetOffer(void);
void Special_CoopBattleConsentMarkPrompt(void);
void Special_CoopBattleConsentGetOutcome(void);
void Special_CoopBattleConsentRespond(void);
void Special_CoopBattleConsentBeginWally(void);
void Special_CoopBattleConsentGetWallyResult(void);
void Special_CoopBattleConsentBeginBrock(void);
void Special_CoopBattleConsentGetBrockResult(void);
bool8 CoopBattleConsent_OnTrainerBattleEnded(bool8 completed, u8 battle_outcome);
void CoopBattleConsent_OnTrainerWhiteout(void);

/* Trainer encounters (phase 1: ordinary route trainers).
 *
 * dotrainerbattle asks CoopTrainerEncounter_TryBegin first. When it returns
 * TRUE a trainer reservation is on the wire and the trainer script stays
 * parked right after dotrainerbattle. CoopBattleConsent_Poll then starts the
 * co-op battle once the server releases it, or starts the vanilla battle with
 * the untouched trainer parameters on decline, offer expiry, reservation
 * rejection, transport loss, a start timeout or a failed co-op start. */
#define COOP_TRAINER_ENCOUNTER_PARTNER_TILES 12
/* The partner's Yes/No prompt declines itself after this long. */
#define COOP_TRAINER_ENCOUNTER_OFFER_FRAMES (10 * 60)
/* The requester waits for the partner's answer this long (offer window plus
 * relay margin) before falling back to the vanilla battle. */
#define COOP_TRAINER_ENCOUNTER_WAIT_FRAMES (12 * 60)
/* After acceptance, the snapshot/ready/start exchange must finish in time. */
#define COOP_TRAINER_ENCOUNTER_START_FRAMES (20 * 60)
/* An aborted co-op battle makes the next sighting of the same trainer, within
 * this window, a vanilla battle so the encounter cannot loop. */
#define COOP_TRAINER_ENCOUNTER_COOLDOWN_FRAMES (60 * 60)

/* Phase 1 trainer classes: everything except gym leaders, Elite Four,
 * champions, rivals, villain admins/leaders/bosses and frontier brains. */
bool8 CoopTrainerEncounter_IsPhaseOneClass(u8 trainerClass);
/* Phase 1 classes plus the Hoenn gym leaders, the Hoenn story battles
 * (sCoopStoryBattles) and their rematches. */
bool8 CoopTrainerEncounter_IsSupportedTrainer(u16 trainerId);
/* Eligible modes: single, no-intro and double for supported trainers; the
 * match-call rematch modes (special BattleSetup_StartRematchBattle) for
 * rematch entries; the continue-script modes only for a Hoenn gym leader's
 * first battle (while the requester lacks that badge) and the story battles. */
bool8 CoopTrainerEncounter_IsEligible(u16 trainerId);
bool8 CoopTrainerEncounter_TryBegin(u16 trainerId);
/* TRUE while this ROM's own parked trainer script is in the co-op battle,
 * i.e. this member requested it. The partner's ROM returns FALSE. */
bool8 CoopTrainerEncounter_IsRequesterBattle(void);
/* Called when a co-op trainer battle returns to the field. Returns TRUE when
 * the battle was an encounter that did not complete: the trainer stays
 * unbeaten, a one-shot cooldown is recorded and the caller must replace the
 * parked trainer script with the release script. */
bool8 CoopTrainerEncounter_OnBattleEnded(bool8 completed);
#if TESTING
/* -1 uses the live presence runtime; 0/1 force the partner-nearby check. */
void CoopTrainerEncounter_TestSetPartnerNearby(s8 nearby);
bool8 CoopTrainerEncounter_TestIsPending(void);
/* Makes the running co-op battle look like the partner's: this ROM did not
 * park a trainer script for it. */
void CoopTrainerEncounter_TestPlayPartner(void);
#endif

#endif
