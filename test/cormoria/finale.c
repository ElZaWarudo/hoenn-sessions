#include "global.h"
#include "event_data.h"
#include "load_save.h"
#include "script.h"
#include "string_util.h"
#include "constants/cormoria_event_ids.h"
#include "test/test.h"
#include "world/event_save.h"

#if ROM_WORLD == 2
extern const u8 Cormoria_ChampionshipVenue_SignUp[];
extern const u8 Cormoria_ChampionshipVenue_Entry[];
extern const u8 Cormoria_CeramBaseCamp_EventScript_ExitTrigger[];
extern const u8 Cormoria_CarabrueTown_TenebrisLab_EventScript_Scientist3[];

static const u8 sThanks[] = _("Thank you for participating in this\nyear's Championship!");
static const u8 sClosed[] = _("Welcome, trainer, to this years\nCormoria Pokémon Championships!\lRegistration will open shortly.");
static const u8 sRegistered[] = _("Please head to your designated battle\nhall through the right.");
static const u8 sRegistration[] = _("Welcome, trainer, to this years\nCormoria Pokémon Championships!\pAre you here to register your\nparticipation?");
static const u8 sMatch[] = _("Welcome trainer!\nAre you ready for your match?");
static const u8 sNotRegistered[] = _("Sorry, only participating trainers may\nenter past this point.");
static const u8 sCleared[] = _("You did it! You actually did it!");
static const u8 sFinale[] = _("Go on, {PLAYER}! Bring the\nchampionship trophy to Carabrue Town!");
static const u8 sPartner[] = _("A bit unorthodox for a researcher, but\nyou and your new partner are looking");
static const u8 sBeforePartner[] = _("I'm sorry we couldn't offer you the\nstandard companion Pokémon.");

static void ResetGateFixture(void)
{
    SetSaveBlocksPointers(0);
    WorldEventSave_InitializeCurrent();
    FlagClear(Cormoria_FLAG_SYS_GAME_CLEAR);
    FlagClear(Cormoria_FLAG_CHAMPIONSHIP_KOHLA_ROOM);
    FlagClear(Cormoria_FLAG_CHAMP_SIGNUP_DONE);
    FlagClear(Cormoria_FLAG_LAB_CALLTOACTION);
    FlagClear(Cormoria_FLAG_POST_FINALE_CUTSCENE);
    FlagClear(Cormoria_FLAG_BADGE02_GET);
    FlagClear(FLAG_SYS_GAME_CLEAR);
    FlagClear(FLAG_BADGE02_GET);
}

static void ExpectMessageBoundary(const u8 *entry, const u8 *prefix, u8 command)
{
    struct ScriptContext ctx;
    EXPECT(RunScriptImmediatelyUntilEffect(SCREFF_V1 | SCREFF_SAVE | SCREFF_HARDWARE, entry, &ctx));
    EXPECT(ctx.scriptPtr != NULL);
    EXPECT_EQ(*ctx.scriptPtr, command);
    EXPECT(ctx.data[0] != 0);
    // The interpreter selected the production text before its first UI effect.
    EXPECT_EQ(StringCompareN((const u8 *)ctx.data[0], prefix, StringLength(prefix)), 0);
}

TEST("Cormoria finale signup gates use regional completion and signup precedence")
{
    u32 i, state = 0;
    for (i = 0; i < 8; i++)
        PARAMETRIZE { state = i; }
    ResetGateFixture();
    if (state >= 1 && state <= 4)
        FlagSet(Cormoria_FLAG_CHAMPIONSHIP_KOHLA_ROOM);
    if (state == 2 || state == 4 || state == 5)
        FlagSet(Cormoria_FLAG_CHAMP_SIGNUP_DONE);
    if (state == 3 || state == 4 || state == 7)
        FlagSet(Cormoria_FLAG_SYS_GAME_CLEAR);
    if (state == 6)
        FlagSet(FLAG_SYS_GAME_CLEAR);
    ExpectMessageBoundary(Cormoria_ChampionshipVenue_SignUp,
                          state == 3 || state == 4 || state == 7 ? sThanks : state == 2 ? sRegistered : state == 1 ? sRegistration : sClosed,
                          state == 1 ? 0x67 : 0x6A);
}

TEST("Cormoria finale match entry requires regional signup and prioritizes completion")
{
    u32 i, state = 0;
    for (i = 0; i < 4; i++)
        PARAMETRIZE { state = i; }
    ResetGateFixture();
    if (state & 1)
        FlagSet(Cormoria_FLAG_CHAMP_SIGNUP_DONE);
    if (state & 2)
        FlagSet(Cormoria_FLAG_SYS_GAME_CLEAR);
    ExpectMessageBoundary(Cormoria_ChampionshipVenue_Entry,
                          state & 2 ? sThanks : state & 1 ? sMatch : sNotRegistered,
                          state == 1 ? 0x67 : 0x6A);
}

TEST("Cormoria finale scientist distinguishes partner finale and regional clear")
{
    u32 i, state = 0;
    for (i = 0; i < 6; i++)
        PARAMETRIZE { state = i; }
    ResetGateFixture();
    if (state >= 1 && state <= 3)
        FlagSet(Cormoria_FLAG_LAB_CALLTOACTION);
    if (state == 2 || state == 3 || state == 5)
        FlagSet(Cormoria_FLAG_POST_FINALE_CUTSCENE);
    if (state == 3)
        FlagSet(Cormoria_FLAG_SYS_GAME_CLEAR);
    if (state == 4)
        FlagSet(FLAG_SYS_GAME_CLEAR);
    ExpectMessageBoundary(Cormoria_CarabrueTown_TenebrisLab_EventScript_Scientist3,
                          state == 3 ? sCleared : state == 2 || state == 5 ? sFinale : state == 1 ? sPartner : sBeforePartner,
                          0x6A);
}

TEST("Cormoria campaign Ceram exit requires its own second badge")
{
    u32 i, state = 0;
    struct ScriptContext ctx;
    for (i = 0; i < 3; i++)
        PARAMETRIZE { state = i; }
    ResetGateFixture();
    if (state == 1)
        FlagSet(FLAG_BADGE02_GET);
    if (state == 2)
        FlagSet(Cormoria_FLAG_BADGE02_GET);
    if (state == 2)
    {
        EXPECT(!RunScriptImmediatelyUntilEffect(SCREFF_V1 | SCREFF_SAVE | SCREFF_HARDWARE,
                                              Cormoria_CeramBaseCamp_EventScript_ExitTrigger, &ctx));
        EXPECT_EQ(ctx.scriptPtr, NULL);
    }
    else
    {
        EXPECT(RunScriptImmediatelyUntilEffect(SCREFF_V1 | SCREFF_SAVE | SCREFF_HARDWARE,
                                             Cormoria_CeramBaseCamp_EventScript_ExitTrigger, &ctx));
        EXPECT(ctx.scriptPtr != NULL);
        EXPECT_EQ(*ctx.scriptPtr, 0x69); // lockall, before movement or dialogue.
    }
}
#endif
