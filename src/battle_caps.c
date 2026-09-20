#include "global.h"
#include "battle_caps.h"
#include "battle.h"
#include "battle_setup.h"
#include "battle_dynamax.h"
#include "battle_gimmick.h"
#include "battle_util.h"
#include "config_changes.h"
#include "constants/johto_content.h"
#include "event_data.h"
#include "region_map.h"
#include "regions.h"
#include "test_runner.h"

struct CappedMon
{
    u32 experience;
    enum Species species;
    enum Species battleSpecies;
    u16 hp;
    u16 maxHp;
    u16 projectedHp;
    u16 projectedMaxHp;
    u16 resumeHp;
    u16 resumeMaxHp;
    bool8 projected;
    bool8 resumeForm;
};

static EWRAM_DATA struct CappedMon sCappedMons[PARTY_SIZE] = {0};
static EWRAM_DATA u8 sBattleLevelCap = 0;

u32 GetBadgeBattleLevelCap(void)
{
    static const u8 caps[] = {15, 19, 24, 29, 31, 33, 42, 46, 58};
    static const u16 hoennLeaders[] = {
        TRAINER_ROXANNE_1, TRAINER_BRAWLY_1, TRAINER_WATTSON_1, TRAINER_FLANNERY_1,
        TRAINER_NORMAN_1, TRAINER_WINONA_1, TRAINER_TATE_AND_LIZA_1, TRAINER_JUAN_1,
    };
    static const u16 kantoLeaders[] = {
        FLAG_DEFEATED_BROCK, FLAG_DEFEATED_MISTY, FLAG_DEFEATED_LT_SURGE, FLAG_DEFEATED_ERIKA,
        FLAG_DEFEATED_KOGA, FLAG_DEFEATED_SABRINA, FLAG_DEFEATED_BLAINE, FLAG_DEFEATED_LEADER_GIOVANNI,
    };
    static const u16 johtoBadges[] = {
        JOHTO_FLAG_BADGE01_GET, JOHTO_FLAG_BADGE02_GET, JOHTO_FLAG_BADGE03_GET, JOHTO_FLAG_BADGE04_GET,
        JOHTO_FLAG_BADGE05_GET, JOHTO_FLAG_BADGE06_GET, JOHTO_FLAG_BADGE07_GET, JOHTO_FLAG_BADGE08_GET,
    };
    u32 badges = 0;
    enum Region region = GetCurrentRegion();
    bool32 laterKanto = region == REGION_KANTO
                     && GetKantoEraByMap(gSaveBlock1Ptr->location.mapGroup,
                                         gSaveBlock1Ptr->location.mapNum,
                                         gMapHeader.regionMapSectionId) == KANTO_ERA_LATER;
    if (laterKanto)
        return MAX_LEVEL;

    for (u32 i = 0; i < NUM_BADGES; i++)
    {
        if (region == REGION_KANTO)
            badges += FlagGet(kantoLeaders[i]);
        else if (region == REGION_JOHTO)
            badges += FlagGet(johtoBadges[i]);
        else
            badges += HasTrainerBeenFought(hoennLeaders[i]);
    }
    bool32 champion;
    if (region == REGION_KANTO)
        champion = FlagGet(FLAG_KANTO_MASTERY_CHAMPION) || VarGet(VAR_MAP_SCENE_PALLET_TOWN_OAK) >= 2;
    else if (region == REGION_JOHTO)
        champion = FlagGet(JOHTO_FLAG_IS_CHAMPION);
    else
        champion = FlagGet(FLAG_IS_CHAMPION);
    if (badges == NUM_BADGES && champion)
        return MAX_LEVEL;
    return caps[badges];
}

enum Species GetSpeciesAtBattleLevelCap(enum Species species, u32 cap)
{
    enum Species result = species;
    // Also check ancestors of stone/trade evolutions: a later non-level edge
    // must not bypass an earlier level requirement. Never invent a level for
    // an evolution whose data has none.
    for (u32 depth = 0; depth < NUM_SPECIES; depth++)
    {
        enum Species parent = GetSpeciesPreEvolution(species);
        bool32 allowed = FALSE;
        if (parent == SPECIES_NONE || parent == species)
            break;
        const struct Evolution *evolutions = GetSpeciesEvolutions(parent);
        for (u32 i = 0; evolutions[i].method != EVOLUTIONS_END; i++)
        {
            if (evolutions[i].targetSpecies != species)
                continue;
            if ((evolutions[i].method != EVO_LEVEL && evolutions[i].method != EVO_LEVEL_BATTLE_ONLY)
             || evolutions[i].param <= cap)
                allowed = TRUE;
        }
        if (!allowed)
            result = parent;
        species = parent;
    }
    return result;
}

static u32 ScaleHp(u32 hp, u32 oldMax, u32 newMax)
{
    if (hp == 0)
        return 0;
    return min(newMax, max(1, hp * newMax / max(1, oldMax)));
}

static u32 HpAfterStatChange(u32 hp, u32 oldMax, u32 newMax)
{
    return hp == 0 ? 0 : min(newMax, hp + (newMax > oldMax ? newMax - oldMax : 0));
}

static void ProjectMon(u32 partyIndex)
{
    struct Pokemon *mon = &gParties[B_TRAINER_0][partyIndex];
    struct CappedMon *saved = &sCappedMons[partyIndex];
    u32 oldMaxHp;
    enum Species species;
    u32 exp;

    if (sBattleLevelCap == 0 || saved->projected || GetMonData(mon, MON_DATA_SPECIES) == SPECIES_NONE
     || GetMonData(mon, MON_DATA_IS_EGG) || GetMonData(mon, MON_DATA_LEVEL) <= sBattleLevelCap
     || ((gBattleTypeFlags & BATTLE_TYPE_INGAME_PARTNER) && partyIndex >= 3))
        return;
    saved->species = GetMonData(mon, MON_DATA_SPECIES);
    saved->experience = GetMonData(mon, MON_DATA_EXP);
    saved->hp = GetMonData(mon, MON_DATA_HP);
    oldMaxHp = GetMonData(mon, MON_DATA_MAX_HP);
    saved->maxHp = oldMaxHp;
    species = saved->resumeForm ? saved->battleSpecies : GetSpeciesAtBattleLevelCap(saved->species, sBattleLevelCap);
    saved->resumeForm = FALSE;
    exp = gExperienceTables[gSpeciesInfo[species].growthRate][sBattleLevelCap];
    SetMonData(mon, MON_DATA_SPECIES, &species);
    SetMonData(mon, MON_DATA_EXP, &exp);
    CalculateMonStats(mon);
    saved->projectedMaxHp = GetMonData(mon, MON_DATA_MAX_HP);
    saved->projectedHp = ScaleHp(saved->hp, oldMaxHp, saved->projectedMaxHp);
    SetMonData(mon, MON_DATA_HP, &saved->projectedHp);
    saved->projected = TRUE;
}

static void RestoreMon(u32 partyIndex)
{
    struct Pokemon *mon = &gParties[B_TRAINER_0][partyIndex];
    struct CappedMon *saved = &sCappedMons[partyIndex];
    u32 hp, maxHp;
    if (!saved->projected)
        return;
    hp = GetMonData(mon, MON_DATA_HP);
    maxHp = GetMonData(mon, MON_DATA_MAX_HP);
    SetMonData(mon, MON_DATA_SPECIES, &saved->species);
    SetMonData(mon, MON_DATA_EXP, &saved->experience);
    CalculateMonStats(mon);
    // An untouched round trip must not lose HP through integer rounding.
    if (hp == saved->projectedHp && maxHp == saved->projectedMaxHp)
        hp = ScaleHp(saved->hp, saved->maxHp, GetMonData(mon, MON_DATA_MAX_HP));
    else
        hp = ScaleHp(hp, maxHp, GetMonData(mon, MON_DATA_MAX_HP));
    SetMonData(mon, MON_DATA_HP, &hp);
    saved->projected = FALSE;
}

void BeginBattleLevelCaps(void)
{
    memset(sCappedMons, 0, sizeof(sCappedMons));
    sBattleLevelCap = MAX_LEVEL;
    if (!GetConfig(B_BADGE_BATTLE_CAP)
     || (gBattleTypeFlags & (BATTLE_TYPE_LINK | BATTLE_TYPE_RECORDED_LINK
                          | BATTLE_TYPE_FRONTIER | BATTLE_TYPE_TRAINER_HILL | BATTLE_TYPE_EREADER_TRAINER
                          | BATTLE_TYPE_FIRST_BATTLE | BATTLE_TYPE_SAFARI | BATTLE_TYPE_CATCH_TUTORIAL
                          | BATTLE_TYPE_POKEDUDE | BATTLE_TYPE_RAID)))
        return;
    // The test runner uses recorded playback to drive ordinary wild battles.
    if ((gBattleTypeFlags & BATTLE_TYPE_RECORDED) && !(gTestRunnerEnabled && (gBattleTypeFlags & BATTLE_TYPE_IS_MASTER)))
        return;
    sBattleLevelCap = GetBadgeBattleLevelCap();
    for (u32 i = 0; i < PARTY_SIZE; i++)
        ProjectMon(i);
}

void EndBattleLevelCaps(void)
{
    for (u32 i = 0; i < PARTY_SIZE; i++)
        RestoreMon(i);
    sBattleLevelCap = MAX_LEVEL;
}

bool32 BattleCaps_BeginExperience(u32 partyIndex)
{
    if (partyIndex < PARTY_SIZE && sCappedMons[partyIndex].projected)
    {
        // Preserve in-battle forms (Mega Evolution, stance/weather forms)
        // while the real species temporarily owns level-up processing.
        sCappedMons[partyIndex].battleSpecies = GetMonData(&gParties[B_TRAINER_0][partyIndex], MON_DATA_SPECIES);
        sCappedMons[partyIndex].resumeHp = GetMonData(&gParties[B_TRAINER_0][partyIndex], MON_DATA_HP);
        sCappedMons[partyIndex].resumeMaxHp = GetMonData(&gParties[B_TRAINER_0][partyIndex], MON_DATA_MAX_HP);
        sCappedMons[partyIndex].resumeForm = TRUE;
        RestoreMon(partyIndex);
        return TRUE;
    }
    return FALSE;
}

bool32 BattleCaps_EndExperience(u32 partyIndex)
{
    if (partyIndex < PARTY_SIZE)
    {
        struct Pokemon *mon = &gParties[B_TRAINER_0][partyIndex];
        struct CappedMon *saved = &sCappedMons[partyIndex];
        bool32 resuming = saved->resumeForm;
        bool32 dynamax = FALSE;
        u32 resumeHp = saved->resumeHp;
        u32 resumeMaxHp = saved->resumeMaxHp;
        if (resuming)
        {
            // Normalize Dynamax units before applying ordinary stat growth.
            // Keep the captured combat HP to avoid repeated rounding losses.
            for (enum BattlerId battler = 0; battler < gBattlersCount; battler++)
                if (GetBattlerMon(battler) == mon && GetActiveGimmick(battler) == GIMMICK_DYNAMAX)
                    dynamax = TRUE;
            if (dynamax)
            {
                uq4_12_t inverse = GetDynamaxLevelHPMultiplier(GetMonData(mon, MON_DATA_DYNAMAX_LEVEL), TRUE);
                u32 hp = UQ_4_12_TO_INT(GetMonData(mon, MON_DATA_HP) * inverse + UQ_4_12_ROUND);
                u32 maxHp = UQ_4_12_TO_INT(GetMonData(mon, MON_DATA_MAX_HP) * inverse + UQ_4_12_ROUND);
                SetMonData(mon, MON_DATA_HP, &hp);
                SetMonData(mon, MON_DATA_MAX_HP, &maxHp);
                resumeHp = UQ_4_12_TO_INT(resumeHp * inverse + UQ_4_12_ROUND);
                resumeMaxHp = UQ_4_12_TO_INT(resumeMaxHp * inverse + UQ_4_12_ROUND);
            }
            // Keep the ordinary HP increase from level/EV stat growth.
            CalculateMonStats(mon);
        }
        ProjectMon(partyIndex);
        if (resuming && saved->projected)
        {
            saved->projectedHp = HpAfterStatChange(resumeHp, resumeMaxHp, saved->projectedMaxHp);
            SetMonData(mon, MON_DATA_HP, &saved->projectedHp);
            if (dynamax)
            {
                ApplyDynamaxHPMultiplier(mon);
                u32 hp = HpAfterStatChange(saved->resumeHp, saved->resumeMaxHp, GetMonData(mon, MON_DATA_MAX_HP));
                SetMonData(mon, MON_DATA_HP, &hp);
            }
        }
        return sCappedMons[partyIndex].projected;
    }
    return FALSE;
}
