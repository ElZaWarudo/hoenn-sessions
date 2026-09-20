#include "global.h"
#include "event_object_movement.h"
#include "pokemon.h"
#include "script.h"
#include "johto/scene_checks.h"
#include "constants/species.h"

u16 Johto_CheckHooh(void)
{
    Script_RequestEffects(SCREFF_V1);
    return GetMonData(&gPlayerParty[0], MON_DATA_SPECIES_OR_EGG) == SPECIES_HO_OH;
}

u16 Johto_CheckAerodactyl(void)
{
    Script_RequestEffects(SCREFF_V1);
    return GetMonData(&gPlayerParty[0], MON_DATA_SPECIES_OR_EGG) == SPECIES_AERODACTYL;
}

u16 Johto_CheckKabuto(void)
{
    Script_RequestEffects(SCREFF_V1);
    return GetMonData(&gPlayerParty[0], MON_DATA_SPECIES_OR_EGG) == SPECIES_KABUTO;
}

u16 Johto_CheckOmanyte(void)
{
    Script_RequestEffects(SCREFF_V1);
    return GetMonData(&gPlayerParty[0], MON_DATA_SPECIES_OR_EGG) == SPECIES_OMANYTE;
}

u16 Johto_CheckTogepi(void)
{
    enum Species species;

    Script_RequestEffects(SCREFF_V1);
    species = GetMonData(&gPlayerParty[0], MON_DATA_SPECIES_OR_EGG);
    return species == SPECIES_TOGEPI
        || species == SPECIES_TOGETIC
        || species == SPECIES_TOGEKISS;
}

u16 Johto_CheckCelebi(void)
{
    struct Pokemon *mon;
    struct ObjectEvent *follower;
    u16 hp;
    u16 maxHp;

    Script_RequestEffects(SCREFF_V1);
    mon = &gPlayerParty[0];
    if (GetMonData(mon, MON_DATA_SPECIES_OR_EGG) != SPECIES_CELEBI)
        return FALSE;

    hp = GetMonData(mon, MON_DATA_HP);
    maxHp = GetMonData(mon, MON_DATA_MAX_HP);
    if (hp == 0 || hp != maxHp)
        return FALSE;

    follower = GetFollowerObject();
    if (follower == NULL || follower->invisible)
        return FALSE;
    return TRUE;
}
