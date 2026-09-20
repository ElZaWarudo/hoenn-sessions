#include "global.h"
#include "johto/daily_events.h"
#include "johto/events.h"
#include "constants/johto_content.h"

static const u16 sJohtoDailyFlags[] =
{
    JOHTO_FLAG_DAILY_BUG_CONTEST_COMPLETED,
    JOHTO_FLAG_DAILY_HAIRCUT1_RECEIVED,
    JOHTO_FLAG_DAILY_HAIRCUT2_RECEIVED,
    JOHTO_FLAG_DAILY_PICKED_LOTO_TICKET,
};

void JohtoDailyEvents_ClearFlags(void)
{
    u32 i;

    for (i = 0; i < ARRAY_COUNT(sJohtoDailyFlags); i++)
        JohtoEvent_SetFlag(sJohtoDailyFlags[i], FALSE);
}
