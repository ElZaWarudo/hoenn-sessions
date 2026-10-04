#ifndef GUARD_COOP_TRADE_OFFER_H
#define GUARD_COOP_TRADE_OFFER_H

#include "gba/types.h"

/*
 * In-game trade offers (game protocol 4). Every multi-byte integer is little
 * endian and every reserved byte is zero; each message is one bridge frame.
 *
 * TradeOfferRequest (0x0016, ROM to sidecar), 16 bytes:
 *    0 1 action: 1 offer, 2 cancel
 *    1 1 slot, party slot 0..5 (zero for cancel)
 *    2 2 reserved
 *    4 4 request_id, nonzero, chosen by this ROM
 *    8 4 personality of the offered Pokemon (zero for cancel)
 *   12 4 OT ID of the offered Pokemon (zero for cancel)
 * TradeOfferDecision (0x0017, ROM to sidecar), 16 bytes:
 *    0 1 decision: 1 accept, 2 decline
 *    1 1 slot of the Pokemon given (zero for decline)
 *    2 2 reserved
 *    4 4 offer_token from TradeOfferReceived
 *    8 4 personality of the Pokemon given (zero for decline)
 *   12 4 OT ID of the Pokemon given (zero for decline)
 * TradeOfferReceived (0x011A, sidecar to ROM), 20 bytes:
 *    0 4 offer_token, nonzero
 *    4 2 species of the offered Pokemon, nonzero
 *    6 1 level, 0..100
 *    7 1 flags: bit 0 egg
 *    8 10 nickname (game encoding, 0xFF padded)
 *   18 2 reserved
 * TradeOfferStatus (0x011B, sidecar to ROM), 12 bytes:
 *    0 1 role: 1 requester (names request_id), 2 responder (names offer_token)
 *    1 1 outcome (enum CoopTradeOfferOutcome)
 *    2 2 reserved
 *    4 4 request_id (requester, nonzero; responder, zero)
 *    8 4 offer_token (zero until the server holds an offer)
 *
 * Both ROMs take a checkpoint before they send an offer or an accept, so the
 * server's snapshot head holds the party the player picked from. The trade
 * itself arrives later as the ledger's TradeCommit (trade_runtime.c).
 */
#define COOP_TRADE_OFFER_REQUEST_SIZE 16
#define COOP_TRADE_OFFER_DECISION_SIZE 16
#define COOP_TRADE_OFFER_RECEIVED_SIZE 20
#define COOP_TRADE_OFFER_STATUS_SIZE 12
#define COOP_TRADE_OFFER_NICKNAME_SIZE 10

#define COOP_TRADE_OFFER_ACTION_OFFER 1
#define COOP_TRADE_OFFER_ACTION_CANCEL 2
#define COOP_TRADE_OFFER_ACCEPT 1
#define COOP_TRADE_OFFER_DECLINE 2
#define COOP_TRADE_OFFER_ROLE_REQUESTER 1
#define COOP_TRADE_OFFER_ROLE_RESPONDER 2
#define COOP_TRADE_OFFER_FLAG_EGG 1

enum CoopTradeOfferOutcome
{
    COOP_TRADE_OUTCOME_PENDING = 1,
    COOP_TRADE_OUTCOME_ACCEPTED,
    COOP_TRADE_OUTCOME_DECLINED,
    COOP_TRADE_OUTCOME_EXPIRED,
    COOP_TRADE_OUTCOME_CANCELLED,
    COOP_TRADE_OUTCOME_UNAVAILABLE,
    COOP_TRADE_OUTCOME_PARTNER_UNAVAILABLE,
    COOP_TRADE_OUTCOME_MAIL,
    COOP_TRADE_OUTCOME_BUSY,
    COOP_TRADE_OUTCOME_STALE,
    /* Local results the ROM shows; never on the wire. */
    COOP_TRADE_RESULT_NO_ANSWER = 32,
    COOP_TRADE_RESULT_LAST_MON,
    COOP_TRADE_RESULT_WITHDRAWN,
};

/* The partner's prompt must be shown and answered in this window (the server
 * keeps an offer open for 60 s). */
#define COOP_TRADE_OFFER_PROMPT_FRAMES (45 * 60)
/* The launcher answers a request or an accept within this window. */
#define COOP_TRADE_OFFER_REPLY_FRAMES (15 * 60)
/* The requester waits for the partner this long (server window + margin). */
#define COOP_TRADE_OFFER_WAIT_FRAMES (70 * 60)
#define COOP_TRADE_OFFER_CANCEL_FRAMES (10 * 60)
#define COOP_TRADE_OFFER_CHECKPOINT_FRAMES (10 * 60)
/* After an accept, the field stays locked until the TradeCommit arrives or
 * this long, so the traded Pokemon cannot be moved out from under it. */
#define COOP_TRADE_OFFER_COMMIT_FRAMES (10 * 60)

enum CoopTradeOfferState
{
    COOP_TRADE_OFFER_IDLE,
    COOP_TRADE_OFFER_REQ_CHECKPOINT,
    COOP_TRADE_OFFER_REQ_SEND,
    COOP_TRADE_OFFER_REQ_REPLY,
    COOP_TRADE_OFFER_REQ_PENDING,
    COOP_TRADE_OFFER_REQ_CANCEL,
    COOP_TRADE_OFFER_RSP_RECEIVED,
    COOP_TRADE_OFFER_RSP_PROMPT,
    COOP_TRADE_OFFER_RSP_CHECKPOINT,
    COOP_TRADE_OFFER_RSP_SEND,
    COOP_TRADE_OFFER_RSP_REPLY,
    COOP_TRADE_OFFER_COMMIT_WAIT,
    COOP_TRADE_OFFER_DONE,
};

void CoopTradeOffer_Init(void);
void CoopTradeOffer_Poll(void);
void CoopTradeOffer_OnSessionReady(void);
void CoopTradeOffer_OnTransportLost(void);
/* FALSE only for a malformed record (a bridge protocol error). */
bool8 CoopTradeOffer_ReceiveOffer(const u8 *payload, u16 length);
bool8 CoopTradeOffer_ReceiveStatus(const u8 *payload, u16 length);

/* The ONLINE menu's "Trade with partner" entry. */
bool8 CoopTradeOffer_CanBegin(void);
void CoopTradeOffer_StartRequestScript(void);

/* Script steps, also driven directly by the tests. */
bool8 CoopTradeOffer_BeginOffer(u8 slot);
/* 1: started (wait), 0: show the result, 2: declined without a message.
 * A slot >= PARTY_SIZE declines. */
u8 CoopTradeOffer_Respond(u8 slot);
/* TRUE once the result is ready. B withdraws a pending offer. */
bool8 CoopTradeOffer_WaitStep(u16 newKeys);
u8 CoopTradeOffer_GetResult(void);
enum CoopTradeOfferState CoopTradeOffer_GetState(void);
void CoopTradeOffer_Finish(void);
const u8 *CoopTradeOffer_GetResultText(void);

void Special_CoopTradeBeginOffer(void);
void Special_CoopTradeBufferOffer(void);
void Special_CoopTradeRespond(void);
void Special_CoopTradeWait(void);
void Special_CoopTradeBufferResult(void);
void Special_CoopTradeFinish(void);

#endif /* GUARD_COOP_TRADE_OFFER_H */
