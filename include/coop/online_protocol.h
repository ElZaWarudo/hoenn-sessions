#ifndef GUARD_COOP_ONLINE_PROTOCOL_H
#define GUARD_COOP_ONLINE_PROTOCOL_H
#include "gba/types.h"

#define COOP_ONLINE_REQUEST_SIZE 12
#define COOP_ONLINE_STATUS_SIZE 128
#define COOP_ONLINE_NAME_SIZE 32
#define COOP_PAIRING_RECORD_SIZE 12
enum CoopPairingAction { COOP_PAIRING_CREATE = 0, COOP_PAIRING_REDEEM = 1 };
enum CoopPairingResult { COOP_PAIRING_CREATED = 0, COOP_PAIRING_JOINED = 1, COOP_PAIRING_UNAVAILABLE = 2, COOP_PAIRING_INVALID = 3 };
struct CoopPairingRequest { u32 request_id; u8 action; u8 code[7]; };
struct CoopPairingStatus { u32 request_id; u8 result; u8 code[7]; };
_Static_assert(sizeof(struct CoopPairingRequest) == COOP_PAIRING_RECORD_SIZE, "pairing request wire size");
_Static_assert(sizeof(struct CoopPairingStatus) == COOP_PAIRING_RECORD_SIZE, "pairing status wire size");
enum CoopOnlineAction
{
    COOP_ONLINE_REFRESH = 0,
    COOP_ONLINE_INVITE = 1,
    COOP_ONLINE_ACCEPT = 2,
    COOP_ONLINE_DECLINE = 3,
    COOP_ONLINE_LEAVE = 4,
    COOP_ONLINE_CANCEL = 5,
    COOP_ONLINE_INVITE_LAST_PARTNER = 6,
};

enum CoopOnlineResult
{
    COOP_ONLINE_READY = 0,
    COOP_ONLINE_UNAVAILABLE = 1,
    COOP_ONLINE_SUCCESS = 2,
    COOP_ONLINE_STALE = 3,
    COOP_ONLINE_FAILED = 4,
};

enum CoopOnlineFlags
{
    COOP_ONLINE_GROUPED = 1,
    COOP_ONLINE_HAS_NEARBY = 2,
    COOP_ONLINE_HAS_INCOMING = 4,
    COOP_ONLINE_HAS_OUTGOING = 8,
    COOP_ONLINE_HAS_LOCATION = 16,
    COOP_ONLINE_HAS_LAST_PARTNER = 32,
};

struct CoopOnlineRequest
{
    u32 request_id;
    u32 view_id;
    u8 action;
    u8 page;
};
struct CoopOnlineStatus
{
    u32 request_id;
    u8 result, flags, nearby_count, incoming_count, nearby_page, incoming_page;
    u8 outgoing_count, outgoing_page;
    u16 location_map_group, location_map_number;
    // Wire slots hold 32 bytes; runtime strings always have a terminator.
    u8 nearby_name[COOP_ONLINE_NAME_SIZE + 1];
    u8 incoming_name[COOP_ONLINE_NAME_SIZE + 1];
    u8 group_name[COOP_ONLINE_NAME_SIZE + 1];
    u8 last_partner_name[17];
};
#endif
