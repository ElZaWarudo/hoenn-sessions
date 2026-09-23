#ifndef GUARD_COOP_PRESENCE_RUNTIME_H
#define GUARD_COOP_PRESENCE_RUNTIME_H

#include "gba/types.h"
#include "coop/net_bridge.h"
#include "coop/presence.h"
#include "constants/event_objects.h"

#define COOP_PRESENCE_RUNTIME_SAMPLE_INTERVAL 6
#define COOP_PRESENCE_RUNTIME_INTERPOLATION_FRAMES 6
#define COOP_PRESENCE_RUNTIME_STALE_FRAMES 90
/* Renderer-only grace after a transient despawn (HIDDEN or DISCONNECTED).
 * The reducer still drops the remote at once, so interaction stays
 * disabled; only the already-placed sprite is left frozen. Well under
 * STALE_FRAMES so a real disconnect cannot hide behind the hold. */
#define COOP_PRESENCE_RUNTIME_DESPAWN_HOLD_FRAMES 30
#define COOP_PRESENCE_RUNTIME_PENDING_CAPACITY COOP_NET_BRIDGE_QUEUE_CAPACITY
#define COOP_PRESENCE_RUNTIME_OBJECT_LOCAL_ID OBJ_EVENT_ID_COOP_REMOTE_PLAYER
#define COOP_PRESENCE_RUNTIME_FOLLOWER_LOCAL_ID OBJ_EVENT_ID_COOP_REMOTE_FOLLOWER
/* Companion publish cadence and signal input limits, in field frames. */
#define COOP_PRESENCE_RUNTIME_COMPANION_INTERVAL 300
#define COOP_PRESENCE_RUNTIME_SIGNAL_COOLDOWN 120
#define COOP_PRESENCE_RUNTIME_PING_TTL_FRAMES 300
#define COOP_PRESENCE_RUNTIME_FOLLOWER_STEP_INTERVAL 8

enum CoopPresenceInteractionResult
{
    COOP_PRESENCE_INTERACTION_NONE = 0,
    COOP_PRESENCE_INTERACTION_SCRIPT_STARTED = 1,
    COOP_PRESENCE_INTERACTION_CONSUMED_NO_LOCK = 2,
};

/* Runtime ownership is intentionally bounded to one remote and one fixed
 * pending frame queue.  The reducer remains the source of truth; object
 * events and sprites are only a best-effort view of its current value. */
void CoopPresenceRuntime_Init(void);
void CoopPresenceRuntime_Reset(void);
void CoopPresenceRuntime_SetSessionEpoch(u32 session_epoch);
void CoopPresenceRuntime_TransportLost(void);
void CoopPresenceRuntime_AdvanceFrame(void);

bool8 CoopPresenceRuntime_EncodeLocalState(u8 *bytes, u32 length);
bool8 CoopPresenceRuntime_GetLocalState(struct CoopPresenceLocalState *out);

/* Called by the bridge only after outer-frame validation.  Decoding is strict
 * and atomic; accepted values wait for the normal overworld callback before
 * they can mutate the reducer or object-event renderer. */
bool8 CoopPresenceRuntime_QueueBridgeFrame(u16 type, const u8 *payload, u16 length);
void CoopPresenceRuntime_Update(void);
void CoopPresenceRuntime_OnWarpCommit(void);

enum CoopPresenceInteractionResult CoopPresenceRuntime_TryInteract(void);
/* L (raw) sends a position ping; L + SELECT sends the next emote in the
 * fixed 1..8 cycle. Both are rate-limited and no-ops while hidden. */
bool8 CoopPresenceRuntime_TryPing(void);
bool8 CoopPresenceRuntime_TryEmote(void);
bool8 CoopPresenceRuntime_IsRemoteObject(const struct ObjectEvent *object_event);
bool8 CoopPresenceRuntime_IsFollowerObject(const struct ObjectEvent *object_event);
/* Latest remote ping tile while its time-to-live has not expired. */
bool8 CoopPresenceRuntime_GetPingMarker(s16 *x, s16 *y);
void CoopPresenceRuntime_RetireFollowerObjectOnReturnToField(u8 objectEventId);
const struct CoopPresenceReducer *CoopPresenceRuntime_GetReducer(void);

#endif /* GUARD_COOP_PRESENCE_RUNTIME_H */
