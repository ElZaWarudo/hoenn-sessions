#include "global.h"
#include "coop/battle_consent.h"
#include "coop/battle_runtime.h"
#include "coop/friendly_battle.h"
#include "battle.h"
#include "battle_setup.h"
#include "coop/identity.h"
#include "coop/net_bridge.h"
#include "coop/presence_runtime.h"
#include "coop/region.h"
#include "coop/trainer_rewards.h"
#include "battle_pyramid.h"
#include "data.h"
#include "event_data.h"
#include "event_object_lock.h"
#include "field_message_box.h"
#include "follower_npc.h"
#include "main.h"
#include "overworld.h"
#include "palette.h"
#include "pokemon.h"
#include "random.h"
#include "script.h"
#include "script_menu.h"
#include "trainer_hill.h"
#include "trainer_see.h"
#include "constants/battle.h"
#include "constants/battle_pyramid.h"
#include "constants/battle_setup.h"
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

/* A parked dotrainerbattle waiting for the co-op decision. */
enum TrainerEncounterState
{
    ENCOUNTER_NONE,
    ENCOUNTER_WAITING,        // reservation out, partner has not accepted
    ENCOUNTER_ACCEPTED,       // accepted; snapshot/ready/start in progress
    ENCOUNTER_IN_BATTLE,      // co-op battle running
    ENCOUNTER_RESUME_VANILLA, // fall back once the parked script is safe
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
    /* The encounter fields fill the padding after startup_attempted; only
     * the cooldown deadline grows the struct (4 bytes). */
    u8 encounter_state;
    u16 cooldown_trainer_id;
    u32 rejection_notice_deadline;
    u8 wally_request_state;
    u8 brock_request_state;
    u16 wally_approach_side;
    u32 cooldown_deadline;
    /* The rules of the friendly offer on screen (responder). */
    u8 offer_rules[COOP_BATTLE_FRIENDLY_RULES_SIZE];
};

static EWRAM_DATA struct ConsentRuntime sConsent = {0};
static void ClearState(void);
extern const u8 EventScript_CoopBattleConsentOffer[];
static const u8 sReserveRejectedText[] = _("Battle request interrupted.\nTry again.");
static const u8 sWaitingForPartnerText[] = _("Waiting for partner…");

#if TESTING
/* 0 = live presence, 1 = forced far, 2 = forced near. */
static u8 sTestPartnerNearby;
#endif

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
         || sConsent.brock_request_state == BROCK_REQUEST_PENDING
         || sConsent.encounter_state == ENCOUNTER_ACCEPTED)
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

/* An accepted battle must start in time; otherwise the reservation is
 * called off locally instead of holding the consent forever. */
static void EnterAccepted(void)
{
    sConsent.state = CONSENT_ACCEPTED;
    sConsent.startup_attempted = FALSE;
    sConsent.deadline_frame = gMain.vblankCounter1
        + (sConsent.kind == COOP_BATTLE_KIND_FRIENDLY
           ? COOP_FRIENDLY_START_FRAMES : COOP_TRAINER_ENCOUNTER_START_FRAMES);
}

static bool8 IsFriendlyConsent(void)
{
    return sConsent.kind == COOP_BATTLE_KIND_FRIENDLY && sConsent.state != CONSENT_IDLE;
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
        EnterAccepted();
    else
    {
        memcpy(sConsent.declined_battle_id, sConsent.battle_id, COOP_BATTLE_ID_SIZE);
        sConsent.declined_deadline_frame = gMain.vblankCounter1 + 30 * 60;
        ClearState();
    }
    return TRUE;
}

/* An unanswered responder offer declines itself: the server resolves the
 * requester at once instead of holding the reservation to its own expiry.
 * A full queue leaves the server's expiry as the fallback. */
static void ExpireResponderOffer(void)
{
    u8 payload[COOP_BATTLE_JOIN_RESPONSE_SIZE];
    bool8 unanswered = sConsent.state == CONSENT_OFFER_DEFERRED
        || sConsent.state == CONSENT_OFFER_READY;

    memcpy(payload, sConsent.battle_id, COOP_BATTLE_ID_SIZE);
    payload[COOP_BATTLE_ID_SIZE] = FALSE;
    CancelResponderOffer();
    if (!unanswered || !IsValidId(payload) || !sConsent.session_ready
     || !CoopNetBridge_CanSendBattle() || CoopBattleRuntime_HasPendingOutboundReplay()
     || !CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_BATTLE_JOIN_RESPONSE,
                                             payload, sizeof(payload)))
        return;
    memcpy(sConsent.declined_battle_id, payload, COOP_BATTLE_ID_SIZE);
    sConsent.declined_deadline_frame = gMain.vblankCounter1 + 30 * 60;
}

static bool8 IsEncounterRequestActive(void)
{
    return (sConsent.encounter_state == ENCOUNTER_WAITING
         || sConsent.encounter_state == ENCOUNTER_ACCEPTED)
        && sConsent.kind == COOP_BATTLE_KIND_COOPERATIVE_TRAINER
        && (sConsent.state == CONSENT_REQUESTING
         || sConsent.state == CONSENT_WAITING
         || sConsent.state == CONSENT_ACCEPTED);
}

/* An abandoned request keeps its nonce while the consent is idle so a late
 * offer, outcome or rejection for it is absorbed instead of being reported
 * as a protocol error. A new request or session replaces it. */
static bool8 IsAbandonedRequest(u32 request_nonce)
{
    return sConsent.state == CONSENT_IDLE && sConsent.request_nonce != 0
        && request_nonce == sConsent.request_nonce;
}

/* Cancels a reservation this ROM no longer waits for. Before a manifest the
 * launcher still tracks the reservation by its ID. */
static void SendRequesterCancel(const u8 *battle_id)
{
    u8 payload[COOP_BATTLE_MANIFEST_SIZE];

    if (!IsValidId(battle_id) || !sConsent.session_ready
     || !CoopNetBridge_CanSendBattle())
        return;
    if (CoopBattleRuntime_GetManifest(payload, sizeof(payload))
     && memcmp(payload, battle_id, COOP_BATTLE_ID_SIZE) == 0)
    {
        (void)CoopBattleRuntime_RequestAbort(COOP_BATTLE_ABORT_CANCELED);
        return;
    }
    memcpy(payload, battle_id, COOP_BATTLE_ID_SIZE);
    payload[COOP_BATTLE_ID_SIZE] = COOP_BATTLE_ABORT_CANCELED;
    (void)CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_BATTLE_ABORT_REQUEST,
                                             payload, COOP_BATTLE_ABORT_REQUEST_SIZE);
}

/* Drops this ROM's side of an encounter and schedules the vanilla battle.
 * A crossing partner offer (responder states) is left alone: it expires on
 * its own and never blocks the parked trainer script. */
static void FallBackEncounter(bool8 cancel)
{
    if (IsEncounterRequestActive()
     || (sConsent.state == CONSENT_OUTCOME_READY
      && sConsent.kind == COOP_BATTLE_KIND_COOPERATIVE_TRAINER))
    {
        u32 nonce = sConsent.request_nonce;

        if (cancel)
            SendRequesterCancel(sConsent.battle_id);
        ClearState();
        sConsent.request_nonce = nonce;
    }
    if (sConsent.outcome == COOP_BATTLE_CONSENT_ACCEPTED)
        ClearOutcome();
    sConsent.rejection_notice_pending = FALSE;
    sConsent.encounter_state = ENCOUNTER_RESUME_VANILLA;
}

/* The trainer script is parked after dotrainerbattle: controls locked, the
 * script context stopped and the field running. */
static bool8 IsParkedEncounterOverworld(void)
{
    return gMain.callback1 == CB1_Overworld && gMain.callback2 == CB2_Overworld
        && !gPaletteFade.active && ArePlayerFieldControlsLocked()
        && !ScriptContext_IsEnabled();
}

static void PollTrainerEncounter(void)
{
    if (sConsent.encounter_state == ENCOUNTER_WAITING
     || sConsent.encounter_state == ENCOUNTER_ACCEPTED)
    {
        if (!IsEncounterRequestActive() || !sConsent.session_ready)
        {
            /* Declined, expired, rejected, aborted, superseded or offline. */
            FallBackEncounter(FALSE);
        }
        else if ((s32)(sConsent.deadline_frame - gMain.vblankCounter1) <= 0)
        {
            FallBackEncounter(TRUE);
        }
        else if (sConsent.encounter_state == ENCOUNTER_WAITING
              && sConsent.state == CONSENT_ACCEPTED)
        {
            sConsent.encounter_state = ENCOUNTER_ACCEPTED;
            sConsent.deadline_frame = gMain.vblankCounter1
                + COOP_TRAINER_ENCOUNTER_START_FRAMES;
        }
    }
    if (sConsent.encounter_state == ENCOUNTER_RESUME_VANILLA
     && IsParkedEncounterOverworld())
    {
        sConsent.encounter_state = ENCOUNTER_NONE;
        HideFieldMessageBox();
        /* The parked parameters are untouched, so their mode still says
         * which vanilla entry the script was about to run. */
        if (IsRematchBattleMode(GetTrainerBattleMode()))
            BattleSetup_StartVanillaRematchBattle();
        else
            BattleSetup_StartVanillaTrainerBattle();
    }
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
        if (IsFriendlyConsent())
            CoopFriendly_End(COOP_FRIENDLY_RESULT_UNAVAILABLE);
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
    if (sConsent.encounter_state == ENCOUNTER_WAITING
     || sConsent.encounter_state == ENCOUNTER_ACCEPTED)
        FallBackEncounter(FALSE);
    if (sConsent.state == CONSENT_OFFER_READY)
        CancelResponderOffer();
    if (sConsent.wally_request_state == WALLY_REQUEST_PENDING
     || sConsent.brock_request_state == BROCK_REQUEST_PENDING)
        ClearState();
}

static bool8 BeginRequest(u8 kind, enum CoopRegion region, u16 trainer_ordinal,
                          const struct CoopBattleFriendlyRules *rules)
{
    u8 payload[COOP_BATTLE_RESERVE_SIZE];
    u16 length;
    u32 nonce;

    if (sConsent.state != CONSENT_IDLE || !sConsent.session_ready
     || !CoopNetBridge_CanSendBattle()
     || CoopBattleRuntime_HasPendingOutboundReplay())
        return FALSE;
    if (kind == COOP_BATTLE_KIND_FRIENDLY && !CoopBattleRuntime_IsValidFriendlyRules(rules))
        return FALSE;
    ClearOutcome();
    length = COOP_BATTLE_RESERVE_SIZE;
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
    else
    {
        CoopBattleRuntime_EncodeFriendlyRules(rules, &payload[5]);
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
    struct CoopBattleFriendlyRules rules;

    if (kind != COOP_BATTLE_KIND_FRIENDLY)
        return FALSE;
    CoopFriendly_GetRules(&rules);
    return BeginRequest(kind, COOP_REGION_UNSPECIFIED, 0, &rules);
}

bool8 CoopBattleConsent_BeginFriendly(const struct CoopBattleFriendlyRules *rules)
{
    return BeginRequest(COOP_BATTLE_KIND_FRIENDLY, COOP_REGION_UNSPECIFIED, 0, rules);
}

bool8 CoopBattleConsent_IsIdle(void)
{
    return sConsent.state == CONSENT_IDLE && sConsent.session_ready
        && sConsent.encounter_state == ENCOUNTER_NONE
        && sConsent.wally_request_state == WALLY_REQUEST_NONE
        && sConsent.brock_request_state == BROCK_REQUEST_NONE;
}

bool8 CoopBattleConsent_GetOfferRules(struct CoopBattleFriendlyRules *rules)
{
    if (sConsent.kind != COOP_BATTLE_KIND_FRIENDLY || sConsent.request_nonce != 0
     || (sConsent.state != CONSENT_OFFER_READY && sConsent.state != CONSENT_RESPONDING
      && sConsent.state != CONSENT_ACCEPTED))
        return FALSE;
    return CoopBattleRuntime_DecodeFriendlyRules(sConsent.offer_rules, rules);
}

/* The partner said Yes: the friendly script now owns the field lock it
 * started under and releases it itself. */
bool8 CoopBattleConsent_TakeFriendlyPromptLock(void)
{
    if (sConsent.kind != COOP_BATTLE_KIND_FRIENDLY
     || (sConsent.state != CONSENT_RESPONDING && sConsent.state != CONSENT_ACCEPTED))
        return FALSE;
    sConsent.controls_locked = FALSE;
    return TRUE;
}

/* Calls off this ROM's friendly reservation before the battle starts. A
 * request without a battle ID stays absorbable as an abandoned nonce. */
void CoopBattleConsent_CancelFriendly(void)
{
    u32 nonce = sConsent.request_nonce;

    if (!IsFriendlyConsent() || sConsent.startup_attempted)
        return;
    if (IsValidId(sConsent.battle_id))
        SendRequesterCancel(sConsent.battle_id);
    ClearState();
    sConsent.request_nonce = nonce;
    ClearOutcome();
}

void CoopBattleConsent_OnFriendlyBattleEnded(void)
{
    if (sConsent.kind != COOP_BATTLE_KIND_FRIENDLY)
        return;
    ClearState();
    ClearOutcome();
}

bool8 CoopBattleConsent_BeginTrainer(u16 legacy_trainer_id)
{
    enum CoopRegion region;
    u16 ordinal;

    if (!CoopRegion_TryGetActive(&region)
     || !CoopIdentity_ResolveTrainerOrdinal(region, legacy_trainer_id, &ordinal))
        return FALSE;
    return BeginRequest(COOP_BATTLE_KIND_COOPERATIVE_TRAINER, region, ordinal, NULL);
}

bool8 CoopBattleConsent_ReceiveReserveRejected(const u8 *payload, u16 length)
{
    u32 request_nonce;

    if (payload == NULL || length != COOP_BATTLE_RESERVE_REJECTED_SIZE)
        return FALSE;
    request_nonce = payload[0] | ((u32)payload[1] << 8)
        | ((u32)payload[2] << 16) | ((u32)payload[3] << 24);
    if (request_nonce != 0 && IsAbandonedRequest(request_nonce))
        return TRUE;
    if (request_nonce == 0 || !sConsent.session_ready
     || sConsent.state != CONSENT_REQUESTING
     || request_nonce != sConsent.request_nonce)
        return FALSE;
    if (sConsent.kind == COOP_BATTLE_KIND_FRIENDLY)
    {
        ClearState();
        CoopFriendly_End(COOP_FRIENDLY_RESULT_UNAVAILABLE);
        return TRUE;
    }
    ClearState();
    /* A trainer encounter falls back to its vanilla battle silently. */
    if (sConsent.encounter_state == ENCOUNTER_NONE)
        sConsent.rejection_notice_pending = TRUE;
    return TRUE;
}

bool8 CoopBattleConsent_ReceiveOffer(const u8 *payload, u16 length)
{
    u8 kind;
    u8 role;
    u32 request_nonce;
    const u8 *rulesBytes;
    struct CoopBattleFriendlyRules rules = {0};
    struct CoopBattleFriendlyRules ownRules;

    if (payload == NULL || length != COOP_BATTLE_JOIN_OFFER_SIZE)
        return FALSE;
    kind = payload[COOP_BATTLE_ID_SIZE];
    role = payload[COOP_BATTLE_ID_SIZE + 1];
    request_nonce = payload[18] | ((u32)payload[19] << 8)
        | ((u32)payload[20] << 16) | ((u32)payload[21] << 24);
    rulesBytes = &payload[COOP_BATTLE_JOIN_OFFER_RULES_OFFSET];
    if (!sConsent.session_ready
     || !IsValidId(payload)
     || (kind != COOP_BATTLE_KIND_COOPERATIVE_TRAINER
      && kind != COOP_BATTLE_KIND_FRIENDLY)
     || role > 1 || (role == 0 && request_nonce == 0)
     || (role == 1 && request_nonce != 0))
        return FALSE;
    if (kind == COOP_BATTLE_KIND_FRIENDLY
        ? !CoopBattleRuntime_DecodeFriendlyRules(rulesBytes, &rules)
        : (rulesBytes[0] | rulesBytes[1] | rulesBytes[2]) != 0)
        return FALSE;
    if (role == 0)
    {
        if (IsAbandonedRequest(request_nonce))
        {
            /* The requester stopped waiting; release the partner now. */
            SendRequesterCancel(payload);
            return TRUE;
        }
        if (sConsent.state == CONSENT_WAITING
         && memcmp(sConsent.battle_id, payload, COOP_BATTLE_ID_SIZE) == 0
         && sConsent.kind == kind && sConsent.request_nonce == request_nonce)
            return TRUE;
        if (sConsent.state != CONSENT_REQUESTING || sConsent.kind != kind
         || sConsent.request_nonce != request_nonce)
            return FALSE;
        if (kind == COOP_BATTLE_KIND_FRIENDLY)
        {
            /* The server must echo the challenge this ROM sent. */
            CoopFriendly_GetRules(&ownRules);
            if (ownRules.format != rules.format || ownRules.level_mode != rules.level_mode
             || ownRules.count != rules.count)
                return FALSE;
        }
        memcpy(sConsent.battle_id, payload, COOP_BATTLE_ID_SIZE);
        sConsent.state = CONSENT_WAITING;
        /* A parked trainer encounter keeps the deadline it started with. */
        if (sConsent.encounter_state != ENCOUNTER_NONE)
            return TRUE;
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
        sConsent.request_nonce = 0;
        memcpy(sConsent.offer_rules, rulesBytes, COOP_BATTLE_FRIENDLY_RULES_SIZE);
        /* A trainer encounter is waiting in front of the requester's
         * trainer, so its prompt is short. A friendly prompt must be answered
         * inside the server's 30 s reservation window. */
        if (kind == COOP_BATTLE_KIND_COOPERATIVE_TRAINER)
        {
            sConsent.deadline_frame = gMain.vblankCounter1
                + COOP_TRAINER_ENCOUNTER_OFFER_FRAMES;
            return TRUE;
        }
        sConsent.deadline_frame = gMain.vblankCounter1 + COOP_FRIENDLY_OFFER_FRAMES;
        return TRUE;
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
    if (IsAbandonedRequest(request_nonce))
    {
        if (outcome == COOP_BATTLE_CONSENT_ACCEPTED)
            SendRequesterCancel(payload);
        return TRUE;
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
        if (sConsent.kind == COOP_BATTLE_KIND_FRIENDLY)
        {
            /* The outcome record stays for an exact replay; the consent is
             * free again and the waiting challenger is told why. */
            ClearState();
            CoopFriendly_End(outcome == COOP_BATTLE_CONSENT_DECLINED
                             ? COOP_FRIENDLY_RESULT_DECLINED
                             : COOP_FRIENDLY_RESULT_NO_ANSWER);
            return TRUE;
        }
        sConsent.state = CONSENT_OUTCOME_READY;
        sConsent.deadline_frame = 0;
    }
    else if (sConsent.state != CONSENT_ACCEPTED)
    {
        EnterAccepted();
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
        bool8 friendly = IsFriendlyConsent();
        CancelResponderOffer();
        if (clear_accepted)
            ClearOutcome();
        if (friendly)
            CoopFriendly_End(payload[COOP_BATTLE_ID_SIZE] == COOP_BATTLE_ABORT_CANCELED
                             ? COOP_FRIENDLY_RESULT_WITHDRAWN
                             : payload[COOP_BATTLE_ID_SIZE] == COOP_BATTLE_ABORT_EXPIRED
                               ? COOP_FRIENDLY_RESULT_NO_ANSWER
                               : COOP_FRIENDLY_RESULT_UNAVAILABLE);
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

/* The friendly script waits with the field locked (parked) once the team
 * is picked; only then does this ROM send its team as the snapshot. */
static bool8 IsParkedFriendlyOverworld(void)
{
    return CoopFriendly_IsWaiting()
        && gMain.callback1 == CB1_Overworld && gMain.callback2 == CB2_Overworld
        && !gPaletteFade.active && ArePlayerFieldControlsLocked()
        && !ScriptContext_IsEnabled();
}

static void PollFriendlyStart(void)
{
    struct Pokemon team[PARTY_SIZE];
    u8 count;

    if (!IsParkedFriendlyOverworld())
        return;
    count = CoopFriendly_BuildTeam(team);
    if (count == 0)
    {
        CoopBattleConsent_CancelFriendly();
        CoopFriendly_End(COOP_FRIENDLY_RESULT_UNAVAILABLE);
        return;
    }
    if (!CoopBattleRuntime_HasManifest())
        (void)CoopBattleRuntime_PollLocalSnapshot(sConsent.battle_id, team, count);
    else
        (void)CoopBattleRuntime_TrySendReady(sConsent.battle_id, team, count);
    if (!CoopBattleRuntime_IsStartReleased() || sConsent.startup_attempted)
        return;
    sConsent.startup_attempted = TRUE;
    HideFieldMessageBox();
    if (BattleSetup_StartCoopFriendlyBattle(team, count))
    {
        CoopFriendly_OnBattleStarted();
        return;
    }
    (void)CoopBattleRuntime_RequestAbort(COOP_BATTLE_ABORT_UNAVAILABLE);
    ClearState();
    ClearOutcome();
    CoopFriendly_End(COOP_FRIENDLY_RESULT_UNAVAILABLE);
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
    PollTrainerEncounter();
    if (sConsent.state == CONSENT_IDLE || sConsent.state == CONSENT_OUTCOME_READY)
        return;
    if (!sConsent.session_ready)
        return;
    /* An unanswered offer or request expires at its deadline. */
    if (sConsent.state != CONSENT_ACCEPTED
     && (s32)(sConsent.deadline_frame - gMain.vblankCounter1) <= 0)
    {
        if (sConsent.kind == COOP_BATTLE_KIND_FRIENDLY
         && (sConsent.state == CONSENT_REQUESTING || sConsent.state == CONSENT_WAITING))
        {
            /* The challenger stops waiting; the late answer is absorbed. */
            CoopBattleConsent_CancelFriendly();
            CoopFriendly_End(COOP_FRIENDLY_RESULT_NO_ANSWER);
            return;
        }
        ExpireResponderOffer();
        ResumeTrainerScriptIfReady();
        return;
    }
    /* An accepted battle that has not started by its deadline is called off
     * (the server's accepted window is ten minutes). A parked trainer
     * encounter keeps its own deadline in PollTrainerEncounter. */
    if (sConsent.state == CONSENT_ACCEPTED && !sConsent.startup_attempted
     && sConsent.encounter_state == ENCOUNTER_NONE
     && sConsent.wally_request_state == WALLY_REQUEST_NONE
     && sConsent.brock_request_state == BROCK_REQUEST_NONE
     && !CoopBattleRuntime_IsEngineActive()
     && (s32)(sConsent.deadline_frame - gMain.vblankCounter1) <= 0)
    {
        if (sConsent.kind == COOP_BATTLE_KIND_FRIENDLY)
        {
            CoopBattleConsent_CancelFriendly();
            CoopFriendly_End(COOP_FRIENDLY_RESULT_TIMED_OUT);
        }
        else
        {
            SendRequesterCancel(sConsent.battle_id);
            ClearState();
            if (sConsent.outcome == COOP_BATTLE_CONSENT_ACCEPTED)
                ClearOutcome();
        }
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
     && sConsent.kind == COOP_BATTLE_KIND_FRIENDLY)
        PollFriendlyStart();
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
            if (sConsent.encounter_state == ENCOUNTER_ACCEPTED)
                HideFieldMessageBox();
            if (BattleSetup_StartCoopTrainerBattle())
            {
                if (sConsent.wally_request_state == WALLY_REQUEST_PENDING)
                    sConsent.wally_request_state = WALLY_REQUEST_IN_BATTLE;
                if (sConsent.brock_request_state == BROCK_REQUEST_PENDING)
                    sConsent.brock_request_state = BROCK_REQUEST_IN_BATTLE;
                if (sConsent.encounter_state == ENCOUNTER_ACCEPTED)
                    sConsent.encounter_state = ENCOUNTER_IN_BATTLE;
            }
            else
            {
                (void)CoopBattleRuntime_RequestAbort(COOP_BATTLE_ABORT_UNAVAILABLE);
                if (sConsent.encounter_state == ENCOUNTER_ACCEPTED)
                {
                    FallBackEncounter(FALSE);
                    PollTrainerEncounter();
                }
            }
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
    /* The ledger handoff behind this special (the ROM waits for a server
     * commit before the script continues) still needs a save checkpoint
     * before and after the battle, with no time limit. Wally's co-op battle
     * now goes through the local-reward path instead: the script's
     * trainerbattle_no_intro reaches BattleSetup_StartTrainerBattle, whose
     * co-op hook offers the battle like every other story battle (A9). */
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

/* Phase 1 covers ordinary route trainers. Gym leaders, the Elite Four and
 * champions, rivals (including Wally), villain admins and bosses, frontier
 * brains and other story classes keep the vanilla battle until their own
 * reward and story rules exist. Team grunts stay eligible: their story
 * battles use continue-script modes, which are excluded separately. */
bool8 CoopTrainerEncounter_IsPhaseOneClass(u8 trainerClass)
{
    switch (trainerClass)
    {
    case TRAINER_CLASS_AQUA_ADMIN:
    case TRAINER_CLASS_AQUA_LEADER:
    case TRAINER_CLASS_MAGMA_ADMIN:
    case TRAINER_CLASS_MAGMA_LEADER:
    case TRAINER_CLASS_ELITE_FOUR:
    case TRAINER_CLASS_LEADER:
    case TRAINER_CLASS_CHAMPION:
    case TRAINER_CLASS_RIVAL:
    case TRAINER_CLASS_SALON_MAIDEN:
    case TRAINER_CLASS_DOME_ACE:
    case TRAINER_CLASS_PALACE_MAVEN:
    case TRAINER_CLASS_ARENA_TYCOON:
    case TRAINER_CLASS_FACTORY_HEAD:
    case TRAINER_CLASS_PIKE_QUEEN:
    case TRAINER_CLASS_PYRAMID_KING:
    case TRAINER_CLASS_RS_PROTAG:
    case TRAINER_CLASS_RIVAL_EARLY_FRLG:
    case TRAINER_CLASS_RIVAL_LATE_FRLG:
    case TRAINER_CLASS_BOSS_FRLG:
    case TRAINER_CLASS_LEADER_FRLG:
    case TRAINER_CLASS_ELITE_FOUR_FRLG:
    case TRAINER_CLASS_CHAMPION_FRLG:
    case TRAINER_CLASS_PKMN_PROF_FRLG:
    case TRAINER_CLASS_PLAYER_FRLG:
    case TRAINER_CLASS_ROCKET_ADMIN:
        return FALSE;
    default:
        return TRUE;
    }
}

/* Phase 1 classes, the eight Hoenn gym leaders and their match-call rematches
 * (A8), and the Hoenn story battles with their rematches (A9, the rivals,
 * Wally, the admins and bosses, story grunts, the Elite Four, the champion
 * and Steven). Every other story class (Kanto, the frontier brains) keeps the
 * vanilla battle. */
bool8 CoopTrainerEncounter_IsSupportedTrainer(u16 trainerId)
{
    u16 base;

    if (CoopTrainerEncounter_IsPhaseOneClass(GetTrainerClassFromId(trainerId))
     || CoopTrainerRewards_IsHoennGymLeader(trainerId)
     || CoopTrainerRewards_GetStoryBattle(trainerId) != COOP_STORY_NONE)
        return TRUE;
    return BattleSetup_GetRematchBaseTrainer(trainerId, &base)
        && CoopTrainerRewards_GetStoryBattle(base) != COOP_STORY_NONE;
}

static bool8 IsStoryTrainer(u16 trainerId)
{
    return CoopTrainerRewards_GetHoennGym(trainerId) != COOP_HOENN_GYM_NONE
        || CoopTrainerRewards_GetStoryBattle(trainerId) != COOP_STORY_NONE;
}

/* The script-continuing modes carry story post-battle scripts. Only a Hoenn
 * gym leader's first battle and the story battles may use them here: their
 * post-battle scripts are known (the gym grants, sCoopStoryBattles). The
 * match-call registration scripts of route trainers stay vanilla. */
static bool8 IsEligibleBattleMode(u16 trainerId)
{
    switch (GetTrainerBattleMode())
    {
    case TRAINER_BATTLE_SINGLE:
    case TRAINER_BATTLE_SINGLE_NO_INTRO_TEXT: // also Norman's gym battle
    case TRAINER_BATTLE_DOUBLE:
        return TRUE;
    case TRAINER_BATTLE_CONTINUE_SCRIPT:
    case TRAINER_BATTLE_CONTINUE_SCRIPT_NO_MUSIC:
    case TRAINER_BATTLE_CONTINUE_SCRIPT_DOUBLE:
    case TRAINER_BATTLE_CONTINUE_SCRIPT_DOUBLE_NO_MUSIC:
        return IsStoryTrainer(trainerId);
#if FREE_MATCH_CALL == FALSE
    case TRAINER_BATTLE_REMATCH:
    case TRAINER_BATTLE_REMATCH_DOUBLE:
        /* ConfigureTrainerBattle already swapped in the rematch entry. */
        return BattleSetup_GetRematchBaseTrainer(trainerId, NULL);
#endif //FREE_MATCH_CALL
    default:
        /* Early rival (a Kanto mode: Hoenn's rivals use no-intro battles),
         * two trainers without intro, pyramid and hill modes stay vanilla. */
        return FALSE;
    }
}

static bool8 IsCooldownTrainer(u16 trainerId)
{
    return sConsent.cooldown_trainer_id != TRAINER_NONE
        && sConsent.cooldown_trainer_id == trainerId
        && (s32)(sConsent.cooldown_deadline - gMain.vblankCounter1) > 0;
}

static bool8 IsPartnerNearby(void)
{
#if TESTING
    if (sTestPartnerNearby != 0)
        return sTestPartnerNearby == 2;
#endif
    return CoopPresenceRuntime_IsPartnerNearby(COOP_TRAINER_ENCOUNTER_PARTNER_TILES);
}

/* Every condition is required; any failure keeps the vanilla battle. */
bool8 CoopTrainerEncounter_IsEligible(u16 trainerId)
{
    enum CoopRegion region;
    u16 ordinal;
    u8 gym;

    if (!IsEligibleBattleMode(trainerId))
        return FALSE;
    if (trainerId == TRAINER_NONE || trainerId == TRAINER_SECRET_BASE
     || trainerId != TRAINER_BATTLE_PARAM.opponentA
     || gNoOfApproachingTrainers == 2
     || CurrentBattlePyramidLocation() != PYRAMID_LOCATION_NONE
     || InTrainerHillChallenge()
     || FollowerNPCIsBattlePartner()
     || (B_FLAG_SKY_BATTLE != 0 && FlagGet(B_FLAG_SKY_BATTLE))
     || IsCooldownTrainer(trainerId))
        return FALSE;
    if (!CoopNetBridge_IsGrouped()
     || sConsent.state != CONSENT_IDLE || !sConsent.session_ready
     || sConsent.encounter_state != ENCOUNTER_NONE
     || sConsent.wally_request_state != WALLY_REQUEST_NONE
     || sConsent.brock_request_state != BROCK_REQUEST_NONE
     || !CoopNetBridge_CanSendBattle()
     || CoopBattleRuntime_HasPendingOutboundReplay()
     || CoopBattleRuntime_IsEngineActive())
        return FALSE;
    if (!CoopRegion_TryGetActive(&region)
     || !CoopIdentity_ResolveTrainerOrdinal(region, trainerId, &ordinal)
     || !CoopTrainerEncounter_IsSupportedTrainer(trainerId))
        return FALSE;
    /* A leader's script only reaches the battle while its trainer flag is
     * clear. A requester that already holds the badge anyway (a cleared
     * flag) keeps the vanilla battle, so a badge is never granted twice. */
    gym = CoopTrainerRewards_GetHoennGym(trainerId);
    if (gym != COOP_HOENN_GYM_NONE && FlagGet(FLAG_BADGE01_GET + gym))
        return FALSE;
    return IsPartnerNearby();
}

bool8 CoopTrainerEncounter_TryBegin(u16 trainerId)
{
    if (IsCooldownTrainer(trainerId))
    {
        /* One-shot: this sighting is vanilla, the next may be co-op. */
        sConsent.cooldown_trainer_id = TRAINER_NONE;
        sConsent.cooldown_deadline = 0;
        return FALSE;
    }
    if (!CoopTrainerEncounter_IsEligible(trainerId)
     || !CoopBattleConsent_BeginTrainer(trainerId))
        return FALSE;
    sConsent.encounter_state = ENCOUNTER_WAITING;
    sConsent.deadline_frame = gMain.vblankCounter1 + COOP_TRAINER_ENCOUNTER_WAIT_FRAMES;
    /* The trainer's intro box is still open; replace it for the wait. */
    HideFieldMessageBox();
    (void)ShowFieldMessage(sWaitingForPartnerText);
    return TRUE;
}

bool8 CoopTrainerEncounter_IsRequesterBattle(void)
{
    return sConsent.encounter_state == ENCOUNTER_IN_BATTLE;
}

bool8 CoopTrainerEncounter_OnBattleEnded(bool8 completed)
{
    if (sConsent.encounter_state != ENCOUNTER_IN_BATTLE)
        return FALSE;
    sConsent.encounter_state = ENCOUNTER_NONE;
    ClearState();
    if (sConsent.outcome == COOP_BATTLE_CONSENT_ACCEPTED)
        ClearOutcome();
    if (completed)
        return FALSE;
    sConsent.cooldown_trainer_id = TRAINER_BATTLE_PARAM.opponentA;
    sConsent.cooldown_deadline = gMain.vblankCounter1
        + COOP_TRAINER_ENCOUNTER_COOLDOWN_FRAMES;
    return TRUE;
}

#if TESTING
void CoopTrainerEncounter_TestSetPartnerNearby(s8 nearby)
{
    sTestPartnerNearby = nearby < 0 ? 0 : (nearby ? 2 : 1);
}

bool8 CoopTrainerEncounter_TestIsPending(void)
{
    return sConsent.encounter_state != ENCOUNTER_NONE;
}

void CoopTrainerEncounter_TestPlayPartner(void)
{
    if (sConsent.encounter_state == ENCOUNTER_IN_BATTLE)
        sConsent.encounter_state = ENCOUNTER_NONE;
}
#endif
