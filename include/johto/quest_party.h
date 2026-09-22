#ifndef GUARD_JOHTO_QUEST_PARTY_H
#define GUARD_JOHTO_QUEST_PARTY_H

#include "global.h"

struct ScriptContext;

enum JohtoQuestNamedGiftId
{
    JOHTO_QUEST_NAMED_GIFT_KENYA = 1,
    JOHTO_QUEST_NAMED_GIFT_SHUCKIE = 2,
};

enum JohtoQuestPartyResult
{
    JOHTO_QUEST_RESULT_FAILURE = 0,
    JOHTO_QUEST_RESULT_MAGIKARP = 1,
    JOHTO_QUEST_RESULT_MAGIKARP_LEVEL_100 = 2,
    JOHTO_QUEST_RESULT_SHUCKIE_TOO_FRIENDLY = 3,
};

enum JohtoQuestNamedResult
{
    JOHTO_QUEST_RESULT_NAMED_GIVEN = 0,
    JOHTO_QUEST_RESULT_NAMED_CANT_GIVE = 2,
};

void Script_JohtoRemoveNamedMon(struct ScriptContext *ctx);
void Script_JohtoRemoveGenericMon(struct ScriptContext *ctx);
void Script_JohtoBaobaCheckMon(struct ScriptContext *ctx);

#endif // GUARD_JOHTO_QUEST_PARTY_H
