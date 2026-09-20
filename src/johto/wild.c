#include "global.h"
#include "johto/wild.h"
#include "rtc.h"

enum TimeOfDay JohtoWild_TimeForHour(u32 hour)
{
    if (hour >= HOURS_PER_DAY)
        return TIME_DAY;

    if (hour >= 6 && hour < 18)
        return TIME_DAY;

    return TIME_NIGHT;
}

enum TimeOfDay JohtoWild_CurrentTime(void)
{
    RtcCalcLocalTime();
    return JohtoWild_TimeForHour(gLocalTime.hours);
}
