#include "global.h"
#include "event_data.h"
#include "pokemon.h"
#include "script.h"
#include "constants/metatile_behaviors.h"
#include "johto/field_moves.h"

extern const u8 Johto_EventScript_Headbutt[];

static u32 GetKnownMoveUser(struct Pokemon *party, u32 count, enum Move move)
{
    u32 i, moveSlot;

    if (count > PARTY_SIZE)
        count = PARTY_SIZE;
    for (i = 0; i < count; i++)
    {
        if (GetMonData(&party[i], MON_DATA_SPECIES) == SPECIES_NONE
         || GetMonData(&party[i], MON_DATA_IS_EGG))
            continue;
        for (moveSlot = 0; moveSlot < MAX_MON_MOVES; moveSlot++)
            if (GetMonData(&party[i], MON_DATA_MOVE1 + moveSlot) == move)
                return i;
    }
    return PARTY_SIZE;
}

u32 JohtoFieldMoves_GetWhirlpoolUser(struct Pokemon *party, u32 count)
{
    return GetKnownMoveUser(party, count, MOVE_WHIRLPOOL);
}

void Script_JohtoCheckWhirlpool(struct ScriptContext *ctx)
{
    Script_RequestEffects(SCREFF_V1);
    gSpecialVar_Result = JohtoFieldMoves_GetWhirlpoolUser(gPlayerParty, gPlayerPartyCount);
}

u32 JohtoFieldMoves_GetHeadbuttUser(struct Pokemon *party, u32 count)
{
    return GetKnownMoveUser(party, count, MOVE_HEADBUTT);
}

void Script_JohtoCheckHeadbutt(struct ScriptContext *ctx)
{
    Script_RequestEffects(SCREFF_V1);
    gSpecialVar_Result = JohtoFieldMoves_GetHeadbuttUser(gPlayerParty, gPlayerPartyCount);
}

const u8 *JohtoFieldMoves_GetHeadbuttScript(u8 metatileBehavior)
{
    if (metatileBehavior == MB_JOHTO_HEADBUTT_TREE)
        return Johto_EventScript_Headbutt;
    return NULL;
}
