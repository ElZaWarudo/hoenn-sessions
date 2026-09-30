#include "global.h"
#include "coop/friendly_battle.h"
#include "coop/battle_consent.h"
#include "coop/battle_runtime.h"
#include "coop/net_bridge.h"
#include "coop/trade_offer.h"
#include "coop/trade_runtime.h"
#include "battle_caps.h"
#include "event_data.h"
#include "main.h"
#include "pokemon.h"
#include "script.h"
#include "string_util.h"
#include "task.h"
#include "constants/characters.h"
#include "constants/species.h"

struct CoopFriendlyBattle
{
    struct CoopBattleFriendlyRules rules;
    u8 team[PARTY_SIZE];
    u8 picked;
    u8 phase;
    u8 result;
};

static EWRAM_DATA struct CoopFriendlyBattle sFriendly = {0};

extern const u8 EventScript_CoopFriendlyChallenge[];

static const u8 sText_Singles[] = _("Singles");
static const u8 sText_Doubles[] = _("Doubles");
static const u8 sText_LevelsAsIs[] = _("levels as is");
static const u8 sText_Levels50[] = _("all at Lv. 50");
static const u8 sText_Cancelled[] = _("Battle cancelled.");
static const u8 sText_Declined[] = _("Your partner declined\nthe battle.");
static const u8 sText_NoAnswer[] = _("Your partner didn't answer.");
static const u8 sText_Withdrawn[] = _("Your partner called off\nthe battle.");
static const u8 sText_Unavailable[] = _("The battle couldn't start.\nTry again later.");
static const u8 sText_TimedOut[] = _("The battle didn't start\nin time.");
static const u8 sText_Won[] = _("You won the battle!");
static const u8 sText_Lost[] = _("You lost the battle.");
static const u8 sText_Draw[] = _("The battle ended in a draw.");
static const u8 sText_NoContest[] = _("The battle ended with\nno result.");

static const struct CoopBattleFriendlyRules sDefaultRules =
{
    .format = COOP_BATTLE_FRIENDLY_SINGLES,
    .level_mode = COOP_BATTLE_FRIENDLY_LEVELS_AS_IS,
    .count = 1,
};

void CoopFriendly_Init(void)
{
    memset(&sFriendly, 0, sizeof(sFriendly));
}

enum CoopFriendlyPhase CoopFriendly_GetPhase(void)
{
    return sFriendly.phase;
}

u8 CoopFriendly_GetResult(void)
{
    return sFriendly.result;
}

void CoopFriendly_GetRules(struct CoopBattleFriendlyRules *rules)
{
    if (rules == NULL)
        return;
    if (sFriendly.phase == COOP_FRIENDLY_IDLE
     || !CoopBattleRuntime_IsValidFriendlyRules(&sFriendly.rules))
        *rules = sDefaultRules;
    else
        *rules = sFriendly.rules;
}

static bool8 IsBattleReady(struct Pokemon *mon)
{
    return GetMonData(mon, MON_DATA_SANITY_HAS_SPECIES)
        && !GetMonData(mon, MON_DATA_SANITY_IS_BAD_EGG)
        && !GetMonData(mon, MON_DATA_IS_EGG)
        && GetMonData(mon, MON_DATA_HP) != 0;
}

u8 CoopFriendly_CountUsableMons(void)
{
    u8 i;
    u8 count = 0;

    for (i = 0; i < gPlayerPartyCount && i < PARTY_SIZE; i++)
        if (IsBattleReady(&gPlayerParty[i]))
            count++;
    return count;
}

bool8 CoopFriendly_CanBegin(void)
{
    return sFriendly.phase == COOP_FRIENDLY_IDLE
        && CoopNetBridge_IsGrouped()
        && CoopNetBridge_CanSendBattle()
        && !CoopBattleRuntime_HasPendingOutboundReplay()
        && !CoopBattleRuntime_HasManifest()
        && !CoopBattleRuntime_IsEngineActive()
        && CoopBattleConsent_IsIdle()
        && CoopTradeOffer_GetState() == COOP_TRADE_OFFER_IDLE
        && CoopTradeRuntime_GetState() == COOP_TRADE_STATE_IDLE
        && CoopFriendly_CountUsableMons() != 0;
}

static void StartPicking(const struct CoopBattleFriendlyRules *rules)
{
    memset(&sFriendly, 0, sizeof(sFriendly));
    sFriendly.rules = *rules;
    sFriendly.phase = COOP_FRIENDLY_PICKING;
}

bool8 CoopFriendly_BeginChallenge(const struct CoopBattleFriendlyRules *rules)
{
    if (!CoopBattleRuntime_IsValidFriendlyRules(rules)
     || rules->count > CoopFriendly_CountUsableMons()
     || !CoopFriendly_CanBegin())
    {
        if (sFriendly.phase == COOP_FRIENDLY_IDLE)
        {
            memset(&sFriendly, 0, sizeof(sFriendly));
            sFriendly.phase = COOP_FRIENDLY_DONE;
            sFriendly.result = COOP_FRIENDLY_RESULT_UNAVAILABLE;
        }
        return FALSE;
    }
    StartPicking(rules);
    if (!CoopBattleConsent_BeginFriendly(rules))
    {
        sFriendly.phase = COOP_FRIENDLY_DONE;
        sFriendly.result = COOP_FRIENDLY_RESULT_UNAVAILABLE;
        return FALSE;
    }
    return TRUE;
}

void CoopFriendly_StartChallengeScript(const struct CoopBattleFriendlyRules *rules)
{
    /* The script's first step sends the challenge with these rules. */
    memset(&sFriendly, 0, sizeof(sFriendly));
    sFriendly.rules = *rules;
    ScriptContext_SetupScript(EventScript_CoopFriendlyChallenge);
}

bool8 CoopFriendly_BeginResponderPicks(const struct CoopBattleFriendlyRules *rules)
{
    if (!CoopBattleRuntime_IsValidFriendlyRules(rules) || sFriendly.phase != COOP_FRIENDLY_IDLE)
        return FALSE;
    StartPicking(rules);
    if (rules->count > CoopFriendly_CountUsableMons())
    {
        /* Accepted before the party could be checked; call it off now. */
        CoopBattleConsent_CancelFriendly();
        sFriendly.phase = COOP_FRIENDLY_DONE;
        sFriendly.result = COOP_FRIENDLY_RESULT_UNAVAILABLE;
        return FALSE;
    }
    return TRUE;
}

u8 CoopFriendly_PickMon(u8 slot)
{
    u8 i;

    if (sFriendly.phase != COOP_FRIENDLY_PICKING)
        return COOP_FRIENDLY_PICK_ENDED;
    if (slot >= PARTY_SIZE)
    {
        CoopBattleConsent_CancelFriendly();
        sFriendly.phase = COOP_FRIENDLY_DONE;
        sFriendly.result = COOP_FRIENDLY_RESULT_CANCELLED;
        return COOP_FRIENDLY_PICK_CANCELLED;
    }
    if (sFriendly.picked >= sFriendly.rules.count || slot >= gPlayerPartyCount
     || !IsBattleReady(&gPlayerParty[slot]))
        return COOP_FRIENDLY_PICK_REFUSED;
    for (i = 0; i < sFriendly.picked; i++)
        if (sFriendly.team[i] == slot)
            return COOP_FRIENDLY_PICK_REFUSED;
    sFriendly.team[sFriendly.picked++] = slot;
    return COOP_FRIENDLY_PICK_OK;
}

u8 CoopFriendly_PickedCount(void)
{
    return sFriendly.picked;
}

bool8 CoopFriendly_IsTeamComplete(void)
{
    return (sFriendly.phase == COOP_FRIENDLY_PICKING || sFriendly.phase == COOP_FRIENDLY_WAITING)
        && sFriendly.rules.count != 0 && sFriendly.picked == sFriendly.rules.count;
}

u8 CoopFriendly_BuildTeam(struct Pokemon *team)
{
    u8 i;

    if (team == NULL || !CoopFriendly_IsTeamComplete())
        return 0;
    for (i = 0; i < sFriendly.picked; i++)
    {
        if (sFriendly.team[i] >= gPlayerPartyCount || !IsBattleReady(&gPlayerParty[sFriendly.team[i]]))
            return 0;
        team[i] = gPlayerParty[sFriendly.team[i]];
    }
    return sFriendly.picked;
}

bool8 CoopFriendly_IsWaiting(void)
{
    return sFriendly.phase == COOP_FRIENDLY_WAITING;
}

void CoopFriendly_BeginWaiting(void)
{
    if (sFriendly.phase == COOP_FRIENDLY_PICKING && CoopFriendly_IsTeamComplete())
        sFriendly.phase = COOP_FRIENDLY_WAITING;
}

void CoopFriendly_End(u8 result)
{
    if (sFriendly.phase != COOP_FRIENDLY_PICKING && sFriendly.phase != COOP_FRIENDLY_WAITING)
        return;
    sFriendly.phase = COOP_FRIENDLY_DONE;
    sFriendly.result = result;
}

void CoopFriendly_OnBattleStarted(void)
{
    sFriendly.phase = COOP_FRIENDLY_IN_BATTLE;
}

void CoopFriendly_OnBattleEnded(u8 result)
{
    sFriendly.phase = COOP_FRIENDLY_DONE;
    sFriendly.result = result;
}

const u8 *CoopFriendly_GetResultText(void)
{
    switch (sFriendly.result)
    {
    case COOP_FRIENDLY_RESULT_CANCELLED: return sText_Cancelled;
    case COOP_FRIENDLY_RESULT_DECLINED: return sText_Declined;
    case COOP_FRIENDLY_RESULT_NO_ANSWER: return sText_NoAnswer;
    case COOP_FRIENDLY_RESULT_WITHDRAWN: return sText_Withdrawn;
    case COOP_FRIENDLY_RESULT_TIMED_OUT: return sText_TimedOut;
    case COOP_FRIENDLY_RESULT_WON: return sText_Won;
    case COOP_FRIENDLY_RESULT_LOST: return sText_Lost;
    case COOP_FRIENDLY_RESULT_DRAW: return sText_Draw;
    case COOP_FRIENDLY_RESULT_NO_CONTEST: return sText_NoContest;
    default: return sText_Unavailable;
    }
}

void CoopFriendly_Finish(void)
{
    if (sFriendly.phase == COOP_FRIENDLY_DONE)
        memset(&sFriendly, 0, sizeof(sFriendly));
}

void CoopFriendly_ScaleTeam(struct Pokemon *team, u8 count, u8 level_mode)
{
    u8 i;

    if (team == NULL || level_mode != COOP_BATTLE_FRIENDLY_LEVELS_50)
        return;
    for (i = 0; i < count && i < PARTY_SIZE; i++)
        BattleCaps_SetMonLevel(&team[i], COOP_BATTLE_FRIENDLY_LEVEL);
}

void CoopFriendly_FillOpponentDisplay(struct SecretBase *base)
{
    struct Pokemon *lead = &gParties[B_TRAINER_1][0];
    u8 name[PLAYER_NAME_LENGTH + 8];
    u32 otId;

    if (base == NULL)
        return;
    memset(base, 0, sizeof(*base));
    memset(name, EOS, sizeof(name));
    GetMonData(lead, MON_DATA_OT_NAME, name);
    memcpy(base->trainerName, name, PLAYER_NAME_LENGTH);
    base->gender = GetMonData(lead, MON_DATA_OT_GENDER);
    otId = GetMonData(lead, MON_DATA_OT_ID);
    base->trainerId[0] = otId;
    base->trainerId[1] = otId >> 8;
    base->trainerId[2] = otId >> 16;
    base->trainerId[3] = otId >> 24;
    base->language = GetMonData(lead, MON_DATA_LANGUAGE);
}

const u8 *CoopFriendly_FormatName(u8 format)
{
    return format == COOP_BATTLE_FRIENDLY_DOUBLES ? sText_Doubles : sText_Singles;
}

const u8 *CoopFriendly_LevelModeName(u8 level_mode)
{
    return level_mode == COOP_BATTLE_FRIENDLY_LEVELS_50 ? sText_Levels50 : sText_LevelsAsIs;
}

static void BufferRules(const struct CoopBattleFriendlyRules *rules)
{
    StringCopy(gStringVar1, CoopFriendly_FormatName(rules->format));
    StringCopy(gStringVar2, CoopFriendly_LevelModeName(rules->level_mode));
    ConvertIntToDecimalStringN(gStringVar3, rules->count, STR_CONV_MODE_LEFT_ALIGN, 1);
}

/* The partner's prompt: the rules of the offer on screen. */
void Special_CoopFriendlyBufferRules(void)
{
    struct CoopBattleFriendlyRules rules;

    if (!CoopBattleConsent_GetOfferRules(&rules))
        rules = sDefaultRules;
    BufferRules(&rules);
    /* FALSE: this party cannot field the challenge's team. */
    gSpecialVar_Result = rules.count <= CoopFriendly_CountUsableMons();
}

/* After the partner's Yes: VAR_RESULT TRUE continues with the team picks. */
void Special_CoopFriendlyBeginResponderPicks(void)
{
    struct CoopBattleFriendlyRules rules;

    if (!CoopBattleConsent_GetOfferRules(&rules) || !CoopBattleConsent_TakeFriendlyPromptLock())
    {
        memset(&sFriendly, 0, sizeof(sFriendly));
        sFriendly.phase = COOP_FRIENDLY_DONE;
        sFriendly.result = COOP_FRIENDLY_RESULT_UNAVAILABLE;
        gSpecialVar_Result = FALSE;
        return;
    }
    gSpecialVar_Result = CoopFriendly_BeginResponderPicks(&rules);
}

/* VAR_RESULT: 0 the flow ended (show the result), 1 pick the next Pokemon
 * (STR_VAR_1 is its number, STR_VAR_2 the team size), 2 the team is done.
 * The first call on the challenger's side sends the challenge. */
void Special_CoopFriendlyBufferPick(void)
{
    if (sFriendly.phase == COOP_FRIENDLY_IDLE)
    {
        struct CoopBattleFriendlyRules rules = sFriendly.rules;

        if (!CoopFriendly_BeginChallenge(&rules))
        {
            gSpecialVar_Result = 0;
            return;
        }
    }
    if (sFriendly.phase != COOP_FRIENDLY_PICKING)
    {
        gSpecialVar_Result = 0;
        return;
    }
    if (CoopFriendly_IsTeamComplete())
    {
        gSpecialVar_Result = 2;
        return;
    }
    ConvertIntToDecimalStringN(gStringVar1, sFriendly.picked + 1, STR_CONV_MODE_LEFT_ALIGN, 1);
    ConvertIntToDecimalStringN(gStringVar2, sFriendly.rules.count, STR_CONV_MODE_LEFT_ALIGN, 1);
    gSpecialVar_Result = 1;
}

void Special_CoopFriendlyPickMon(void)
{
    gSpecialVar_Result = CoopFriendly_PickMon(gSpecialVar_0x8004);
}

static void Task_CoopFriendlyWait(u8 taskId)
{
    if (sFriendly.phase == COOP_FRIENDLY_WAITING && JOY_NEW(B_BUTTON))
    {
        CoopBattleConsent_CancelFriendly();
        CoopFriendly_End(COOP_FRIENDLY_RESULT_CANCELLED);
    }
    if (sFriendly.phase == COOP_FRIENDLY_IN_BATTLE)
    {
        /* The battle owns the parked script; it resumes on return. */
        DestroyTask(taskId);
        return;
    }
    if (sFriendly.phase != COOP_FRIENDLY_WAITING)
    {
        DestroyTask(taskId);
        ScriptContext_Enable();
    }
}

/* Used with waitstate: the field stays locked until the battle starts (the
 * script resumes after it) or the flow ends. */
void Special_CoopFriendlyWait(void)
{
    CoopFriendly_BeginWaiting();
    CreateTask(Task_CoopFriendlyWait, 80);
}

void Special_CoopFriendlyBufferResult(void)
{
    StringCopy(gStringVar4, CoopFriendly_GetResultText());
    gSpecialVar_Result = sFriendly.result;
}

void Special_CoopFriendlyFinish(void)
{
    CoopFriendly_Finish();
}
