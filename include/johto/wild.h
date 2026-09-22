#ifndef GUARD_JOHTO_WILD_H
#define GUARD_JOHTO_WILD_H

#include "global.h"
#include "constants/rtc.h"

enum TimeOfDay JohtoWild_TimeForHour(u32 hour);
enum TimeOfDay JohtoWild_CurrentTime(void);

#endif // GUARD_JOHTO_WILD_H
