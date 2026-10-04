#ifndef GUARD_CORMORIA_EXTRA_FLAGS_H
#define GUARD_CORMORIA_EXTRA_FLAGS_H

#include "constants/world_events.h"

/* A copied debug script uses this Dreamstone-only system flag. Keep it
 * after the 483 authenticated campaign flags in this ROM's own event record. */
#define Cormoria_FLAG_VISITED_RIVETSHORE_RANGER (WORLD_EVENT_FLAG_START + 483)

/* Ordinals 484–485 belong to Derby's persistent state. Gastree's leader
 * reward uses the next world-local flag so its receipt survives item moves. */
#define Cormoria_FLAG_GASTREEGYM_LEADER_RARE_SHARD_RECEIVED (WORLD_EVENT_FLAG_START + 486)

#endif // GUARD_CORMORIA_EXTRA_FLAGS_H
