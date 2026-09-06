#ifndef GUARD_COOP_ONLINE_PROTOCOL_H
#define GUARD_COOP_ONLINE_PROTOCOL_H
#include "gba/types.h"

#define COOP_ONLINE_REQUEST_SIZE 12
#define COOP_ONLINE_STATUS_SIZE 112
#define COOP_ONLINE_NAME_SIZE 32
enum CoopOnlineAction { COOP_ONLINE_REFRESH, COOP_ONLINE_INVITE, COOP_ONLINE_ACCEPT, COOP_ONLINE_DECLINE, COOP_ONLINE_LEAVE };
enum CoopOnlineResult { COOP_ONLINE_READY, COOP_ONLINE_UNAVAILABLE, COOP_ONLINE_SUCCESS, COOP_ONLINE_STALE, COOP_ONLINE_FAILED };
enum CoopOnlineFlags { COOP_ONLINE_GROUPED = 1, COOP_ONLINE_HAS_NEARBY = 2, COOP_ONLINE_HAS_INCOMING = 4 };
struct CoopOnlineRequest { u32 request_id; u32 view_id; u8 action; u8 page; };
struct CoopOnlineStatus
{
    u32 request_id;
    u8 result, flags, nearby_count, incoming_count, nearby_page, incoming_page;
    // Wire slots hold 32 bytes; runtime strings always have a terminator.
    u8 nearby_name[COOP_ONLINE_NAME_SIZE + 1];
    u8 incoming_name[COOP_ONLINE_NAME_SIZE + 1];
    u8 group_name[COOP_ONLINE_NAME_SIZE + 1];
};
#endif
