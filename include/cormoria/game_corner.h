#ifndef GUARD_CORMORIA_GAME_CORNER_H
#define GUARD_CORMORIA_GAME_CORNER_H

#include "constants/vars.h"

/* The donor's game-corner reward variable is world-local in each ROM save. */
#define GAME_CORNER_VAR_WINNINGS VAR_GIFT_UNUSED_2

/* Dreamstone's pinned game-corner configuration disables these optional vars. */
#define GAME_CORNER_VAR_ID_CHECK 0
#define FLIP_VAR_LEVEL 0

#endif /* GUARD_CORMORIA_GAME_CORNER_H */
