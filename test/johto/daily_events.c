#include "global.h"
#include "event_data.h"
#include "johto/daily_events.h"
#include "johto/events.h"
#include "johto/save.h"
#include "load_save.h"
#include "constants/flags.h"
#include "constants/johto_content.h"
#include "constants/vars.h"
#include "test/test.h"

static const u16 sSelectedFlags[] =
{
    JOHTO_FLAG_DAILY_BUG_CONTEST_COMPLETED,
    JOHTO_FLAG_DAILY_HAIRCUT1_RECEIVED,
    JOHTO_FLAG_DAILY_HAIRCUT2_RECEIVED,
    JOHTO_FLAG_DAILY_PICKED_LOTO_TICKET,
};

static void ResetDailyFixture(void)
{
    SetSaveBlocksPointers(0);
    InitEventData();
    JohtoSave_InitializeCurrent();
}

TEST("Johto daily flags clear through the host hook without touching unrelated state")
{
    u32 i;

    ResetDailyFixture();
    for (i = 0; i < ARRAY_COUNT(sSelectedFlags); i++)
    {
        EXPECT(JohtoEvent_SetFlag(sSelectedFlags[i], TRUE));
        EXPECT(JohtoEvent_GetFlag(sSelectedFlags[i]));
    }
    FlagSet(FLAG_DAILY_CONTEST_LOBBY_RECEIVED_BERRY);
    FlagSet(FLAG_DAILY_PICKED_LOTO_TICKET);
    FlagSet(FLAG_SYS_POKEDEX_GET);
    EXPECT(JohtoEvent_SetFlag(JOHTO_FLAG_COMPLETED_HOOH_PUZZLE, TRUE));
    EXPECT(JohtoEvent_SetVariable(JOHTO_VAR_BUG_CONTEST_STATE, 0xCAFE));

    ClearDailyFlags();
    for (i = 0; i < ARRAY_COUNT(sSelectedFlags); i++)
        EXPECT(!JohtoEvent_GetFlag(sSelectedFlags[i]));
    EXPECT(!FlagGet(FLAG_DAILY_CONTEST_LOBBY_RECEIVED_BERRY));
    EXPECT(!FlagGet(FLAG_DAILY_PICKED_LOTO_TICKET));
    EXPECT(FlagGet(FLAG_SYS_POKEDEX_GET));
    EXPECT(JohtoEvent_GetFlag(JOHTO_FLAG_COMPLETED_HOOH_PUZZLE));
    EXPECT_EQ(JohtoEvent_GetVariable(JOHTO_VAR_BUG_CONTEST_STATE), 0xCAFE);

    ClearDailyFlags();
    for (i = 0; i < ARRAY_COUNT(sSelectedFlags); i++)
        EXPECT(!JohtoEvent_GetFlag(sSelectedFlags[i]));
    EXPECT(FlagGet(FLAG_SYS_POKEDEX_GET));
    EXPECT(JohtoEvent_GetFlag(JOHTO_FLAG_COMPLETED_HOOH_PUZZLE));
    EXPECT_EQ(JohtoEvent_GetVariable(JOHTO_VAR_BUG_CONTEST_STATE), 0xCAFE);
}

TEST("Johto-only daily clearing leaves legacy daily flags alone")
{
    ResetDailyFixture();
    FlagSet(FLAG_DAILY_CONTEST_LOBBY_RECEIVED_BERRY);
    EXPECT(JohtoEvent_SetFlag(JOHTO_FLAG_DAILY_PICKED_LOTO_TICKET, TRUE));

    JohtoDailyEvents_ClearFlags();

    EXPECT(FlagGet(FLAG_DAILY_CONTEST_LOBBY_RECEIVED_BERRY));
    EXPECT(!JohtoEvent_GetFlag(JOHTO_FLAG_DAILY_PICKED_LOTO_TICKET));
}
