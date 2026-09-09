#include "global.h"
#include "battle.h"
#include "constants/items.h"
#include "constants/party_menu.h"
#include "constants/species.h"
#include "event_data.h"
#include "item.h"
#include "johto/haircut.h"
#include "main.h"
#include "pokemon.h"
#include "script.h"

static enum HoldEffect GetHaircutHoldEffect(struct Pokemon *mon)
{
    enum Item heldItem = GetMonData(mon, MON_DATA_HELD_ITEM, NULL);

    if (heldItem == ITEM_ENIGMA_BERRY_E_READER)
    {
        if (gMain.inBattle)
            return gEnigmaBerries[0].holdEffect;
#if FREE_ENIGMA_BERRY == FALSE
        return gSaveBlock1Ptr->enigmaBerry.holdEffect;
#else
        return 0;
#endif
    }

    return GetItemHoldEffect(heldItem);
}

bool8 JohtoHaircut_Apply(u16 partySlot)
{
    struct Pokemon *mon;
    enum Species species;
    enum HoldEffect holdEffect;
    s32 friendship;

    if (partySlot >= PARTY_SIZE || partySlot >= gPlayerPartyCount)
        return FALSE;

    mon = &gPlayerParty[partySlot];
    species = GetMonData(mon, MON_DATA_SPECIES_OR_EGG, NULL);
    if (species == SPECIES_NONE || species == SPECIES_EGG)
        return FALSE;

    if (ShouldSkipFriendshipChange())
        return TRUE;

    friendship = GetMonData(mon, MON_DATA_FRIENDSHIP, NULL);
    holdEffect = GetHaircutHoldEffect(mon);
    friendship += CalculateFriendshipBonuses(mon, 99, holdEffect);
    if (friendship > MAX_FRIENDSHIP)
        friendship = MAX_FRIENDSHIP;

    SetMonData(mon, MON_DATA_FRIENDSHIP, &friendship);
    return TRUE;
}

void HaircutBrother1(void)
{
    Script_RequestEffects(SCREFF_V1 | SCREFF_SAVE);
    gSpecialVar_Result = JohtoHaircut_Apply(gSpecialVar_0x8004);
}
