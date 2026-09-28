#include "global.h"
#include "coop/battle_consent.h"
#include "coop/battle_runtime.h"
#include "battle.h"
#include "battle_setup.h"
#include "coop/identity.h"
#include "coop/net_bridge.h"
#include "coop/region.h"
#include "event_data.h"
#include "event_object_lock.h"
#include "field_message_box.h"
#include "main.h"
#include "overworld.h"
#include "palette.h"
#include "pokemon.h"
#include "random.h"
#include "script.h"
#include "script_menu.h"
#include "constants/battle.h"
#include "constants/trainers.h"
#include "constants/vars.h"

enum ConsentState
{
    CONSENT_IDLE,
    CONSENT_REQUESTING,
    CONSENT_WAITING,
    CONSENT_OFFER_DEFERRED,
    CONSENT_OFFER_READY,
    CONSENT_RESPONDING,
    CONSENT_ACCEPTED,
    CONSENT_OUTCOME_READY,
    CONSENT_COMMIT_PENDING,
};

enum WallyRequestState
{
    WALLY_REQUEST_NONE,
    WALLY_REQUEST_PENDING,
    WALLY_REQUEST_IN_BATTLE,
    WALLY_REQUEST_WON,
    WALLY_REQUEST_LOST,
    WALLY_REQUEST_COMMIT_PENDING,
};

enum BrockRequestState
{
    BROCK_REQUEST_NONE,
    BROCK_REQUEST_PENDING,
    BROCK_REQUEST_IN_BATTLE,
    BROCK_REQUEST_WON,
    BROCK_REQUEST_LOST,
    BROCK_REQUEST_COMMIT_PENDING,
};

struct ConsentRuntime
{
    u8 battle_id[COOP_BATTLE_ID_SIZE];
    u8 declined_battle_id[COOP_BATTLE_ID_SIZE];
    u8 outcome_battle_id[COOP_BATTLE_ID_SIZE];
    u32 next_nonce;
    u32 request_nonce;
    u32 session_epoch;
    u32 deadline_frame;
    u32 declined_deadline_frame;
    u32 outcome_request_nonce;
    u8 kind;
    u8 state;
    u8 outcome;
    bool8 decision;
    bool8 controls_locked;
    bool8 session_ready;
    bool8 rejection_notice_pending;
    bool8 rejection_notice_visible;
    bool8 startup_attempted;
    u32 rejection_notice_deadline;
    u8 wally_request_state;
    u8 brock_request_state;
    u16 wally_approach_side;
};

static EWRAM_DATA struct ConsentRuntime sConsent = {0};
static void ClearState(void);
extern const u8 EventScript_CoopBattleConsentOffer[];
static const u8 sReserveRejectedText[] = _("Battle request interrupted.\nTry again.");

static void FailPendingTrainerCommit(void)
{
    if (sConsent.state != CONSENT_COMMIT_PENDING)
        return;
    if (sConsent.wally_request_state == WALLY_REQUEST_COMMIT_PENDING)
        sConsent.wally_request_state = WALLY_REQUEST_LOST;
    if (sConsent.brock_request_state == BROCK_REQUEST_COMMIT_PENDING)
        sConsent.brock_request_state = BROCK_REQUEST_LOST;
    sConsent.state = CONSENT_OUTCOME_READY;
    sConsent.deadline_frame = 0;
}

static bool8 IsValidId(const u8 *id)
{
    u8 i;

    for (i = 0; i < COOP_BATTLE_ID_SIZE; i++)
        if (id[i] != 0)
            return TRUE;
    return FALSE;
}

static bool8 IsSafeOverworld(void)
{
    return gMain.callback1 == CB1_Overworld && gMain.callback2 == CB2_Overworld
        && !gPaletteFade.active && !ArePlayerFieldControlsLocked()
        && !ScriptContext_IsEnabled();
}

/* Scripted trainer encounters keep controls locked while waiting. Their
 * scripts must not be running when battle startup begins. */
static bool8 IsSafeTrainerRequestOverworld(void)
{
    return (sConsent.wally_request_state == WALLY_REQUEST_PENDING
         || sConsent.brock_request_state == BROCK_REQUEST_PENDING)
        && gMain.callback1 == CB1_Overworld && gMain.callback2 == CB2_Overworld
        && !gPaletteFade.active && ArePlayerFieldControlsLocked()
        && !ScriptContext_IsEnabled();
}

static void ResumeTrainerScriptIfReady(void)
{
    if (gMain.callback1 != CB1_Overworld || gMain.callback2 != CB2_Overworld
     || gPaletteFade.active || ScriptContext_IsEnabled())
        return;
    if ((sConsent.wally_request_state == WALLY_REQUEST_PENDING
      || sConsent.brock_request_state == BROCK_REQUEST_PENDING
      || sConsent.wally_request_state == WALLY_REQUEST_WON
      || sConsent.wally_request_state == WALLY_REQUEST_LOST
      || sConsent.brock_request_state == BROCK_REQUEST_WON
      || sConsent.brock_request_state == BROCK_REQUEST_LOST)
     && (sConsent.state == CONSENT_IDLE || sConsent.state == CONSENT_OUTCOME_READY))
    {
        ClearState();
        sConsent.wally_request_state = WALLY_REQUEST_NONE;
        sConsent.brock_request_state = BROCK_REQUEST_NONE;
        ScriptContext_Enable();
    }
}

static void ClearState(void)
{
    ScriptMenu_ClearCoopConsentYesNoMarker();
    memset(sConsent.battle_id, 0, sizeof(sConsent.battle_id));
    sConsent.request_nonce = 0;
    sConsent.deadline_frame = 0;
    sConsent.kind = 0;
    sConsent.state = CONSENT_IDLE;
    sConsent.decision = FALSE;
    sConsent.startup_attempted = FALSE;
}

static void CancelResponderOffer(void)
{
    if (sConsent.state == CONSENT_OFFER_READY)
    {
        bool8 canceledMenu;

        /* Remove only this offer's Yes/No task, then discard its waiting
         * script so a late Yes cannot answer an expired reservation. */
        canceledMenu = ScriptMenu_CancelCoopConsentYesNo();
        if (canceledMenu || ArePlayerFieldControlsLocked())
        {
            HideFieldMessageBox();
            ScriptContext_Init();
            if (sConsent.controls_locked && ArePlayerFieldControlsLocked())
                UnlockPlayerFieldControls();
        }
        sConsent.controls_locked = FALSE;
    }
    ClearState();
}

static void ClearOutcome(void)
{
    memset(sConsent.outcome_battle_id, 0, sizeof(sConsent.outcome_battle_id));
    sConsent.outcome_request_nonce = 0;
    sConsent.outcome = 0;
}

static bool8 SendDecision(void)
{
    u8 payload[COOP_BATTLE_JOIN_RESPONSE_SIZE];

    if (!CoopNetBridge_CanSendBattle() || CoopBattleRuntime_HasPendingOutboundReplay()
     || !IsValidId(sConsent.battle_id))
        return FALSE;
    memcpy(payload, sConsent.battle_id, COOP_BATTLE_ID_SIZE);
    payload[COOP_BATTLE_ID_SIZE] = sConsent.decision;
    if (!CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_BATTLE_JOIN_RESPONSE,
                                             payload, sizeof(payload)))
        return FALSE;
    if (sConsent.decision)
        sConsent.state = CONSENT_ACCEPTED;
    else
    {
        memcpy(sConsent.declined_battle_id, sConsent.battle_id, COOP_BATTLE_ID_SIZE);
        sConsent.declined_deadline_frame = gMain.vblankCounter1 + 30 * 60;
        ClearState();
    }
    return TRUE;
}

void CoopBattleConsent_Init(void)
{
    memset(&sConsent, 0, sizeof(sConsent));
    ScriptMenu_ClearCoopConsentYesNoMarker();
    sConsent.next_nonce = 1;
}

void CoopBattleConsent_OnSessionReady(void)
{
    u32 epoch = CoopNetBridge_GetSessionEpoch();
    bool8 resuming = !sConsent.session_ready;

    if (sConsent.session_epoch != 0 && sConsent.session_epoch != epoch)
    {
        bool8 commit_was_pending = sConsent.state == CONSENT_COMMIT_PENDING;
        /* A commit grant is only valid in the lease epoch that produced the
         * terminal battle. Release the waiting script as a failed co-op
         * result if that epoch is replaced. */
        FailPendingTrainerCommit();
        CancelResponderOffer();
        if (commit_was_pending)
        {
            /* CancelResponderOffer also clears the normal consent fields;
             * restore only the terminal failure result for the waiting map
             * script. */
            if (sConsent.wally_request_state == WALLY_REQUEST_NONE)
                sConsent.wally_request_state = WALLY_REQUEST_LOST;
            if (sConsent.brock_request_state == BROCK_REQUEST_NONE)
                sConsent.brock_request_state = BROCK_REQUEST_LOST;
            sConsent.state = CONSENT_OUTCOME_READY;
        }
        ClearOutcome();
        sConsent.rejection_notice_pending = FALSE;
        if (sConsent.rejection_notice_visible)
            HideFieldMessageBox();
        sConsent.rejection_notice_visible = FALSE;
        memset(sConsent.declined_battle_id, 0, COOP_BATTLE_ID_SIZE);
        sConsent.declined_deadline_frame = 0;
    }
    if (resuming && sConsent.state != CONSENT_IDLE
     && sConsent.state != CONSENT_ACCEPTED)
        sConsent.deadline_frame = gMain.vblankCounter1 + 35 * 60;
    sConsent.session_epoch = epoch;
    sConsent.session_ready = TRUE;
}

void CoopBattleConsent_OnTransportLost(void)
{
    sConsent.session_ready = FALSE;
    if (sConsent.state == CONSENT_OFFER_READY)
        CancelResponderOffer();
    if (sConsent.wally_request_state == WALLY_REQUEST_PENDING
     || sConsent.brock_request_state == BROCK_REQUEST_PENDING)
        ClearState();
}

static bool8 BeginRequest(u8 kind, enum CoopRegion region, u16 trainer_ordinal)
{
    u8 payload[COOP_BATTLE_TRAINER_RESERVE_SIZE];
    u16 length;
    u32 nonce;

    if (sConsent.state != CONSENT_IDLE || !sConsent.session_ready
     || !CoopNetBridge_CanSendBattle()
     || CoopBattleRuntime_HasPendingOutboundReplay())
        return FALSE;
    ClearOutcome();
    length = kind == COOP_BATTLE_KIND_COOPERATIVE_TRAINER
        ? COOP_BATTLE_TRAINER_RESERVE_SIZE : COOP_BATTLE_RESERVE_SIZE;
    /* A ROM restart can happen under the same launcher lease. Start each
     * request from the game's timer-seeded RNG so the restarted ROM does not
     * reuse the previous boot's first nonce. The counter also distinguishes
     * repeated draws within one boot. */
    nonce = Random32() ^ sConsent.next_nonce;
    if (nonce == 0)
        nonce = sConsent.next_nonce;
    payload[0] = kind;
    payload[1] = nonce;
    payload[2] = nonce >> 8;
    payload[3] = nonce >> 16;
    payload[4] = nonce >> 24;
    if (kind == COOP_BATTLE_KIND_COOPERATIVE_TRAINER)
    {
        payload[5] = region;
        payload[6] = trainer_ordinal;
        payload[7] = trainer_ordinal >> 8;
    }
    if (!CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_TRAINER_BATTLE_RESERVE,
                                             payload, length))
        return FALSE;
    sConsent.next_nonce++;
    if (sConsent.next_nonce == 0)
        sConsent.next_nonce = 1;
    sConsent.kind = kind;
    sConsent.request_nonce = nonce;
    sConsent.state = CONSENT_REQUESTING;
    sConsent.rejection_notice_pending = FALSE;
    if (sConsent.rejection_notice_visible)
        HideFieldMessageBox();
    sConsent.rejection_notice_visible = FALSE;
    sConsent.deadline_frame = gMain.vblankCounter1 + 35 * 60;
    return TRUE;
}

bool8 CoopBattleConsent_Begin(u8 kind)
{
    if (kind != COOP_BATTLE_KIND_FRIENDLY)
        return FALSE;
    return BeginRequest(kind, COOP_REGION_UNSPECIFIED, 0);
}

bool8 CoopBattleConsent_BeginTrainer(u16 legacy_trainer_id)
{
    enum CoopRegion region;
    u16 ordinal;

    if (!CoopRegion_TryGetActive(&region)
     || !CoopIdentity_ResolveTrainerOrdinal(region, legacy_trainer_id, &ordinal))
        return FALSE;
    return BeginRequest(COOP_BATTLE_KIND_COOPERATIVE_TRAINER, region, ordinal);
}

bool8 CoopBattleConsent_ReceiveReserveRejected(const u8 *payload, u16 length)
{
    u32 request_nonce;

    if (payload == NULL || length != COOP_BATTLE_RESERVE_REJECTED_SIZE)
        return FALSE;
    request_nonce = payload[0] | ((u32)payload[1] << 8)
        | ((u32)payload[2] << 16) | ((u32)payload[3] << 24);
    if (request_nonce == 0 || !sConsent.session_ready
     || sConsent.state != CONSENT_REQUESTING
     || request_nonce != sConsent.request_nonce)
        return FALSE;
    ClearState();
    sConsent.rejection_notice_pending = TRUE;
    return TRUE;
}

bool8 CoopBattleConsent_ReceiveOffer(const u8 *payload, u16 length)
{
    u8 kind;
    u8 role;
    u32 request_nonce;

    if (payload == NULL || length != COOP_BATTLE_JOIN_OFFER_SIZE)
        return FALSE;
    kind = payload[COOP_BATTLE_ID_SIZE];
    role = payload[COOP_BATTLE_ID_SIZE + 1];
    request_nonce = payload[18] | ((u32)payload[19] << 8)
        | ((u32)payload[20] << 16) | ((u32)payload[21] << 24);
    if (!sConsent.session_ready
     || !IsValidId(payload)
     || (kind != COOP_BATTLE_KIND_COOPERATIVE_TRAINER
      && kind != COOP_BATTLE_KIND_FRIENDLY)
     || role > 1 || (role == 0 && request_nonce == 0)
     || (role == 1 && request_nonce != 0))
        return FALSE;
    if (role == 0)
    {
        if (sConsent.state == CONSENT_WAITING
         && memcmp(sConsent.battle_id, payload, COOP_BATTLE_ID_SIZE) == 0
         && sConsent.kind == kind && sConsent.request_nonce == request_nonce)
            return TRUE;
        if (sConsent.state != CONSENT_REQUESTING || sConsent.kind != kind
         || sConsent.request_nonce != request_nonce)
            return FALSE;
        memcpy(sConsent.battle_id, payload, COOP_BATTLE_ID_SIZE);
        sConsent.state = CONSENT_WAITING;
    }
    else
    {
        if ((s32)(sConsent.declined_deadline_frame - gMain.vblankCounter1) > 0
         && memcmp(sConsent.declined_battle_id, payload, COOP_BATTLE_ID_SIZE) == 0)
            return TRUE;
        if ((sConsent.state == CONSENT_OFFER_DEFERRED
          || sConsent.state == CONSENT_OFFER_READY
          || sConsent.state == CONSENT_RESPONDING
          || sConsent.state == CONSENT_ACCEPTED)
         && memcmp(sConsent.battle_id, payload, COOP_BATTLE_ID_SIZE) == 0
         && sConsent.kind == kind)
            return TRUE;
        if (sConsent.state != CONSENT_IDLE && sConsent.state != CONSENT_REQUESTING)
            return FALSE;
        memcpy(sConsent.battle_id, payload, COOP_BATTLE_ID_SIZE);
        sConsent.kind = kind;
        sConsent.state = CONSENT_OFFER_DEFERRED;
    }
    sConsent.deadline_frame = gMain.vblankCounter1 + 30 * 60;
    return TRUE;
}

bool8 CoopBattleConsent_ReceiveOutcome(const u8 *payload, u16 length)
{
    u32 request_nonce;
    u8 outcome;

    if (payload == NULL || length != COOP_BATTLE_CONSENT_OUTCOME_SIZE
     || !IsValidId(payload))
        return FALSE;
    request_nonce = payload[16] | ((u32)payload[17] << 8)
        | ((u32)payload[18] << 16) | ((u32)payload[19] << 24);
    outcome = payload[20];
    if (request_nonce == 0
     || outcome < COOP_BATTLE_CONSENT_ACCEPTED
     || outcome > COOP_BATTLE_CONSENT_EXPIRED)
        return FALSE;

    /* A reconnect may replay the terminal record after the ROM has already
     * observed it. Accept only the exact same battle/nonce/outcome so a stale
     * result can never satisfy a later consent request. */
    if (sConsent.outcome != 0)
    {
        return memcmp(sConsent.outcome_battle_id, payload, COOP_BATTLE_ID_SIZE) == 0
            && sConsent.outcome_request_nonce == request_nonce
            && sConsent.outcome == outcome;
    }
    if (!sConsent.session_ready
     || (sConsent.state != CONSENT_WAITING && sConsent.state != CONSENT_ACCEPTED)
     || !IsValidId(sConsent.battle_id)
     || memcmp(sConsent.battle_id, payload, COOP_BATTLE_ID_SIZE) != 0
     || sConsent.request_nonce != request_nonce)
        return FALSE;

    memcpy(sConsent.outcome_battle_id, payload, COOP_BATTLE_ID_SIZE);
    sConsent.outcome_request_nonce = request_nonce;
    sConsent.outcome = outcome;
    if (outcome != COOP_BATTLE_CONSENT_ACCEPTED)
    {
        sConsent.state = CONSENT_OUTCOME_READY;
        sConsent.deadline_frame = 0;
    }
    else
    {
        sConsent.state = CONSENT_ACCEPTED;
    }
    return TRUE;
}

bool8 CoopBattleConsent_ReceiveAbort(const u8 *payload, u16 length)
{
    if (payload == NULL || length != COOP_BATTLE_ABORT_SIZE
     || !IsValidId(payload)
     || payload[COOP_BATTLE_ID_SIZE] < COOP_BATTLE_ABORT_CANCELED
     || payload[COOP_BATTLE_ID_SIZE] > COOP_BATTLE_ABORT_UNAVAILABLE)
        return FALSE;
    if (memcmp(sConsent.battle_id, payload, COOP_BATTLE_ID_SIZE) == 0)
    {
        if (sConsent.state == CONSENT_COMMIT_PENDING)
        {
            /* The terminal battle is already out of the engine. An abort is
             * a definitive co-op failure; never let the script fall through
             * to its vanilla solo-battle branch. */
            FailPendingTrainerCommit();
            return TRUE;
        }
        /* Acceptance is only a gate for starting the future battle. Once the
         * reservation is aborted, do not leave a stale accepted result for a
         * later script query. Declined/expired outcomes remain queryable so
         * the player can be told why the consent did not proceed. */
        bool8 clear_accepted = sConsent.outcome == COOP_BATTLE_CONSENT_ACCEPTED;
        CancelResponderOffer();
        if (clear_accepted)
            ClearOutcome();
    }
    if (memcmp(sConsent.declined_battle_id, payload, COOP_BATTLE_ID_SIZE) == 0)
        memset(sConsent.declined_battle_id, 0, COOP_BATTLE_ID_SIZE);
    return TRUE;
}

bool8 CoopBattleConsent_IsCurrentBattle(const u8 *battle_id)
{
    return battle_id != NULL && sConsent.session_ready
        && (sConsent.state == CONSENT_WAITING || sConsent.state == CONSENT_ACCEPTED)
        && IsValidId(sConsent.battle_id)
        && memcmp(sConsent.battle_id, battle_id, COOP_BATTLE_ID_SIZE) == 0;
}

bool8 CoopBattleConsent_CopyCurrentBattleId(u8 *battle_id, u16 capacity)
{
    if (battle_id == NULL || capacity < COOP_BATTLE_ID_SIZE
     || !CoopBattleConsent_IsCurrentBattle(sConsent.battle_id))
        return FALSE;
    memcpy(battle_id, sConsent.battle_id, COOP_BATTLE_ID_SIZE);
    return TRUE;
}

u8 CoopBattleConsent_GetOutcome(void)
{
    return sConsent.session_ready ? sConsent.outcome : 0;
}

bool8 CoopBattleConsent_GetOutcomeRecord(u8 *battle_id, u32 *request_nonce, u8 *outcome)
{
    if (!sConsent.session_ready || sConsent.outcome == 0
     || battle_id == NULL || request_nonce == NULL || outcome == NULL)
        return FALSE;
    memcpy(battle_id, sConsent.outcome_battle_id, COOP_BATTLE_ID_SIZE);
    *request_nonce = sConsent.outcome_request_nonce;
    *outcome = sConsent.outcome;
    return TRUE;
}

void CoopBattleConsent_Poll(void)
{
    if (sConsent.rejection_notice_visible)
    {
        if ((s32)(sConsent.rejection_notice_deadline - gMain.vblankCounter1) <= 0
         || gMain.callback1 != CB1_Overworld || gMain.callback2 != CB2_Overworld
         || ScriptContext_IsEnabled())
        {
            HideFieldMessageBox();
            sConsent.rejection_notice_visible = FALSE;
        }
    }
    if (sConsent.rejection_notice_pending && IsSafeOverworld()
     && IsFieldMessageBoxHidden() && ShowFieldMessage(sReserveRejectedText))
    {
        sConsent.rejection_notice_pending = FALSE;
        sConsent.rejection_notice_visible = TRUE;
        sConsent.rejection_notice_deadline = gMain.vblankCounter1 + 180;
    }
    /* A Yes/No box pauses its script with controls still locked. Only the
     * script ending or being abandoned releases that lock. */
    if (sConsent.state == CONSENT_OFFER_READY && !ScriptContext_IsEnabled()
     && !ArePlayerFieldControlsLocked())
        CancelResponderOffer();
    if (sConsent.controls_locked && sConsent.state != CONSENT_OFFER_READY
     && !ScriptContext_IsEnabled()
     && gMain.callback1 == CB1_Overworld && gMain.callback2 == CB2_Overworld)
    {
        if (ArePlayerFieldControlsLocked())
            UnlockPlayerFieldControls();
        sConsent.controls_locked = FALSE;
    }
    if (sConsent.state == CONSENT_COMMIT_PENDING
     && CoopBattleRuntime_HasAcceptedBattleCommit())
    {
        if (sConsent.wally_request_state == WALLY_REQUEST_COMMIT_PENDING)
            sConsent.wally_request_state = WALLY_REQUEST_WON;
        if (sConsent.brock_request_state == BROCK_REQUEST_COMMIT_PENDING)
            sConsent.brock_request_state = BROCK_REQUEST_WON;
        sConsent.state = CONSENT_OUTCOME_READY;
    }
    ResumeTrainerScriptIfReady();
    if (sConsent.state == CONSENT_IDLE || sConsent.state == CONSENT_OUTCOME_READY)
        return;
    if (!sConsent.session_ready)
        return;
    /* A queued acceptance remains bound to this battle until an abort or a
     * new session. The offer deadline only limits an unanswered offer. */
    if (sConsent.state != CONSENT_ACCEPTED
     && (s32)(sConsent.deadline_frame - gMain.vblankCounter1) <= 0)
    {
        CancelResponderOffer();
        ResumeTrainerScriptIfReady();
        return;
    }
    if (sConsent.state == CONSENT_OFFER_DEFERRED && IsSafeOverworld())
    {
        LockPlayerFieldControls();
        sConsent.controls_locked = TRUE;
        sConsent.state = CONSENT_OFFER_READY;
        ScriptContext_SetupScript(EventScript_CoopBattleConsentOffer);
    }
    if (sConsent.state == CONSENT_RESPONDING)
        (void)SendDecision();
    if (sConsent.state == CONSENT_ACCEPTED
     && sConsent.kind == COOP_BATTLE_KIND_COOPERATIVE_TRAINER
     && (IsSafeOverworld() || IsSafeTrainerRequestOverworld()))
    {
        if (!CoopBattleRuntime_HasManifest())
            (void)CoopBattleRuntime_PollLocalSnapshot(sConsent.battle_id,
                gParties[B_TRAINER_0], gPartiesCount[B_TRAINER_0]);
        else
            (void)CoopBattleRuntime_TrySendReady(sConsent.battle_id,
                gParties[B_TRAINER_0], gPartiesCount[B_TRAINER_0]);
        if (CoopBattleRuntime_IsStartReleased() && !sConsent.startup_attempted)
        {
            sConsent.startup_attempted = TRUE;
            if (BattleSetup_StartCoopTrainerBattle())
            {
                if (sConsent.wally_request_state == WALLY_REQUEST_PENDING)
                    sConsent.wally_request_state = WALLY_REQUEST_IN_BATTLE;
                if (sConsent.brock_request_state == BROCK_REQUEST_PENDING)
                    sConsent.brock_request_state = BROCK_REQUEST_IN_BATTLE;
            }
            else
                (void)CoopBattleRuntime_RequestAbort(COOP_BATTLE_ABORT_UNAVAILABLE);
        }
    }
}

void Special_CoopBattleConsentGetOffer(void)
{
    gSpecialVar_Result = sConsent.state == CONSENT_OFFER_READY ? sConsent.kind : 0;
}

void Special_CoopBattleConsentMarkPrompt(void)
{
    if (sConsent.state == CONSENT_OFFER_READY)
        ScriptMenu_MarkNextYesNoAsCoopConsent();
}

void Special_CoopBattleConsentGetOutcome(void)
{
    gSpecialVar_Result = CoopBattleConsent_GetOutcome();
}

void Special_CoopBattleConsentRespond(void)
{
    if (sConsent.state != CONSENT_OFFER_READY)
    {
        gSpecialVar_Result = FALSE;
        return;
    }
    sConsent.decision = gSpecialVar_0x8004 != 0;
    sConsent.state = CONSENT_RESPONDING;
    gSpecialVar_Result = SendDecision();
}

void Special_CoopBattleConsentBeginWally(void)
{
    /* The co-op result still needs a save checkpoint before and after the
     * battle. Route the story encounter through its normal battle until the
     * complete save handoff can be finalized without a time limit. */
    gSpecialVar_Result = FALSE;
}

void Special_CoopBattleConsentGetWallyResult(void)
{
    if (sConsent.wally_request_state == WALLY_REQUEST_WON)
        gSpecialVar_Result = 1;
    else if (sConsent.wally_request_state == WALLY_REQUEST_LOST
          || sConsent.wally_request_state == WALLY_REQUEST_COMMIT_PENDING)
        gSpecialVar_Result = 2;
    else
        gSpecialVar_Result = 0;
    VarSet(VAR_0x8008, sConsent.wally_approach_side);
    sConsent.wally_request_state = WALLY_REQUEST_NONE;
    CoopBattleRuntime_ForgetTerminalCommit();
}

void Special_CoopBattleConsentBeginBrock(void)
{
    /* Brock's badge and TM save transition has no server commit validator yet.
     * Keep the normal gym battle available until that ledger path exists. */
    gSpecialVar_Result = FALSE;
}

void Special_CoopBattleConsentGetBrockResult(void)
{
    if (sConsent.brock_request_state == BROCK_REQUEST_WON)
        gSpecialVar_Result = 1;
    else if (sConsent.brock_request_state == BROCK_REQUEST_LOST
          || sConsent.brock_request_state == BROCK_REQUEST_COMMIT_PENDING)
        gSpecialVar_Result = 2;
    else
        gSpecialVar_Result = 0;
    sConsent.brock_request_state = BROCK_REQUEST_NONE;
    CoopBattleRuntime_ForgetTerminalCommit();
}

bool8 CoopBattleConsent_OnTrainerBattleEnded(bool8 completed, u8 battle_outcome)
{
    if (sConsent.wally_request_state == WALLY_REQUEST_IN_BATTLE)
    {
        if (completed && battle_outcome == B_OUTCOME_WON
         && !CoopBattleRuntime_HasAcceptedBattleCommit())
        {
            sConsent.wally_request_state = WALLY_REQUEST_COMMIT_PENDING;
            sConsent.state = CONSENT_COMMIT_PENDING;
            sConsent.deadline_frame = 0;
            return TRUE;
        }
        sConsent.wally_request_state = completed && battle_outcome == B_OUTCOME_WON
            ? WALLY_REQUEST_WON : WALLY_REQUEST_LOST;
    }
    else if (sConsent.brock_request_state == BROCK_REQUEST_IN_BATTLE)
    {
        if (completed && battle_outcome == B_OUTCOME_WON
         && !CoopBattleRuntime_HasAcceptedBattleCommit())
        {
            sConsent.brock_request_state = BROCK_REQUEST_COMMIT_PENDING;
            sConsent.state = CONSENT_COMMIT_PENDING;
            sConsent.deadline_frame = 0;
            return TRUE;
        }
        sConsent.brock_request_state = completed && battle_outcome == B_OUTCOME_WON
            ? BROCK_REQUEST_WON : BROCK_REQUEST_LOST;
    }
    else
        return FALSE;
    ClearState();
    return TRUE;
}

void CoopBattleConsent_OnTrainerWhiteout(void)
{
    if (sConsent.wally_request_state == WALLY_REQUEST_LOST)
        sConsent.wally_request_state = WALLY_REQUEST_NONE;
    if (sConsent.brock_request_state == BROCK_REQUEST_LOST)
        sConsent.brock_request_state = BROCK_REQUEST_NONE;
}
