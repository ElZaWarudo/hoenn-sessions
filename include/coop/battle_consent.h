#ifndef GUARD_COOP_BATTLE_CONSENT_H
#define GUARD_COOP_BATTLE_CONSENT_H

#include "gba/types.h"
#include "coop/battle_protocol.h"

enum CoopBattleKind
{
    COOP_BATTLE_KIND_COOPERATIVE_TRAINER = 1,
    COOP_BATTLE_KIND_FRIENDLY = 2,
};

void CoopBattleConsent_Init(void);
void CoopBattleConsent_Poll(void);
bool8 CoopBattleConsent_Begin(u8 kind);
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

#endif
