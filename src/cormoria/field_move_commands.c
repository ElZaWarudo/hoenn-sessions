#include "global.h"
#include "cormoria/field_move_commands.h"
#include "event_data.h"
#include "item.h"
#include "party_menu.h"
#include "pokemon.h"
#include "script.h"
#include "constants/items.h"
#include "constants/moves.h"
#include "constants/species.h"

static enum Item GetCarriedMachineForMove(enum Move move)
{
    switch (move)
    {
    case MOVE_SECRET_POWER: return ITEM_TM43;
    case MOVE_CUT:          return ITEM_HM01;
    case MOVE_FLY:          return ITEM_HM02;
    case MOVE_SURF:         return ITEM_HM03;
    case MOVE_STRENGTH:     return ITEM_HM04;
    case MOVE_FLASH:        return ITEM_HM05;
    case MOVE_ROCK_SMASH:   return ITEM_HM06;
    case MOVE_WATERFALL:    return ITEM_HM07;
    case MOVE_DIVE:         return ITEM_HM08;
    default:                return ITEM_NONE;
    }
}

bool8 ScrCmd_CormoriaCheckPartyMove(struct ScriptContext *ctx)
{
    enum Move move = ScriptReadHalfword(ctx);
    enum Item machine = GetCarriedMachineForMove(move);
    u32 i;

    Script_RequestEffects(SCREFF_V1);
    gSpecialVar_Result = PARTY_SIZE;
    for (i = 0; i < PARTY_SIZE; i++)
    {
        enum Species species = GetMonData(&gPlayerParty[i], MON_DATA_SPECIES);
        if (species == SPECIES_NONE)
            break;
        if (!GetMonData(&gPlayerParty[i], MON_DATA_IS_EGG)
            && MonKnowsMove(&gPlayerParty[i], move))
        {
            gSpecialVar_Result = i;
            gSpecialVar_0x8004 = species;
            return FALSE;
        }
    }
    if (machine != ITEM_NONE && CheckBagHasItem(machine, 1))
    {
        gSpecialVar_Result = 0;
        gSpecialVar_0x8004 = GetMonData(&gPlayerParty[0], MON_DATA_SPECIES);
    }
    return FALSE;
}
