#include "global.h"
#include "coop/trade_offer.h"
#include "coop/battle_runtime.h"
#include "coop/net_bridge.h"
#include "coop/trade_runtime.h"
#include "event_data.h"
#include "mail.h"
#include "main.h"
#include "overworld.h"
#include "palette.h"
#include "pokemon.h"
#include "random.h"
#include "script.h"
#include "string_util.h"
#include "task.h"
#include "constants/characters.h"
#include "constants/items.h"

/*
 * The in-game trade UI. One flow at a time, as requester or responder:
 *
 *   requester: party menu -> checkpoint -> TradeOfferRequest -> PENDING
 *              -> ACCEPTED / DECLINED / EXPIRED / CANCELLED (B) / failure
 *   responder: TradeOfferReceived -> prompt when the field is free -> Yes
 *              -> party menu -> checkpoint -> TradeOfferDecision (accept)
 *              -> ACCEPTED / failure; No or the deadline declines.
 *
 * The checkpoint before an offer or an accept makes the server's snapshot
 * head hold the live party; the server refuses a slot that no longer holds
 * the named personality and OT ID. The field stays locked (the script waits)
 * from the party selection until the outcome, and after an accept until the
 * TradeCommit arrives, so the chosen Pokemon cannot be moved or offered
 * twice. Nothing in the party changes here: only trade_runtime.c applies a
 * trade, and only from a server-issued TradeCommit.
 */

#define FLAG_EGG            COOP_TRADE_OFFER_FLAG_EGG
#define FLAG_CHECKPOINT     (1 << 1)
#define FLAG_CANCEL_SENT    (1 << 2)
#define FLAG_CLOSED         (1 << 3)

struct CoopTradeOffer
{
    u32 request_id;
    u32 offer_token;
    u32 personality;
    u32 ot_id;
    u32 deadline;
    /* A request this ROM stopped waiting for; withdrawn when possible. */
    u32 abandoned_request;
    u16 species;
    u8 nickname[COOP_TRADE_OFFER_NICKNAME_SIZE];
    u8 level;
    u8 flags;
    u8 state;
    u8 slot;
    u8 result;
    u8 role;
};

static EWRAM_DATA struct CoopTradeOffer sTrade = {0};

extern const u8 EventScript_CoopTradeRequest[];
extern const u8 EventScript_CoopTradeOffer[];

static const u8 sText_Accepted[] = _("The trade was accepted!");
static const u8 sText_Declined[] = _("Your partner declined the trade.");
static const u8 sText_Expired[] = _("The trade offer expired.");
static const u8 sText_Cancelled[] = _("Trade cancelled.");
static const u8 sText_Withdrawn[] = _("Your partner withdrew the offer.");
static const u8 sText_Unavailable[] = _("Trade unavailable.\nTry again later.");
static const u8 sText_PartnerUnavailable[] = _("Your partner can't trade\nright now.");
static const u8 sText_Mail[] = _("A POKéMON holding MAIL\ncan't be traded.");
static const u8 sText_Busy[] = _("Another trade is in progress.");
static const u8 sText_Stale[] = _("Trade data changed.\nTry again.");
static const u8 sText_NoAnswer[] = _("Your partner didn't answer.");
static const u8 sText_LastMon[] = _("You can't trade your\nlast POKéMON.");
static const u8 sText_Egg[] = _("EGG");

static void WriteU32(u8 *bytes, u32 value)
{
    bytes[0] = value;
    bytes[1] = value >> 8;
    bytes[2] = value >> 16;
    bytes[3] = value >> 24;
}

static u32 ReadU32(const u8 *bytes)
{
    return (u32)bytes[0] | ((u32)bytes[1] << 8)
         | ((u32)bytes[2] << 16) | ((u32)bytes[3] << 24);
}

static u32 Now(void)
{
    return gMain.vblankCounter1;
}

static bool8 IsPast(u32 deadline)
{
    return (s32)(deadline - Now()) <= 0;
}

static void Reset(void)
{
    u32 abandoned = sTrade.abandoned_request;

    memset(&sTrade, 0, sizeof(sTrade));
    sTrade.abandoned_request = abandoned;
}

static void Finish(u8 result)
{
    sTrade.state = COOP_TRADE_OFFER_DONE;
    sTrade.result = result;
    sTrade.flags &= ~(FLAG_CHECKPOINT | FLAG_CANCEL_SENT);
}

static bool8 CanSend(void)
{
    return CoopNetBridge_CanSendBattle() && !CoopBattleRuntime_HasPendingOutboundReplay();
}

static bool8 SendRequest(u8 action, u32 request_id)
{
    u8 payload[COOP_TRADE_OFFER_REQUEST_SIZE] = {0};

    if (!CanSend())
        return FALSE;
    payload[0] = action;
    WriteU32(payload + 4, request_id);
    if (action == COOP_TRADE_OFFER_ACTION_OFFER)
    {
        payload[1] = sTrade.slot;
        WriteU32(payload + 8, sTrade.personality);
        WriteU32(payload + 12, sTrade.ot_id);
    }
    return CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_TRADE_OFFER_REQUEST,
                                              payload, sizeof(payload));
}

static bool8 SendDecision(u8 decision, u32 offer_token)
{
    u8 payload[COOP_TRADE_OFFER_DECISION_SIZE] = {0};

    if (!CanSend())
        return FALSE;
    payload[0] = decision;
    WriteU32(payload + 4, offer_token);
    if (decision == COOP_TRADE_OFFER_ACCEPT)
    {
        payload[1] = sTrade.slot;
        WriteU32(payload + 8, sTrade.personality);
        WriteU32(payload + 12, sTrade.ot_id);
    }
    return CoopNetBridge_EnqueueGameToNetwork(COOP_BRIDGE_MESSAGE_TRADE_OFFER_DECISION,
                                              payload, sizeof(payload));
}

/* Stops waiting for this ROM's request; the launcher is told to withdraw it
 * now, or on the next session if the bridge cannot take the frame. */
static void Abandon(void)
{
    if (sTrade.request_id == 0)
        return;
    if (!SendRequest(COOP_TRADE_OFFER_ACTION_CANCEL, sTrade.request_id))
        sTrade.abandoned_request = sTrade.request_id;
}

static bool8 IsSafeOverworld(void)
{
    return gMain.callback1 == CB1_Overworld && gMain.callback2 == CB2_Overworld
        && !gPaletteFade.active && !ArePlayerFieldControlsLocked()
        && !ScriptContext_IsEnabled();
}

static bool8 HoldsMail(struct Pokemon *mon)
{
    return GetMonData(mon, MON_DATA_MAIL) != MAIL_NONE
        || ItemIsMail(GetMonData(mon, MON_DATA_HELD_ITEM));
}

/* The chosen Pokemon is the party's only one that is not an egg. */
static bool8 IsLastUsableMon(u8 slot)
{
    u8 i;

    for (i = 0; i < gPlayerPartyCount && i < PARTY_SIZE; i++)
    {
        if (i != slot && GetMonData(&gPlayerParty[i], MON_DATA_SANITY_HAS_SPECIES)
         && !GetMonData(&gPlayerParty[i], MON_DATA_IS_EGG))
            return FALSE;
    }
    return TRUE;
}

/* Checks a party pick; returns 0 or the result that refuses it. */
static u8 CheckPick(u8 slot)
{
    struct Pokemon *mon;

    if (slot >= PARTY_SIZE || slot >= gPlayerPartyCount)
        return COOP_TRADE_OUTCOME_STALE;
    mon = &gPlayerParty[slot];
    if (!GetMonData(mon, MON_DATA_SANITY_HAS_SPECIES)
     || GetMonData(mon, MON_DATA_SANITY_IS_BAD_EGG))
        return COOP_TRADE_OUTCOME_STALE;
    if (HoldsMail(mon))
        return COOP_TRADE_OUTCOME_MAIL;
    if (IsLastUsableMon(slot))
        return COOP_TRADE_RESULT_LAST_MON;
    sTrade.slot = slot;
    sTrade.personality = mon->box.personality;
    sTrade.ot_id = mon->box.otId;
    return 0;
}

/* Runs the checkpoint that must precede an offer or an accept. */
static void PollCheckpoint(u8 next_state)
{
    enum CoopCheckpointRequestResult request;

    if (!(sTrade.flags & FLAG_CHECKPOINT))
    {
        if (IsPast(sTrade.deadline))
        {
            if (next_state == COOP_TRADE_OFFER_RSP_SEND)
                (void)SendDecision(COOP_TRADE_OFFER_DECLINE, sTrade.offer_token);
            Finish(COOP_TRADE_OUTCOME_UNAVAILABLE);
            return;
        }
        request = CoopNetBridge_RequestCheckpoint();
        if (request == COOP_CHECKPOINT_REQUEST_STARTED)
            sTrade.flags |= FLAG_CHECKPOINT;
        else if (request == COOP_CHECKPOINT_REQUEST_OFFLINE)
            Finish(COOP_TRADE_OUTCOME_UNAVAILABLE);
        return;
    }
    switch (CoopNetBridge_GetCheckpointState())
    {
    case COOP_CHECKPOINT_STATE_WAITING_FOR_GRANT:
        return;
    case COOP_CHECKPOINT_STATE_GRANTED:
        sTrade.flags &= ~FLAG_CHECKPOINT;
        if (CoopNetBridge_ConsumeCheckpointGrant()
         && CoopNetBridge_IsCheckpointAuthorizedForSave()
         && CoopTradeRuntime_RunCheckpointSave())
        {
            if (sTrade.flags & FLAG_CLOSED)
            {
                /* The offer ended while this ROM was saving. */
                Finish(sTrade.result);
                return;
            }
            sTrade.state = next_state;
            sTrade.deadline = Now() + COOP_TRADE_OFFER_REPLY_FRAMES;
            return;
        }
        CoopNetBridge_NotifySaveResult(FALSE);
        return;
    default:
        /* Refused or timed out; the next frame asks again until the
         * deadline. */
        sTrade.flags &= ~FLAG_CHECKPOINT;
        return;
    }
}

void CoopTradeOffer_Init(void)
{
    memset(&sTrade, 0, sizeof(sTrade));
}

bool8 CoopTradeOffer_CanBegin(void)
{
    return sTrade.state == COOP_TRADE_OFFER_IDLE
        && CoopNetBridge_IsGrouped()
        && CanSend()
        && CoopTradeRuntime_GetState() == COOP_TRADE_STATE_IDLE
        && !CoopBattleRuntime_HasManifest()
        && !CoopBattleRuntime_IsEngineActive()
        && gPlayerPartyCount >= 2;
}

void CoopTradeOffer_StartRequestScript(void)
{
    ScriptContext_SetupScript(EventScript_CoopTradeRequest);
}

bool8 CoopTradeOffer_BeginOffer(u8 slot)
{
    u8 refusal;
    u32 request_id;

    if (!CoopTradeOffer_CanBegin())
    {
        if (sTrade.state == COOP_TRADE_OFFER_IDLE)
        {
            Reset();
            sTrade.role = COOP_TRADE_OFFER_ROLE_REQUESTER;
            Finish(!CoopNetBridge_IsGrouped() ? COOP_TRADE_OUTCOME_PARTNER_UNAVAILABLE
                 : CoopTradeRuntime_GetState() != COOP_TRADE_STATE_IDLE
                   ? COOP_TRADE_OUTCOME_BUSY : COOP_TRADE_OUTCOME_UNAVAILABLE);
        }
        return FALSE;
    }
    Reset();
    sTrade.role = COOP_TRADE_OFFER_ROLE_REQUESTER;
    refusal = CheckPick(slot);
    if (refusal != 0)
    {
        Finish(refusal);
        return FALSE;
    }
    do
    {
        request_id = Random32();
    } while (request_id == 0 || request_id == sTrade.abandoned_request);
    sTrade.request_id = request_id;
    sTrade.state = COOP_TRADE_OFFER_REQ_CHECKPOINT;
    sTrade.deadline = Now() + COOP_TRADE_OFFER_CHECKPOINT_FRAMES;
    return TRUE;
}

u8 CoopTradeOffer_Respond(u8 slot)
{
    u8 refusal;

    if (sTrade.state != COOP_TRADE_OFFER_RSP_PROMPT)
    {
        if (sTrade.state != COOP_TRADE_OFFER_DONE)
        {
            Reset();
            sTrade.role = COOP_TRADE_OFFER_ROLE_RESPONDER;
            Finish(COOP_TRADE_OUTCOME_EXPIRED);
        }
        return 0;
    }
    if (slot >= PARTY_SIZE)
    {
        (void)SendDecision(COOP_TRADE_OFFER_DECLINE, sTrade.offer_token);
        Reset();
        return 2;
    }
    if (sTrade.flags & FLAG_CLOSED)
    {
        Finish(sTrade.result);
        return 0;
    }
    if (IsPast(sTrade.deadline))
    {
        (void)SendDecision(COOP_TRADE_OFFER_DECLINE, sTrade.offer_token);
        Finish(COOP_TRADE_OUTCOME_EXPIRED);
        return 0;
    }
    refusal = CheckPick(slot);
    if (refusal != 0)
    {
        /* The requester is told the offer was declined. */
        (void)SendDecision(COOP_TRADE_OFFER_DECLINE, sTrade.offer_token);
        Finish(refusal);
        return 0;
    }
    sTrade.state = COOP_TRADE_OFFER_RSP_CHECKPOINT;
    sTrade.deadline = Now() + COOP_TRADE_OFFER_CHECKPOINT_FRAMES;
    return 1;
}

static void BeginCancel(void)
{
    sTrade.state = COOP_TRADE_OFFER_REQ_CANCEL;
    sTrade.deadline = Now() + COOP_TRADE_OFFER_CANCEL_FRAMES;
    sTrade.flags &= ~FLAG_CANCEL_SENT;
    if (SendRequest(COOP_TRADE_OFFER_ACTION_CANCEL, sTrade.request_id))
        sTrade.flags |= FLAG_CANCEL_SENT;
}

bool8 CoopTradeOffer_WaitStep(u16 newKeys)
{
    if (sTrade.state == COOP_TRADE_OFFER_REQ_PENDING && (newKeys & B_BUTTON))
        BeginCancel();
    return sTrade.state == COOP_TRADE_OFFER_DONE || sTrade.state == COOP_TRADE_OFFER_IDLE;
}

u8 CoopTradeOffer_GetResult(void)
{
    return sTrade.result;
}

enum CoopTradeOfferState CoopTradeOffer_GetState(void)
{
    return sTrade.state;
}

void CoopTradeOffer_Finish(void)
{
    if (sTrade.state == COOP_TRADE_OFFER_DONE)
        Reset();
}

const u8 *CoopTradeOffer_GetResultText(void)
{
    switch (sTrade.result)
    {
    case COOP_TRADE_OUTCOME_ACCEPTED: return sText_Accepted;
    case COOP_TRADE_OUTCOME_DECLINED: return sText_Declined;
    case COOP_TRADE_OUTCOME_EXPIRED: return sText_Expired;
    case COOP_TRADE_OUTCOME_CANCELLED:
        return sTrade.role == COOP_TRADE_OFFER_ROLE_RESPONDER ? sText_Withdrawn : sText_Cancelled;
    case COOP_TRADE_RESULT_WITHDRAWN: return sText_Withdrawn;
    case COOP_TRADE_OUTCOME_PARTNER_UNAVAILABLE: return sText_PartnerUnavailable;
    case COOP_TRADE_OUTCOME_MAIL: return sText_Mail;
    case COOP_TRADE_OUTCOME_BUSY: return sText_Busy;
    case COOP_TRADE_OUTCOME_STALE: return sText_Stale;
    case COOP_TRADE_RESULT_NO_ANSWER: return sText_NoAnswer;
    case COOP_TRADE_RESULT_LAST_MON: return sText_LastMon;
    default: return sText_Unavailable;
    }
}

bool8 CoopTradeOffer_ReceiveOffer(const u8 *payload, u16 length)
{
    u32 token;
    u16 species;

    if (payload == NULL || length != COOP_TRADE_OFFER_RECEIVED_SIZE)
        return FALSE;
    token = ReadU32(payload);
    species = payload[4] | (payload[5] << 8);
    if (token == 0 || species == SPECIES_NONE || payload[6] > MAX_LEVEL
     || (payload[7] & ~COOP_TRADE_OFFER_FLAG_EGG) != 0
     || payload[18] != 0 || payload[19] != 0)
        return FALSE;
    /* A redelivery (control restart) of the offer already held. */
    if (sTrade.role == COOP_TRADE_OFFER_ROLE_RESPONDER && sTrade.offer_token == token
     && sTrade.state != COOP_TRADE_OFFER_IDLE)
        return TRUE;
    if (sTrade.state != COOP_TRADE_OFFER_IDLE || species >= NUM_SPECIES
     || CoopTradeRuntime_GetState() != COOP_TRADE_STATE_IDLE)
    {
        /* Busy with another trade, or unable to show it: let the requester
         * know now instead of at the server's deadline. */
        (void)SendDecision(COOP_TRADE_OFFER_DECLINE, token);
        return TRUE;
    }
    Reset();
    sTrade.role = COOP_TRADE_OFFER_ROLE_RESPONDER;
    sTrade.offer_token = token;
    sTrade.species = species;
    sTrade.level = payload[6];
    sTrade.flags = payload[7] & COOP_TRADE_OFFER_FLAG_EGG;
    memcpy(sTrade.nickname, payload + 8, COOP_TRADE_OFFER_NICKNAME_SIZE);
    sTrade.state = COOP_TRADE_OFFER_RSP_RECEIVED;
    sTrade.deadline = Now() + COOP_TRADE_OFFER_PROMPT_FRAMES;
    return TRUE;
}

static void ReceiveRequesterStatus(u8 outcome, u32 request_id, u32 token)
{
    if (sTrade.role != COOP_TRADE_OFFER_ROLE_REQUESTER || request_id != sTrade.request_id)
        return;
    switch (sTrade.state)
    {
    case COOP_TRADE_OFFER_REQ_REPLY:
        if (outcome == COOP_TRADE_OUTCOME_PENDING)
        {
            sTrade.offer_token = token;
            sTrade.state = COOP_TRADE_OFFER_REQ_PENDING;
            sTrade.deadline = Now() + COOP_TRADE_OFFER_WAIT_FRAMES;
            return;
        }
        break;
    case COOP_TRADE_OFFER_REQ_PENDING:
    case COOP_TRADE_OFFER_REQ_CANCEL:
        if (outcome == COOP_TRADE_OUTCOME_PENDING
         || (token != 0 && token != sTrade.offer_token))
            return;
        break;
    default:
        return;
    }
    if (outcome == COOP_TRADE_OUTCOME_ACCEPTED)
    {
        sTrade.state = COOP_TRADE_OFFER_COMMIT_WAIT;
        sTrade.result = COOP_TRADE_OUTCOME_ACCEPTED;
        sTrade.deadline = Now() + COOP_TRADE_OFFER_COMMIT_FRAMES;
        return;
    }
    Finish(outcome);
}

static void ReceiveResponderStatus(u8 outcome, u32 token)
{
    u8 result;

    if (sTrade.role != COOP_TRADE_OFFER_ROLE_RESPONDER || token != sTrade.offer_token)
        return;
    result = outcome == COOP_TRADE_OUTCOME_CANCELLED ? COOP_TRADE_RESULT_WITHDRAWN : outcome;
    switch (sTrade.state)
    {
    case COOP_TRADE_OFFER_RSP_RECEIVED:
        /* Gone before it could be shown. */
        Reset();
        return;
    case COOP_TRADE_OFFER_RSP_PROMPT:
    case COOP_TRADE_OFFER_RSP_CHECKPOINT:
    case COOP_TRADE_OFFER_RSP_SEND:
        /* The script is asking or saving; it shows this once it resumes. */
        sTrade.flags |= FLAG_CLOSED;
        sTrade.result = result;
        return;
    case COOP_TRADE_OFFER_RSP_REPLY:
        if (outcome == COOP_TRADE_OUTCOME_ACCEPTED)
        {
            sTrade.state = COOP_TRADE_OFFER_COMMIT_WAIT;
            sTrade.result = COOP_TRADE_OUTCOME_ACCEPTED;
            sTrade.deadline = Now() + COOP_TRADE_OFFER_COMMIT_FRAMES;
            return;
        }
        Finish(result);
        return;
    default:
        return;
    }
}

bool8 CoopTradeOffer_ReceiveStatus(const u8 *payload, u16 length)
{
    u8 role, outcome;
    u32 request_id, token;

    if (payload == NULL || length != COOP_TRADE_OFFER_STATUS_SIZE)
        return FALSE;
    role = payload[0];
    outcome = payload[1];
    request_id = ReadU32(payload + 4);
    token = ReadU32(payload + 8);
    if (payload[2] != 0 || payload[3] != 0
     || outcome < COOP_TRADE_OUTCOME_PENDING || outcome > COOP_TRADE_OUTCOME_STALE)
        return FALSE;
    if (role == COOP_TRADE_OFFER_ROLE_REQUESTER)
    {
        if (request_id == 0 || (outcome == COOP_TRADE_OUTCOME_PENDING && token == 0))
            return FALSE;
        ReceiveRequesterStatus(outcome, request_id, token);
        return TRUE;
    }
    if (role == COOP_TRADE_OFFER_ROLE_RESPONDER)
    {
        if (request_id != 0 || token == 0 || outcome == COOP_TRADE_OUTCOME_PENDING)
            return FALSE;
        ReceiveResponderStatus(outcome, token);
        return TRUE;
    }
    return FALSE;
}

void CoopTradeOffer_OnSessionReady(void)
{
    /* A new transport generation dropped any unconsumed frame; a request or
     * decision in flight can no longer be answered. */
    CoopTradeOffer_OnTransportLost();
}

void CoopTradeOffer_OnTransportLost(void)
{
    switch (sTrade.state)
    {
    case COOP_TRADE_OFFER_REQ_CHECKPOINT:
    case COOP_TRADE_OFFER_REQ_SEND:
        Finish(COOP_TRADE_OUTCOME_UNAVAILABLE);
        break;
    case COOP_TRADE_OFFER_REQ_REPLY:
    case COOP_TRADE_OFFER_REQ_PENDING:
    case COOP_TRADE_OFFER_REQ_CANCEL:
        sTrade.abandoned_request = sTrade.request_id;
        Finish(sTrade.state == COOP_TRADE_OFFER_REQ_CANCEL
             ? COOP_TRADE_OUTCOME_CANCELLED : COOP_TRADE_OUTCOME_UNAVAILABLE);
        break;
    case COOP_TRADE_OFFER_RSP_RECEIVED:
        Reset();
        break;
    case COOP_TRADE_OFFER_RSP_PROMPT:
        sTrade.flags |= FLAG_CLOSED;
        sTrade.result = COOP_TRADE_OUTCOME_UNAVAILABLE;
        break;
    case COOP_TRADE_OFFER_RSP_CHECKPOINT:
    case COOP_TRADE_OFFER_RSP_SEND:
    case COOP_TRADE_OFFER_RSP_REPLY:
        Finish(COOP_TRADE_OUTCOME_UNAVAILABLE);
        break;
    case COOP_TRADE_OFFER_COMMIT_WAIT:
        /* Accepted on the server; the commit arrives with the next session. */
        Finish(COOP_TRADE_OUTCOME_ACCEPTED);
        break;
    default:
        break;
    }
}

void CoopTradeOffer_Poll(void)
{
    if (sTrade.abandoned_request != 0 && sTrade.request_id != sTrade.abandoned_request
     && SendRequest(COOP_TRADE_OFFER_ACTION_CANCEL, sTrade.abandoned_request))
        sTrade.abandoned_request = 0;

    switch (sTrade.state)
    {
    case COOP_TRADE_OFFER_REQ_CHECKPOINT:
        PollCheckpoint(COOP_TRADE_OFFER_REQ_SEND);
        break;
    case COOP_TRADE_OFFER_REQ_SEND:
        if (SendRequest(COOP_TRADE_OFFER_ACTION_OFFER, sTrade.request_id))
        {
            sTrade.state = COOP_TRADE_OFFER_REQ_REPLY;
            sTrade.deadline = Now() + COOP_TRADE_OFFER_REPLY_FRAMES;
        }
        else if (IsPast(sTrade.deadline))
        {
            Finish(COOP_TRADE_OUTCOME_UNAVAILABLE);
        }
        break;
    case COOP_TRADE_OFFER_REQ_REPLY:
        if (IsPast(sTrade.deadline))
        {
            Abandon();
            Finish(COOP_TRADE_OUTCOME_UNAVAILABLE);
        }
        break;
    case COOP_TRADE_OFFER_REQ_PENDING:
        if (IsPast(sTrade.deadline))
        {
            Abandon();
            Finish(COOP_TRADE_RESULT_NO_ANSWER);
        }
        break;
    case COOP_TRADE_OFFER_REQ_CANCEL:
        if (!(sTrade.flags & FLAG_CANCEL_SENT)
         && SendRequest(COOP_TRADE_OFFER_ACTION_CANCEL, sTrade.request_id))
            sTrade.flags |= FLAG_CANCEL_SENT;
        if (IsPast(sTrade.deadline))
        {
            if (!(sTrade.flags & FLAG_CANCEL_SENT))
                sTrade.abandoned_request = sTrade.request_id;
            Finish(COOP_TRADE_OUTCOME_CANCELLED);
        }
        break;
    case COOP_TRADE_OFFER_RSP_RECEIVED:
        if (IsPast(sTrade.deadline))
        {
            (void)SendDecision(COOP_TRADE_OFFER_DECLINE, sTrade.offer_token);
            Reset();
        }
        else if (IsSafeOverworld() && CoopTradeRuntime_GetState() == COOP_TRADE_STATE_IDLE)
        {
            sTrade.state = COOP_TRADE_OFFER_RSP_PROMPT;
            ScriptContext_SetupScript(EventScript_CoopTradeOffer);
        }
        break;
    case COOP_TRADE_OFFER_RSP_CHECKPOINT:
        PollCheckpoint(COOP_TRADE_OFFER_RSP_SEND);
        break;
    case COOP_TRADE_OFFER_RSP_SEND:
        if (sTrade.flags & FLAG_CLOSED)
            Finish(sTrade.result);
        else if (SendDecision(COOP_TRADE_OFFER_ACCEPT, sTrade.offer_token))
        {
            sTrade.state = COOP_TRADE_OFFER_RSP_REPLY;
            sTrade.deadline = Now() + COOP_TRADE_OFFER_REPLY_FRAMES;
        }
        else if (IsPast(sTrade.deadline))
            Finish(COOP_TRADE_OUTCOME_UNAVAILABLE);
        break;
    case COOP_TRADE_OFFER_RSP_REPLY:
        if (IsPast(sTrade.deadline))
            Finish(COOP_TRADE_OUTCOME_UNAVAILABLE);
        break;
    case COOP_TRADE_OFFER_COMMIT_WAIT:
        if (CoopTradeRuntime_GetState() != COOP_TRADE_STATE_IDLE || IsPast(sTrade.deadline))
            Finish(COOP_TRADE_OUTCOME_ACCEPTED);
        break;
    default:
        break;
    }
}

void Special_CoopTradeBeginOffer(void)
{
    gSpecialVar_Result = CoopTradeOffer_BeginOffer(gSpecialVar_0x8004);
}

void Special_CoopTradeBufferOffer(void)
{
    if (sTrade.state != COOP_TRADE_OFFER_RSP_PROMPT || (sTrade.flags & FLAG_CLOSED)
     || IsPast(sTrade.deadline))
    {
        gSpecialVar_Result = CoopTradeOffer_Respond(0);
        return;
    }
    if (sTrade.flags & FLAG_EGG)
    {
        StringCopy(gStringVar1, sText_Egg);
        gSpecialVar_Result = 2;
        return;
    }
    StringCopy(gStringVar2, GetSpeciesName(sTrade.species));
    if (sTrade.nickname[0] == EOS)
    {
        StringCopy(gStringVar1, gStringVar2);
    }
    else
    {
        memcpy(gStringVar1, sTrade.nickname, COOP_TRADE_OFFER_NICKNAME_SIZE);
        gStringVar1[COOP_TRADE_OFFER_NICKNAME_SIZE] = EOS;
    }
    ConvertIntToDecimalStringN(gStringVar3, sTrade.level, STR_CONV_MODE_LEFT_ALIGN, 3);
    gSpecialVar_Result = 1;
}

void Special_CoopTradeRespond(void)
{
    gSpecialVar_Result = CoopTradeOffer_Respond(gSpecialVar_0x8004);
}

static void Task_CoopTradeWait(u8 taskId)
{
    if (CoopTradeOffer_WaitStep(gMain.newKeys))
    {
        DestroyTask(taskId);
        ScriptContext_Enable();
    }
}

/* Used with waitstate: the task re-enables the script once the flow ends. */
void Special_CoopTradeWait(void)
{
    CreateTask(Task_CoopTradeWait, 80);
}

void Special_CoopTradeBufferResult(void)
{
    StringCopy(gStringVar4, CoopTradeOffer_GetResultText());
}

void Special_CoopTradeFinish(void)
{
    CoopTradeOffer_Finish();
}
