#include "global.h"
#include "constants/characters.h"
#include "event_data.h"
#include "pokemon.h"
#include "script.h"
#include "string_util.h"
#include "johto/script_text.h"

void Script_JohtoBufferMonCategory(struct ScriptContext *ctx)
{
    u8 destination = ScriptReadByte(ctx);
    u16 species = VarGet(ScriptReadHalfword(ctx));
    u8 *buffers[] = {gStringVar1, gStringVar2, gStringVar3};

    Script_RequestEffects(SCREFF_V1);
    if (destination >= ARRAY_COUNT(buffers))
        return;
    if (species == SPECIES_NONE || species >= NUM_SPECIES)
    {
        buffers[destination][0] = EOS;
        return;
    }
    StringCopy(buffers[destination], GetSpeciesCategory(species));
}
