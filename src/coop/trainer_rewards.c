#include "global.h"
#include "coop/trainer_rewards.h"
#include "coop/battle_consent.h"
#include "coop/battle_runtime.h"
#include "battle.h"
#include "battle_controllers.h"
#include "battle_script_commands.h"
#include "battle_setup.h"
#include "battle_util.h"
#include "caps.h"
#include "data.h"
#include "evolution_scene.h"
#include "event_data.h"
#include "event_object_movement.h"
#include "item.h"
#include "main.h"
#include "mastery.h"
#include "money.h"
#include "overworld.h"
#include "pokemon.h"
#include "script.h"
#include "window.h"
#include "constants/battle.h"
#include "constants/flags.h"
#include "constants/hold_effects.h"
#include "constants/items.h"
#include "constants/maps.h"
#include "constants/opponents.h"
#include "constants/trainers.h"
#include "constants/vars.h"

_Static_assert(COOP_BATTLE_MULTI_PARTY_SIZE <= 3,
               "a faint record holds three staged-slot bits per field");

/* 14 bytes of EWRAM: everything else is derived after the battle. */
struct CoopTrainerRewardState
{
    u8 faints[PARTY_SIZE]; // per opponent party slot, see COOP_TRAINER_REWARD_*
    u8 sent[2];            // per opponent flank: local staged slots sent in
    bool8 armed;
    u8 money_multiplier;
    u8 gym_notice;         // GYM_NOTICE_* after a GYM_PARTNER settlement
    u8 story_notice;       // STORY_NOTICE_* after a STORY_PARTNER settlement
    u16 story_hide_mask;   // gObjectEvents slots the story grant hid
};

#define GYM_NOTICE_PENDING  0x80
#define GYM_NOTICE_TM_GIVEN 0x40
#define GYM_NOTICE_GYM_MASK 0x07

#define STORY_NOTICE_PENDING     0x80
#define STORY_NOTICE_ITEM_GIVEN  0x40
#define STORY_NOTICE_BATTLE_MASK 0x1F

_Static_assert(COOP_STORY_COUNT <= STORY_NOTICE_BATTLE_MASK + 1,
               "a story notice holds the battle in five bits");
_Static_assert(OBJECT_EVENTS_COUNT <= 16, "one hide-mask bit per object event");

static EWRAM_DATA struct CoopTrainerRewardState sRewards = {0};

/* The partner's share of each Hoenn gym's post-battle script
 * (data/maps/<Gym>/scripts.inc: <Leader>Defeated and Give<TM>). The grant
 * script applies every flag, var, gym-trainer flag and special it runs; the
 * TM and its received flag are given here; the notice script shows the
 * badge message, fanfare and TM. See data/scripts/coop_gym_rewards.inc. */
struct CoopHoennGym
{
    u16 leader;
    u16 tm;
    u16 tmFlag;
    const u8 *grantScript;
    const u8 *noticeScript;
};

extern const u8 CoopGym_EventScript_GrantRustboro[];
extern const u8 CoopGym_EventScript_GrantDewford[];
extern const u8 CoopGym_EventScript_GrantMauville[];
extern const u8 CoopGym_EventScript_GrantLavaridge[];
extern const u8 CoopGym_EventScript_GrantPetalburg[];
extern const u8 CoopGym_EventScript_GrantFortree[];
extern const u8 CoopGym_EventScript_GrantMossdeep[];
extern const u8 CoopGym_EventScript_GrantSootopolis[];
extern const u8 CoopGym_EventScript_NoticeRustboro[];
extern const u8 CoopGym_EventScript_NoticeDewford[];
extern const u8 CoopGym_EventScript_NoticeMauville[];
extern const u8 CoopGym_EventScript_NoticeLavaridge[];
extern const u8 CoopGym_EventScript_NoticePetalburg[];
extern const u8 CoopGym_EventScript_NoticeFortree[];
extern const u8 CoopGym_EventScript_NoticeMossdeep[];
extern const u8 CoopGym_EventScript_NoticeSootopolis[];

static const struct CoopHoennGym sHoennGyms[COOP_HOENN_GYM_COUNT] =
{
    {TRAINER_ROXANNE_1, ITEM_TM_ROCK_TOMB, FLAG_RECEIVED_TM_ROCK_TOMB,
     CoopGym_EventScript_GrantRustboro, CoopGym_EventScript_NoticeRustboro},
    {TRAINER_BRAWLY_1, ITEM_TM_BULK_UP, FLAG_RECEIVED_TM_BULK_UP,
     CoopGym_EventScript_GrantDewford, CoopGym_EventScript_NoticeDewford},
    {TRAINER_WATTSON_1, ITEM_TM_SHOCK_WAVE, FLAG_RECEIVED_TM_SHOCK_WAVE,
     CoopGym_EventScript_GrantMauville, CoopGym_EventScript_NoticeMauville},
    {TRAINER_FLANNERY_1, ITEM_TM_OVERHEAT, FLAG_RECEIVED_TM_OVERHEAT,
     CoopGym_EventScript_GrantLavaridge, CoopGym_EventScript_NoticeLavaridge},
    {TRAINER_NORMAN_1, ITEM_TM_FACADE, FLAG_RECEIVED_TM_FACADE,
     CoopGym_EventScript_GrantPetalburg, CoopGym_EventScript_NoticePetalburg},
    {TRAINER_WINONA_1, ITEM_TM_AERIAL_ACE, FLAG_RECEIVED_TM_AERIAL_ACE,
     CoopGym_EventScript_GrantFortree, CoopGym_EventScript_NoticeFortree},
    {TRAINER_TATE_AND_LIZA_1, ITEM_TM_CALM_MIND, FLAG_RECEIVED_TM_CALM_MIND,
     CoopGym_EventScript_GrantMossdeep, CoopGym_EventScript_NoticeMossdeep},
    {TRAINER_JUAN_1, ITEM_TM_WATER_PULSE, FLAG_RECEIVED_TM_WATER_PULSE,
     CoopGym_EventScript_GrantSootopolis, CoopGym_EventScript_NoticeSootopolis},
};

_Static_assert(FLAG_BADGE08_GET - FLAG_BADGE01_GET + 1 == COOP_HOENN_GYM_COUNT,
               "one badge flag per Hoenn gym, in gym order");

u8 CoopTrainerRewards_GetHoennGym(u16 trainerId)
{
    u8 i;

    if (trainerId == TRAINER_NONE)
        return COOP_HOENN_GYM_NONE;
    for (i = 0; i < COOP_HOENN_GYM_COUNT; i++)
        if (sHoennGyms[i].leader == trainerId)
            return i;
    return COOP_HOENN_GYM_NONE;
}

bool8 CoopTrainerRewards_IsHoennGymLeader(u16 trainerId)
{
    u16 base;

    if (CoopTrainerRewards_GetHoennGym(trainerId) != COOP_HOENN_GYM_NONE)
        return TRUE;
    return BattleSetup_GetRematchBaseTrainer(trainerId, &base)
        && CoopTrainerRewards_GetHoennGym(base) != COOP_HOENN_GYM_NONE;
}

/* Exactly the badges before this gym: every earlier one, this one and no
 * later one missing (the badge count equals the gym's index). */
bool8 CoopTrainerRewards_IsGymPartnerEligible(u8 gym)
{
    u8 i;

    if (gym >= COOP_HOENN_GYM_COUNT)
        return FALSE;
    for (i = 0; i < COOP_HOENN_GYM_COUNT; i++)
        if (FlagGet(FLAG_BADGE01_GET + i) != (i < gym))
            return FALSE;
    return TRUE;
}

const u8 *CoopTrainerRewards_GetGymNoticeScript(u16 trainerId)
{
    u8 gym = CoopTrainerRewards_GetHoennGym(trainerId);

    return gym == COOP_HOENN_GYM_NONE ? NULL : sHoennGyms[gym].noticeScript;
}

void CoopTrainerRewards_LoadGymNoticeItem(struct ScriptContext *ctx)
{
    u8 gym = sRewards.gym_notice & GYM_NOTICE_GYM_MASK;

    gSpecialVar_0x8000 = sHoennGyms[gym].tm;
    gSpecialVar_0x8001 = 1;
    gSpecialVar_0x8007 = (sRewards.gym_notice & GYM_NOTICE_TM_GIVEN) != 0;
    sRewards.gym_notice = 0;
}

/* Everything the leader's post-battle script gives the player, applied to
 * the partner's save at once so an interrupted notice cannot lose any of it.
 * Only presentation is left to the notice script. */
static void GrantGymToPartner(u8 gym)
{
    const struct CoopHoennGym *entry = &sHoennGyms[gym];
    bool8 tmGiven = FALSE;

    RunScriptImmediately(entry->grantScript);
    if (AddBagItem(entry->tm, 1))
    {
        FlagSet(entry->tmFlag);
        tmGiven = TRUE;
    }
    sRewards.gym_notice = GYM_NOTICE_PENDING | (tmGiven ? GYM_NOTICE_TM_GIVEN : 0) | gym;
}

static void PayPrize(u16 trainerId, u8 moneyMultiplier)
{
    AddMoney(&gSaveBlock1Ptr->money, CoopTrainerRewards_GetPrizeMoney(trainerId, moneyMultiplier));
}

/* A9: the Hoenn story battles. The inventory and the reason for each kind are
 * in docs/plans/2026-09-29-coop-trainers-and-outcome-ledger-plan.md; the
 * grant scripts are data/scripts/coop_story_rewards.inc.
 *
 * GRANT: the partner at the same story point gets the post-battle script's
 * flags, vars and item. The requester's script checked the story var (and the
 * trainer's object was on its map) before the battle, and its post-battle
 * script sets the story flag, so the partner qualifies with the flag clear,
 * the var at storyValue and presenceFlag clear.
 * HELPER: the post-battle script drives a scene (a legendary awakening, the
 * Hall of Fame, a gift Pokémon, a trainer used by two different battles...)
 * the partner's map cannot rebuild; the partner earns EXP only.
 * ORDINARY: the script only shows text; the trainer flag decides.
 *
 * The server mirrors this table (coop-server trainer_rules.rs parses the
 * lines below; keep one entry per line). */
struct CoopStoryBattleInfo
{
    u8 kind;
    u16 storyFlag;    // set by the battle's script; the partner needs it clear (0: none)
    u16 storyVar;     // 0: none; else the partner needs VarGet == storyValue
    u16 storyValue;
    u16 presenceFlag; // the trainer's object flag; the partner needs it clear (0: none)
    u16 item;         // given like `giveitem` (ITEM_NONE: none)
    u16 redrawMap;    // the Elite Four room whose door the notice opens, or MAP_UNDEFINED
    const u8 *grantScript;
};

struct CoopStoryTrainer
{
    u16 trainer;
    u8 battle;
};

extern const u8 CoopStory_EventScript_GrantRivalRoute103[];
extern const u8 CoopStory_EventScript_GrantRivalRoute110[];
extern const u8 CoopStory_EventScript_GrantRivalRoute119[];
extern const u8 CoopStory_EventScript_GrantRivalLilycove[];
extern const u8 CoopStory_EventScript_GrantWallyMauville[];
extern const u8 CoopStory_EventScript_GrantWallyVictoryRoad[];
extern const u8 CoopStory_EventScript_GrantGruntJaggedPass[];
extern const u8 CoopStory_EventScript_GrantMattAquaHideout[];
extern const u8 CoopStory_EventScript_GrantMaxieMtChimney[];
extern const u8 CoopStory_EventScript_GrantSidney[];
extern const u8 CoopStory_EventScript_GrantPhoebe[];
extern const u8 CoopStory_EventScript_GrantGlacia[];
extern const u8 CoopStory_EventScript_GrantDrake[];
extern const u8 CoopStory_EventScript_GrantStevenMeteorFalls[];
extern const u8 CoopStory_EventScript_Notice[];

#define GRANT COOP_STORY_KIND_GRANT
#define HELPER COOP_STORY_KIND_HELPER
#define ORDINARY COOP_STORY_KIND_ORDINARY

static const struct CoopStoryBattleInfo sCoopStoryBattles[COOP_STORY_COUNT] =
{
    [COOP_STORY_RIVAL_ROUTE103] = {GRANT, FLAG_DEFEATED_RIVAL_ROUTE103, 0, 0, FLAG_HIDE_ROUTE_103_RIVAL, ITEM_NONE, MAP_UNDEFINED, CoopStory_EventScript_GrantRivalRoute103},
    [COOP_STORY_RIVAL_ROUTE110] = {GRANT, 0, VAR_ROUTE110_STATE, 0, FLAG_HIDE_ROUTE_110_RIVAL, ITEM_DOWSING_MACHINE, MAP_UNDEFINED, CoopStory_EventScript_GrantRivalRoute110},
    [COOP_STORY_RIVAL_ROUTE119] = {GRANT, FLAG_RECEIVED_HM_FLY, VAR_ROUTE119_STATE, 0, 0, ITEM_HM_FLY, MAP_UNDEFINED, CoopStory_EventScript_GrantRivalRoute119},
    [COOP_STORY_RIVAL_LILYCOVE] = {GRANT, FLAG_MET_RIVAL_LILYCOVE, 0, 0, FLAG_HIDE_LILYCOVE_CITY_RIVAL, ITEM_NONE, MAP_UNDEFINED, CoopStory_EventScript_GrantRivalLilycove},
    [COOP_STORY_WALLY_MAUVILLE] = {GRANT, FLAG_DEFEATED_WALLY_MAUVILLE, 0, 0, FLAG_HIDE_MAUVILLE_CITY_WALLY, ITEM_NONE, MAP_UNDEFINED, CoopStory_EventScript_GrantWallyMauville},
    [COOP_STORY_WALLY_VICTORY_ROAD] = {GRANT, FLAG_DEFEATED_WALLY_VICTORY_ROAD, VAR_VICTORY_ROAD_1F_STATE, 0, 0, ITEM_NONE, MAP_UNDEFINED, CoopStory_EventScript_GrantWallyVictoryRoad},
    [COOP_STORY_GRUNT_JAGGED_PASS] = {GRANT, FLAG_BEAT_MAGMA_GRUNT_JAGGED_PASS, 0, 0, FLAG_HIDE_JAGGED_PASS_MAGMA_GUARD, ITEM_NONE, MAP_UNDEFINED, CoopStory_EventScript_GrantGruntJaggedPass},
    [COOP_STORY_MATT_AQUA_HIDEOUT] = {GRANT, FLAG_TEAM_AQUA_ESCAPED_IN_SUBMARINE, 0, 0, FLAG_HIDE_AQUA_HIDEOUT_GRUNTS, ITEM_NONE, MAP_UNDEFINED, CoopStory_EventScript_GrantMattAquaHideout},
    [COOP_STORY_MAXIE_MT_CHIMNEY] = {GRANT, FLAG_DEFEATED_EVIL_TEAM_MT_CHIMNEY, 0, 0, FLAG_HIDE_MT_CHIMNEY_TEAM_MAGMA, ITEM_NONE, MAP_UNDEFINED, CoopStory_EventScript_GrantMaxieMtChimney},
    [COOP_STORY_SIDNEY] = {GRANT, FLAG_DEFEATED_ELITE_4_SIDNEY, VAR_ELITE_4_STATE, 1, 0, ITEM_NONE, MAP_EVER_GRANDE_CITY_SIDNEYS_ROOM, CoopStory_EventScript_GrantSidney},
    [COOP_STORY_PHOEBE] = {GRANT, FLAG_DEFEATED_ELITE_4_PHOEBE, VAR_ELITE_4_STATE, 2, 0, ITEM_NONE, MAP_EVER_GRANDE_CITY_PHOEBES_ROOM, CoopStory_EventScript_GrantPhoebe},
    [COOP_STORY_GLACIA] = {GRANT, FLAG_DEFEATED_ELITE_4_GLACIA, VAR_ELITE_4_STATE, 3, 0, ITEM_NONE, MAP_EVER_GRANDE_CITY_GLACIAS_ROOM, CoopStory_EventScript_GrantGlacia},
    [COOP_STORY_DRAKE] = {GRANT, FLAG_DEFEATED_ELITE_4_DRAKE, VAR_ELITE_4_STATE, 4, 0, ITEM_NONE, MAP_EVER_GRANDE_CITY_DRAKES_ROOM, CoopStory_EventScript_GrantDrake},
    [COOP_STORY_STEVEN_METEOR_FALLS] = {GRANT, FLAG_DEFEATED_METEOR_FALLS_STEVEN, 0, 0, 0, ITEM_NONE, MAP_UNDEFINED, CoopStory_EventScript_GrantStevenMeteorFalls},
    [COOP_STORY_RIVAL_RUSTBORO] = {HELPER, 0, 0, 0, 0, ITEM_NONE, MAP_UNDEFINED, NULL},
    [COOP_STORY_GRUNT_PETALBURG_WOODS] = {HELPER, 0, 0, 0, 0, ITEM_NONE, MAP_UNDEFINED, NULL},
    [COOP_STORY_GRUNT_RUSTURF_TUNNEL] = {HELPER, 0, 0, 0, 0, ITEM_NONE, MAP_UNDEFINED, NULL},
    [COOP_STORY_GRUNTS_OCEANIC_MUSEUM] = {HELPER, 0, 0, 0, 0, ITEM_NONE, MAP_UNDEFINED, NULL},
    [COOP_STORY_SHELLY_WEATHER_INSTITUTE] = {HELPER, 0, 0, 0, 0, ITEM_NONE, MAP_UNDEFINED, NULL},
    [COOP_STORY_GRUNTS_SPACE_CENTER] = {HELPER, 0, 0, 0, 0, ITEM_NONE, MAP_UNDEFINED, NULL},
    [COOP_STORY_MAXIE_MAGMA_HIDEOUT] = {HELPER, 0, 0, 0, 0, ITEM_NONE, MAP_UNDEFINED, NULL},
    [COOP_STORY_ARCHIE_SEAFLOOR_CAVERN] = {HELPER, 0, 0, 0, 0, ITEM_NONE, MAP_UNDEFINED, NULL},
    [COOP_STORY_CHAMPION_WALLACE] = {HELPER, 0, 0, 0, 0, ITEM_NONE, MAP_UNDEFINED, NULL},
    [COOP_STORY_AQUA_HIDEOUT_GRUNTS] = {ORDINARY, 0, 0, 0, 0, ITEM_NONE, MAP_UNDEFINED, NULL},
    [COOP_STORY_ADMINS] = {ORDINARY, 0, 0, 0, 0, ITEM_NONE, MAP_UNDEFINED, NULL},
    [COOP_STORY_WALLY_VICTORY_ROAD_EXIT] = {ORDINARY, 0, 0, 0, 0, ITEM_NONE, MAP_UNDEFINED, NULL},
};

static const struct CoopStoryTrainer sCoopStoryTrainers[] =
{
    {TRAINER_MAY_ROUTE_103_TREECKO, COOP_STORY_RIVAL_ROUTE103},
    {TRAINER_MAY_ROUTE_103_TORCHIC, COOP_STORY_RIVAL_ROUTE103},
    {TRAINER_MAY_ROUTE_103_MUDKIP, COOP_STORY_RIVAL_ROUTE103},
    {TRAINER_BRENDAN_ROUTE_103_TREECKO, COOP_STORY_RIVAL_ROUTE103},
    {TRAINER_BRENDAN_ROUTE_103_TORCHIC, COOP_STORY_RIVAL_ROUTE103},
    {TRAINER_BRENDAN_ROUTE_103_MUDKIP, COOP_STORY_RIVAL_ROUTE103},
    {TRAINER_MAY_ROUTE_110_TREECKO, COOP_STORY_RIVAL_ROUTE110},
    {TRAINER_MAY_ROUTE_110_TORCHIC, COOP_STORY_RIVAL_ROUTE110},
    {TRAINER_MAY_ROUTE_110_MUDKIP, COOP_STORY_RIVAL_ROUTE110},
    {TRAINER_BRENDAN_ROUTE_110_TREECKO, COOP_STORY_RIVAL_ROUTE110},
    {TRAINER_BRENDAN_ROUTE_110_TORCHIC, COOP_STORY_RIVAL_ROUTE110},
    {TRAINER_BRENDAN_ROUTE_110_MUDKIP, COOP_STORY_RIVAL_ROUTE110},
    {TRAINER_MAY_ROUTE_119_TREECKO, COOP_STORY_RIVAL_ROUTE119},
    {TRAINER_MAY_ROUTE_119_TORCHIC, COOP_STORY_RIVAL_ROUTE119},
    {TRAINER_MAY_ROUTE_119_MUDKIP, COOP_STORY_RIVAL_ROUTE119},
    {TRAINER_BRENDAN_ROUTE_119_TREECKO, COOP_STORY_RIVAL_ROUTE119},
    {TRAINER_BRENDAN_ROUTE_119_TORCHIC, COOP_STORY_RIVAL_ROUTE119},
    {TRAINER_BRENDAN_ROUTE_119_MUDKIP, COOP_STORY_RIVAL_ROUTE119},
    {TRAINER_MAY_LILYCOVE_TREECKO, COOP_STORY_RIVAL_LILYCOVE},
    {TRAINER_MAY_LILYCOVE_TORCHIC, COOP_STORY_RIVAL_LILYCOVE},
    {TRAINER_MAY_LILYCOVE_MUDKIP, COOP_STORY_RIVAL_LILYCOVE},
    {TRAINER_BRENDAN_LILYCOVE_TREECKO, COOP_STORY_RIVAL_LILYCOVE},
    {TRAINER_BRENDAN_LILYCOVE_TORCHIC, COOP_STORY_RIVAL_LILYCOVE},
    {TRAINER_BRENDAN_LILYCOVE_MUDKIP, COOP_STORY_RIVAL_LILYCOVE},
    {TRAINER_WALLY_MAUVILLE, COOP_STORY_WALLY_MAUVILLE},
    {TRAINER_WALLY_VR_1, COOP_STORY_WALLY_VICTORY_ROAD},
    {TRAINER_GRUNT_JAGGED_PASS, COOP_STORY_GRUNT_JAGGED_PASS},
    {TRAINER_MATT, COOP_STORY_MATT_AQUA_HIDEOUT},
    {TRAINER_MAXIE_MT_CHIMNEY, COOP_STORY_MAXIE_MT_CHIMNEY},
    {TRAINER_SIDNEY, COOP_STORY_SIDNEY},
    {TRAINER_PHOEBE, COOP_STORY_PHOEBE},
    {TRAINER_GLACIA, COOP_STORY_GLACIA},
    {TRAINER_DRAKE, COOP_STORY_DRAKE},
    {TRAINER_STEVEN, COOP_STORY_STEVEN_METEOR_FALLS},
    {TRAINER_MAY_RUSTBORO_TREECKO, COOP_STORY_RIVAL_RUSTBORO},
    {TRAINER_MAY_RUSTBORO_TORCHIC, COOP_STORY_RIVAL_RUSTBORO},
    {TRAINER_MAY_RUSTBORO_MUDKIP, COOP_STORY_RIVAL_RUSTBORO},
    {TRAINER_BRENDAN_RUSTBORO_TREECKO, COOP_STORY_RIVAL_RUSTBORO},
    {TRAINER_BRENDAN_RUSTBORO_TORCHIC, COOP_STORY_RIVAL_RUSTBORO},
    {TRAINER_BRENDAN_RUSTBORO_MUDKIP, COOP_STORY_RIVAL_RUSTBORO},
    {TRAINER_GRUNT_PETALBURG_WOODS, COOP_STORY_GRUNT_PETALBURG_WOODS},
    {TRAINER_GRUNT_RUSTURF_TUNNEL, COOP_STORY_GRUNT_RUSTURF_TUNNEL},
    {TRAINER_GRUNT_MUSEUM_1, COOP_STORY_GRUNTS_OCEANIC_MUSEUM},
    {TRAINER_GRUNT_MUSEUM_2, COOP_STORY_GRUNTS_OCEANIC_MUSEUM},
    {TRAINER_SHELLY_WEATHER_INSTITUTE, COOP_STORY_SHELLY_WEATHER_INSTITUTE},
    {TRAINER_GRUNT_SPACE_CENTER_2, COOP_STORY_GRUNTS_SPACE_CENTER},
    {TRAINER_GRUNT_SPACE_CENTER_5, COOP_STORY_GRUNTS_SPACE_CENTER},
    {TRAINER_GRUNT_SPACE_CENTER_6, COOP_STORY_GRUNTS_SPACE_CENTER},
    {TRAINER_GRUNT_SPACE_CENTER_7, COOP_STORY_GRUNTS_SPACE_CENTER},
    {TRAINER_MAXIE_MAGMA_HIDEOUT, COOP_STORY_MAXIE_MAGMA_HIDEOUT},
    {TRAINER_ARCHIE, COOP_STORY_ARCHIE_SEAFLOOR_CAVERN},
    {TRAINER_WALLACE, COOP_STORY_CHAMPION_WALLACE},
    {TRAINER_GRUNT_AQUA_HIDEOUT_1, COOP_STORY_AQUA_HIDEOUT_GRUNTS},
    {TRAINER_GRUNT_AQUA_HIDEOUT_2, COOP_STORY_AQUA_HIDEOUT_GRUNTS},
    {TRAINER_GRUNT_AQUA_HIDEOUT_3, COOP_STORY_AQUA_HIDEOUT_GRUNTS},
    {TRAINER_GRUNT_AQUA_HIDEOUT_4, COOP_STORY_AQUA_HIDEOUT_GRUNTS},
    {TRAINER_SHELLY_SEAFLOOR_CAVERN, COOP_STORY_ADMINS},
    {TRAINER_TABITHA_MT_CHIMNEY, COOP_STORY_ADMINS},
    {TRAINER_TABITHA_MAGMA_HIDEOUT, COOP_STORY_ADMINS},
    {TRAINER_WALLY_VR_2, COOP_STORY_WALLY_VICTORY_ROAD_EXIT},
};

#undef GRANT
#undef HELPER
#undef ORDINARY

u8 CoopTrainerRewards_GetStoryBattle(u16 trainerId)
{
    u32 i;

    if (trainerId == TRAINER_NONE)
        return COOP_STORY_NONE;
    for (i = 0; i < ARRAY_COUNT(sCoopStoryTrainers); i++)
        if (sCoopStoryTrainers[i].trainer == trainerId)
            return sCoopStoryTrainers[i].battle;
    return COOP_STORY_NONE;
}

enum CoopStoryKind CoopTrainerRewards_GetStoryKind(u8 battle)
{
    return battle < COOP_STORY_COUNT ? sCoopStoryBattles[battle].kind : COOP_STORY_KIND_ORDINARY;
}

bool8 CoopTrainerRewards_IsFullTeamClass(u8 trainerClass)
{
    switch (trainerClass)
    {
    case TRAINER_CLASS_LEADER:
    case TRAINER_CLASS_ELITE_FOUR:
    case TRAINER_CLASS_CHAMPION:
    case TRAINER_CLASS_AQUA_ADMIN:
    case TRAINER_CLASS_AQUA_LEADER:
    case TRAINER_CLASS_MAGMA_ADMIN:
    case TRAINER_CLASS_MAGMA_LEADER:
        return TRUE;
    default:
        return FALSE;
    }
}

bool8 CoopTrainerRewards_IsStoryPartnerEligible(u8 battle)
{
    const struct CoopStoryBattleInfo *entry;

    if (battle >= COOP_STORY_COUNT)
        return FALSE;
    entry = &sCoopStoryBattles[battle];
    if (entry->kind != COOP_STORY_KIND_GRANT || entry->grantScript == NULL)
        return FALSE;
    if (entry->storyFlag != 0 && FlagGet(entry->storyFlag))
        return FALSE;
    if (entry->storyVar != 0 && VarGet(entry->storyVar) != entry->storyValue)
        return FALSE;
    if (entry->presenceFlag != 0 && FlagGet(entry->presenceFlag))
        return FALSE;
    return TRUE;
}

const u8 *CoopTrainerRewards_GetStoryNoticeScript(void)
{
    return CoopStory_EventScript_Notice;
}

/* The object's hide flag from its map template (0 for objects without one,
 * such as a follower or an object the map does not list). */
static u16 GetStoryObjectFlag(const struct ObjectEvent *object)
{
    const struct ObjectEventTemplate *template;

    if (!object->active || object->isPlayer)
        return 0;
    template = GetObjectEventTemplateByLocalIdAndMap(object->localId, object->mapNum,
                                                     object->mapGroup);
    return template != NULL ? template->flagId : 0;
}

/* Objects on the partner's screen whose hide flag is clear. */
static u16 GetShownFlaggedObjects(void)
{
    u16 mask = 0;
    u32 i;

    for (i = 0; i < OBJECT_EVENTS_COUNT; i++)
    {
        u16 flag = GetStoryObjectFlag(&gObjectEvents[i]);

        if (flag != 0 && !FlagGet(flag))
            mask |= 1u << i;
    }
    return mask;
}

/* The post-battle script's state, applied to the partner's save at once so an
 * interrupted notice cannot lose it. Objects it hides that are on the
 * partner's screen (a rival standing on the same route) are removed by the
 * notice, exactly as the map would not spawn them on its next load. */
static void GrantStoryToPartner(u8 battle)
{
    const struct CoopStoryBattleInfo *entry = &sCoopStoryBattles[battle];
    u16 shown = GetShownFlaggedObjects();
    u16 hidden = 0;
    bool8 itemGiven = FALSE;
    u32 i;

    RunScriptImmediately(entry->grantScript);
    /* `giveitem` in the vanilla script: a full bag only shows a message. */
    if (entry->item != ITEM_NONE)
        itemGiven = AddBagItem(entry->item, 1);
    for (i = 0; i < OBJECT_EVENTS_COUNT; i++)
    {
        u16 flag = GetStoryObjectFlag(&gObjectEvents[i]);

        if ((shown & (1u << i)) && flag != 0 && FlagGet(flag))
            hidden |= 1u << i;
    }
    sRewards.story_hide_mask = hidden;
    sRewards.story_notice = STORY_NOTICE_PENDING | (itemGiven ? STORY_NOTICE_ITEM_GIVEN : 0) | battle;
}

void CoopTrainerRewards_SyncStoryObjects(struct ScriptContext *ctx)
{
    u32 i;

    for (i = 0; i < OBJECT_EVENTS_COUNT; i++)
    {
        struct ObjectEvent *object = &gObjectEvents[i];
        u16 flag;

        if (!(sRewards.story_hide_mask & (1u << i)))
            continue;
        flag = GetStoryObjectFlag(object);
        if (flag != 0 && FlagGet(flag))
            RemoveObjectEvent(object);
    }
    sRewards.story_hide_mask = 0;
}

void CoopTrainerRewards_LoadStoryNotice(struct ScriptContext *ctx)
{
    u8 notice = sRewards.story_notice;
    const struct CoopStoryBattleInfo *entry;

    gSpecialVar_0x8004 = FALSE;
    gSpecialVar_0x8005 = FALSE;
    gSpecialVar_0x8000 = ITEM_NONE;
    gSpecialVar_0x8001 = 0;
    gSpecialVar_0x8007 = FALSE;
    sRewards.story_notice = 0;
    if (!(notice & STORY_NOTICE_PENDING)
     || (notice & STORY_NOTICE_BATTLE_MASK) >= COOP_STORY_COUNT)
        return;
    entry = &sCoopStoryBattles[notice & STORY_NOTICE_BATTLE_MASK];
    gSpecialVar_0x8004 = entry->redrawMap != MAP_UNDEFINED
        && gSaveBlock1Ptr->location.mapGroup == MAP_GROUP(entry->redrawMap)
        && gSaveBlock1Ptr->location.mapNum == MAP_NUM(entry->redrawMap);
    if (entry->item != ITEM_NONE)
    {
        gSpecialVar_0x8005 = TRUE;
        gSpecialVar_0x8000 = entry->item;
        gSpecialVar_0x8001 = 1;
        gSpecialVar_0x8007 = (notice & STORY_NOTICE_ITEM_GIVEN) != 0;
    }
}

static u32 CountBits(u32 bits)
{
    u32 count = 0;

    for (; bits != 0; bits &= bits - 1)
        count++;
    return count;
}

/* The local member always fights from the left player position, with its
 * staged mons in B_TRAINER_0 (member 1's ROM translates targets instead). */
static u32 GetLocalSentBit(bool32 requirePresent)
{
    enum BattlerId local = GetBattlerAtPosition(B_POSITION_PLAYER_LEFT);

    if (local >= gBattlersCount || gBattlerPartyIndexes[local] >= COOP_BATTLE_MULTI_PARTY_SIZE
     || (requirePresent && (gAbsentBattlerFlags & (1u << local))))
        return 0;
    return 1u << gBattlerPartyIndexes[local];
}

void CoopTrainerRewards_Begin(void)
{
    memset(&sRewards, 0, sizeof(sRewards));
    sRewards.money_multiplier = 1;
    sRewards.armed = TRUE;
}

void CoopTrainerRewards_OnSentPokesReset(void)
{
    if (!sRewards.armed)
        return;
    sRewards.sent[0] = sRewards.sent[1] = GetLocalSentBit(FALSE);
}

void CoopTrainerRewards_OnOpponentSwitchIn(u8 battler)
{
    if (!sRewards.armed || battler >= MAX_BATTLERS_COUNT || IsOnPlayerSide(battler))
        return;
    sRewards.sent[(battler & BIT_FLANK) >> 1] = GetLocalSentBit(TRUE);
}

void CoopTrainerRewards_OnPlayerSwitchIn(u8 battler)
{
    if (!sRewards.armed || battler != GetBattlerAtPosition(B_POSITION_PLAYER_LEFT))
        return;
    sRewards.sent[0] |= GetLocalSentBit(FALSE);
    sRewards.sent[1] |= GetLocalSentBit(FALSE);
}

/* Reads the battle, never writes it: the vanilla Cmd_getexp decisions that
 * depend on the moment of the faint (who is alive, who fought it, who holds
 * an EXP Share) are frozen here and replayed after the battle. */
void CoopTrainerRewards_RecordFaint(u8 battler)
{
    u32 slot;
    u32 i;
    u32 valid = 0;
    u32 share = 0;

    if (!sRewards.armed || battler >= gBattlersCount || IsOnPlayerSide(battler))
        return;
    slot = gBattlerPartyIndexes[battler];
    if (slot >= PARTY_SIZE || (sRewards.faints[slot] & COOP_TRAINER_REWARD_FAINTED))
        return;
    for (i = 0; i < COOP_BATTLE_MULTI_PARTY_SIZE; i++)
    {
        struct Pokemon *mon = &gParties[B_TRAINER_0][i];

        if (!IsValidForBattle(mon))
            continue;
        valid |= 1u << i;
        if (GetItemHoldEffect(GetMonData(mon, MON_DATA_HELD_ITEM)) == HOLD_EFFECT_EXP_SHARE
         || IsGen6ExpShareEnabled())
            share |= 1u << i;
    }
    sRewards.faints[slot] = COOP_TRAINER_REWARD_FAINTED
                          | (sRewards.sent[(battler & BIT_FLANK) >> 1] & valid)
                          | (share << COOP_TRAINER_REWARD_SHARE_SHIFT);
}

void CoopTrainerRewards_OnBattleWon(u8 moneyMultiplier)
{
    if (sRewards.armed)
        sRewards.money_multiplier = moneyMultiplier != 0 ? moneyMultiplier : 1;
}

u32 CoopTrainerRewards_GetPrizeMoney(u16 trainerId, u8 moneyMultiplier)
{
    return GetTrainerPrizeMoney(trainerId, moneyMultiplier != 0 ? moneyMultiplier : 1,
                                GetTrainerBattleType(trainerId) == TRAINER_BATTLE_TYPE_DOUBLES);
}

static void LearnLevelUpMovesSilently(struct Pokemon *mon)
{
    enum Move move;

    /* GiveMoveToMon fills an empty slot; a full moveset returns
     * MON_HAS_MAX_MOVES and the move is skipped (no replace prompt). */
    for (move = MonTryLearningNewMove(mon, TRUE); move != MOVE_NONE;
         move = MonTryLearningNewMove(mon, FALSE))
        ;
}

/* Level by level, so every level's moves and friendship are applied as the
 * battle controller would have done. */
static void GiveMonExperience(struct Pokemon *mon, u32 amount, u32 partySlot)
{
    enum Species species = GetMonData(mon, MON_DATA_SPECIES);
    const u32 *table = gExperienceTables[gSpeciesInfo[species].growthRate];
    u32 experience = GetMonData(mon, MON_DATA_EXP);
    u32 maximum = GetMaxMonExperience(species);
    u32 level = GetMonData(mon, MON_DATA_LEVEL);
    u32 target;

    if (amount == 0 || experience >= maximum)
        return;
    target = experience + min(amount, maximum - experience);
    while (level < MAX_LEVEL && target >= table[level + 1])
    {
        u32 step = table[level + 1];
        u32 newLevel;

        SetMonData(mon, MON_DATA_EXP, &step);
        CalculateMonStats(mon);
        newLevel = GetMonData(mon, MON_DATA_LEVEL);
        if (newLevel <= level)
            break;
        level = newLevel;
        AdjustFriendship(mon, FRIENDSHIP_EVENT_GROW_LEVEL);
        LearnLevelUpMovesSilently(mon);
        gLeveledUpInBattle |= 1u << partySlot;
    }
    SetMonData(mon, MON_DATA_EXP, &target);
    CalculateMonStats(mon);
}

/* Cmd_getexp for one fainted opponent, from its faint record. */
static void ApplyOpponentExperience(struct Pokemon *opponent, u8 record,
                                    const u8 *stagedSlots, u8 stagedCount)
{
    enum Species fainted = GetMonData(opponent, MON_DATA_SPECIES);
    u32 faintedLevel = GetMonData(opponent, MON_DATA_LEVEL);
    u32 sentBits = record & COOP_TRAINER_REWARD_STAGED_MASK;
    u32 shareBits = (record >> COOP_TRAINER_REWARD_SHARE_SHIFT) & COOP_TRAINER_REWARD_STAGED_MASK;
    u32 viaSentIn = CountBits(sentBits);
    u32 viaExpShare = CountBits(shareBits);
    u32 calculatedExp;
    u32 expValue;
    u32 shareValue;
    u32 i;

    if (fainted == SPECIES_NONE || (sentBits | shareBits) == 0)
        return;
    calculatedExp = gSpeciesInfo[fainted].expYield * faintedLevel;
    if (B_SCALED_EXP >= GEN_5 && B_SCALED_EXP != GEN_6)
        calculatedExp /= 5;
    else
        calculatedExp /= 7;
    if (B_TRAINER_EXP_MULTIPLIER <= GEN_7)
        calculatedExp = (calculatedExp * 150) / 100;

    if (B_SPLIT_EXP < GEN_6)
    {
        if (viaExpShare)
        {
            expValue = SAFE_DIV(calculatedExp / 2, viaSentIn);
            shareValue = calculatedExp / 2 / viaExpShare;
            if (shareValue == 0)
                shareValue = 1;
        }
        else
        {
            expValue = SAFE_DIV(calculatedExp, viaSentIn);
            shareValue = 0;
        }
        if (expValue == 0)
            expValue = 1;
    }
    else
    {
        expValue = calculatedExp;
        shareValue = calculatedExp / 2;
        if (shareValue == 0)
            shareValue = 1;
    }

    for (i = 0; i < stagedCount; i++)
    {
        struct Pokemon *mon;
        bool32 wasSentOut = (sentBits & (1u << i)) != 0;
        bool32 shared = (shareBits & (1u << i)) != 0;
        s32 reward;
        u32 level;

        if ((!wasSentOut && !shared) || stagedSlots[i] >= PARTY_SIZE)
            continue;
        mon = &gParties[B_TRAINER_0][stagedSlots[i]];
        if (GetMonData(mon, MON_DATA_SPECIES_OR_EGG) == SPECIES_NONE
         || GetMonData(mon, MON_DATA_SPECIES_OR_EGG) == SPECIES_EGG)
            continue;
        if (!CanMonGainExperience(mon))
        {
            if (B_MAX_LEVEL_EV_GAINS >= GEN_5)
                MonGainEVs(mon, fainted);
            continue;
        }
        level = GetMonData(mon, MON_DATA_LEVEL);
        reward = wasSentOut ? GetSoftLevelCapExpValue(level, expValue) : 0;
        if (shared && (B_SPLIT_EXP < GEN_6 || reward == 0))
            reward += GetSoftLevelCapExpValue(level, shareValue);
        ApplyMonExperienceMultipliers(&reward, mon, faintedLevel);
        if (B_EXP_CAP_TYPE == EXP_CAP_HARD && reward != 0)
        {
            const u32 *table = gExperienceTables[gSpeciesInfo[GetMonData(mon, MON_DATA_SPECIES)].growthRate];
            u32 current = GetMonData(mon, MON_DATA_EXP);
            u32 cap = GetCurrentLevelCap();

            if (level >= cap)
                reward = 0;
            else if (table[cap] < current + reward)
                reward = table[cap] - current;
        }
        MonGainEVs(mon, fainted);
        if (reward > 0)
            GiveMonExperience(mon, reward, stagedSlots[i]);
    }
}

enum CoopTrainerRewardRole CoopTrainerRewards_Apply(bool8 won, u16 trainerId,
                                                    const u8 *stagedSlots, u8 stagedCount,
                                                    bool8 requester)
{
    struct CoopTrainerRewardState state = sRewards;
    enum CoopTrainerRewardRole role;
    u16 base;
    u8 gym;
    u8 story;
    u32 i;

    /* Disarm first: whatever happens below, this battle is settled. */
    memset(&sRewards, 0, sizeof(sRewards));
    if (!state.armed || !won || trainerId == TRAINER_NONE || trainerId >= TRAINERS_COUNT
     || stagedSlots == NULL || stagedCount == 0
     || stagedCount > COOP_BATTLE_MULTI_PARTY_SIZE
     || !CoopTrainerEncounter_IsSupportedTrainer(trainerId))
        return COOP_TRAINER_REWARD_NONE;

    gym = CoopTrainerRewards_GetHoennGym(trainerId);
    story = CoopTrainerRewards_GetStoryBattle(trainerId);
    if (story != COOP_STORY_NONE
     && CoopTrainerRewards_GetStoryKind(story) == COOP_STORY_KIND_ORDINARY)
        story = COOP_STORY_NONE; // the ordinary trainer-flag rule below
    if (gym != COOP_HOENN_GYM_NONE)
    {
        /* The requester reached the battle through the leader's unbeaten
         * script and without the badge (the eligibility gate), so it is the
         * participant; its resumed script gives the badge, TM and flags. */
        if (requester || CoopTrainerRewards_IsGymPartnerEligible(gym))
        {
            role = requester ? COOP_TRAINER_REWARD_PARTICIPANT
                             : COOP_TRAINER_REWARD_GYM_PARTNER;
            PayPrize(trainerId, state.money_multiplier);
            /* CB2_EndTrainerBattle's win branch for the leader. */
            BattleSetup_RegisterTrainerInMatchCall(trainerId);
            SetTrainerFlag(trainerId);
            if (!requester)
                GrantGymToPartner(gym);
        }
        else
        {
            role = COOP_TRAINER_REWARD_HELPER;
        }
    }
    else if (story != COOP_STORY_NONE)
    {
        /* The requester's own story script led to the battle and sets the
         * story state when it resumes; vanilla pays every such win (an Elite
         * Four member is fought again on each run). A partner at the same
         * story point of a GRANT battle gets that state now; anyone else
         * helps. */
        if (requester || CoopTrainerRewards_IsStoryPartnerEligible(story))
        {
            role = requester ? COOP_TRAINER_REWARD_PARTICIPANT
                             : COOP_TRAINER_REWARD_STORY_PARTNER;
            PayPrize(trainerId, state.money_multiplier);
            /* CB2_EndTrainerBattle's win branch for this trainer. */
            BattleSetup_RegisterTrainerInMatchCall(trainerId);
            SetTrainerFlag(trainerId);
            if (!requester)
                GrantStoryToPartner(story);
        }
        else
        {
            role = COOP_TRAINER_REWARD_HELPER;
        }
    }
    else if (BattleSetup_GetRematchBaseTrainer(trainerId, &base))
    {
        /* A rematch pays whoever has beaten the first battle, as vanilla
         * pays every rematch win. Only the requester fought it through its
         * own match-call state, so only its ROM records the rematch. */
        if (HasTrainerBeenFought(base))
        {
            role = COOP_TRAINER_REWARD_PARTICIPANT;
            PayPrize(trainerId, state.money_multiplier);
            if (requester)
                BattleSetup_ApplyCoopRematchWin(trainerId);
        }
        else
        {
            role = COOP_TRAINER_REWARD_HELPER;
        }
    }
    else if (HasTrainerBeenFought(trainerId))
    {
        role = COOP_TRAINER_REWARD_HELPER;
    }
    else
    {
        role = COOP_TRAINER_REWARD_PARTICIPANT;
        PayPrize(trainerId, state.money_multiplier);
        SetTrainerFlag(trainerId);
    }

    gLeveledUpInBattle = 0;
    gTriedEvolving = 0;
    for (i = 0; i < PARTY_SIZE; i++)
        if (state.faints[i] & COOP_TRAINER_REWARD_FAINTED)
            ApplyOpponentExperience(&gParties[B_TRAINER_1][i], state.faints[i],
                                    stagedSlots, stagedCount);
    return role;
}

bool8 CoopTrainerRewards_HasPendingEvolutions(void)
{
    return gLeveledUpInBattle != 0;
}

/* TryEvolvePokemon's level-up pass, outside the battle. */
void CB2_CoopTrainerRewardEvolutions(void)
{
    u32 i;

    for (i = 0; i < PARTY_SIZE; i++)
    {
        struct Pokemon *mon = &gParties[B_TRAINER_0][i];
        bool32 canStopEvo = TRUE;
        enum Species species;

        if (!(gLeveledUpInBattle & (1u << i)))
            continue;
        gLeveledUpInBattle &= ~(1u << i);
        species = GetEvolutionTargetSpecies(mon, EVO_MODE_BATTLE_ONLY, gLeveledUpInBattle,
                                            NULL, &canStopEvo, CHECK_EVO);
        if (species == SPECIES_NONE)
            continue;
        GetEvolutionTargetSpecies(mon, EVO_MODE_BATTLE_ONLY, gLeveledUpInBattle,
                                  NULL, &canStopEvo, DO_EVO);
        gCB2_AfterEvolution = CB2_CoopTrainerRewardEvolutions;
        FreeAllWindowBuffers();
        EvolutionScene(mon, species, canStopEvo, i);
        return;
    }
    gLeveledUpInBattle = 0;
    gTriedEvolving = 0;
    SetMainCallback2(CB2_ReturnToFieldContinueScriptPlayMapMusic);
}

#if TESTING
bool8 CoopTrainerRewards_TestIsArmed(void)
{
    return sRewards.armed;
}

u8 CoopTrainerRewards_TestGetFaintRecord(u8 opponentSlot)
{
    return opponentSlot < PARTY_SIZE ? sRewards.faints[opponentSlot] : 0;
}

u16 CoopTrainerRewards_TestGetStoryHideMask(void)
{
    return sRewards.story_hide_mask;
}
#endif
