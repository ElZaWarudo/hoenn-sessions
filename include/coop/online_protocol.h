#ifndef GUARD_COOP_ONLINE_PROTOCOL_H
#define GUARD_COOP_ONLINE_PROTOCOL_H
#include "gba/types.h"

#define COOP_ONLINE_REQUEST_SIZE 12
#define COOP_ONLINE_STATUS_SIZE 112
#define COOP_ONLINE_NAME_SIZE 32
enum CoopOnlineAction
{
    COOP_ONLINE_REFRESH = 0,
    COOP_ONLINE_INVITE = 1,
    COOP_ONLINE_ACCEPT = 2,
    COOP_ONLINE_DECLINE = 3,
    COOP_ONLINE_LEAVE = 4,
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
    // Wire slots hold 32 bytes; runtime strings always have a terminator.
    u8 nearby_name[COOP_ONLINE_NAME_SIZE + 1];
    u8 incoming_name[COOP_ONLINE_NAME_SIZE + 1];
    u8 group_name[COOP_ONLINE_NAME_SIZE + 1];
};
#endif
