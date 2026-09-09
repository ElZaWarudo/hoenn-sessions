#include "global.h"
#include "event_data.h"
#include "pokemon.h"
#include "script.h"
#include "johto/field_moves.h"

u32 JohtoFieldMoves_GetWhirlpoolUser(struct Pokemon *party, u32 count)
{
    u32 i, move;

    if (count > PARTY_SIZE)
        count = PARTY_SIZE;
    for (i = 0; i < count; i++)
    {
        if (GetMonData(&party[i], MON_DATA_SPECIES) == SPECIES_NONE
         || GetMonData(&party[i], MON_DATA_IS_EGG))
            continue;
        for (move = 0; move < MAX_MON_MOVES; move++)
            if (GetMonData(&party[i], MON_DATA_MOVE1 + move) == MOVE_WHIRLPOOL)
                return i;
    }
    return PARTY_SIZE;
}

void Script_JohtoCheckWhirlpool(struct ScriptContext *ctx)
{
    Script_RequestEffects(SCREFF_V1);
    gSpecialVar_Result = JohtoFieldMoves_GetWhirlpoolUser(gPlayerParty, gPlayerPartyCount);
}
