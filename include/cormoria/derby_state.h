#ifndef GUARD_CORMORIA_DERBY_STATE_H
#define GUARD_CORMORIA_DERBY_STATE_H

#include "constants/world_events.h"

/* The imported campaign occupies world-local vars 0–42 and flags 0–482.
 * Derby's persistent race roster is isolated from both the shared player
 * state and other regions' story variables. */
#define DERBY_VAR_RACER_1      (WORLD_EVENT_VAR_START + 43)
#define DERBY_VAR_RACER_2      (WORLD_EVENT_VAR_START + 44)
#define DERBY_VAR_RACER_3      (WORLD_EVENT_VAR_START + 45)
#define DERBY_VAR_RACER_4      (WORLD_EVENT_VAR_START + 46)
#define DERBY_VAR_RACER_5      (WORLD_EVENT_VAR_START + 47)
#define DERBY_VAR_RACER_6      (WORLD_EVENT_VAR_START + 48)
#define DERBY_VAR_RACER_NAME_1 (WORLD_EVENT_VAR_START + 49)
#define DERBY_VAR_RACER_NAME_2 (WORLD_EVENT_VAR_START + 50)
#define DERBY_VAR_RACER_NAME_3 (WORLD_EVENT_VAR_START + 51)
#define DERBY_VAR_RACER_NAME_4 (WORLD_EVENT_VAR_START + 52)
#define DERBY_VAR_RACER_NAME_5 (WORLD_EVENT_VAR_START + 53)
#define DERBY_VAR_RACER_NAME_6 (WORLD_EVENT_VAR_START + 54)

/* Ordinal 483 is the Cormoria debug flag. */
#define DERBY_FLAG_RESET    (WORLD_EVENT_FLAG_START + 484)
#define DERBY_FLAG_NICKNAME (WORLD_EVENT_FLAG_START + 485)

#endif // GUARD_CORMORIA_DERBY_STATE_H
