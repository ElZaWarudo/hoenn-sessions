#include "global.h"
#include "mastery.h"
#include "string_util.h"

static const u8 sAlpha[] = _("Alpha ");
static const u8 sBeta[] = _("Beta ");
static const u8 sOmega[] = _("Omega ");
static const u8 sAlphaShort[] = _("A");
static const u8 sBetaShort[] = _("B");
static const u8 sOmegaShort[] = _("O");

// The slowest growth curve needs 1,640,000 EXP at level 100.
STATIC_ASSERT(1640000 + MAX_MASTERY_LEVEL * MASTERY_EXP_PER_LEVEL < (1 << 24), MasteryExperienceFitsSave);

u32 GetMasteryLevel(enum Species species, u32 experience)
{
    u32 base = gExperienceTables[gSpeciesInfo[species].growthRate][MAX_LEVEL];
    if (experience <= base)
        return 0;
    return min((experience - base) / MASTERY_EXP_PER_LEVEL, MAX_MASTERY_LEVEL);
}

u32 GetMonMasteryLevel(struct Pokemon *mon)
{
    return GetMasteryLevel(GetMonData(mon, MON_DATA_SPECIES), GetMonData(mon, MON_DATA_EXP));
}

u32 GetMaxMonExperience(enum Species species)
{
    return gExperienceTables[gSpeciesInfo[species].growthRate][MAX_LEVEL]
         + MAX_MASTERY_LEVEL * MASTERY_EXP_PER_LEVEL;
}

bool32 CanMonGainExperience(struct Pokemon *mon)
{
    return GetMonData(mon, MON_DATA_EXP) < GetMaxMonExperience(GetMonData(mon, MON_DATA_SPECIES));
}

void AddMonExperience(struct Pokemon *mon, u32 experience)
{
    u32 maximum = GetMaxMonExperience(GetMonData(mon, MON_DATA_SPECIES));
    u32 current = min(GetMonData(mon, MON_DATA_EXP), maximum);
    current += min(experience, maximum - current);
    SetMonData(mon, MON_DATA_EXP, &current);
}

void GetProgressLevelExpBounds(enum Species species, u32 experience, u32 *start, u32 *next)
{
    const u32 *table = gExperienceTables[gSpeciesInfo[species].growthRate];
    if (experience >= table[MAX_LEVEL])
    {
        *start = table[MAX_LEVEL] + GetMasteryLevel(species, experience) * MASTERY_EXP_PER_LEVEL;
        *next = min(*start + MASTERY_EXP_PER_LEVEL, GetMaxMonExperience(species));
        return;
    }
    *start = 0;
    for (u32 level = 1; level <= MAX_LEVEL; level++)
    {
        *next = table[level];
        if (*next > experience)
            return;
        *start = *next;
    }
}

u32 GetProgressLevelNextExp(enum Species species, u32 experience)
{
    u32 start, next;
    GetProgressLevelExpBounds(species, experience, &start, &next);
    return next;
}

void FormatMasteryLevel(u8 *dest, u32 masteryLevel, bool32 compact)
{
    static const u8 *const names[] = {sAlpha, sBeta, sOmega};
    static const u8 *const shortNames[] = {sAlphaShort, sBetaShort, sOmegaShort};
    u32 index = min(max(masteryLevel, 1), MAX_MASTERY_LEVEL) - 1;
    dest = StringCopy(dest, (compact ? shortNames : names)[index / MASTERY_LEVELS_PER_RANK]);
    ConvertIntToDecimalStringN(dest, index % MASTERY_LEVELS_PER_RANK + 1, STR_CONV_MODE_LEFT_ALIGN, 3);
}
