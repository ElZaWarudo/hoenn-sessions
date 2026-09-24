#ifndef GUARD_CORMORIA_EXTRA_FLAGS_H
#define GUARD_CORMORIA_EXTRA_FLAGS_H

#include "constants/world_events.h"

/* A copied debug script uses this Dreamstone-only system flag. Keep it
 * after the 483 authenticated campaign flags in this ROM's own event record. */
#define Cormoria_FLAG_VISITED_RIVETSHORE_RANGER (WORLD_EVENT_FLAG_START + 483)

#endif // GUARD_CORMORIA_EXTRA_FLAGS_H
