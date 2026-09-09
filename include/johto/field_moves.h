#ifndef GUARD_JOHTO_FIELD_MOVES_H
#define GUARD_JOHTO_FIELD_MOVES_H

struct Pokemon;
struct ScriptContext;

u32 JohtoFieldMoves_GetWhirlpoolUser(struct Pokemon *party, u32 count);
void Script_JohtoCheckWhirlpool(struct ScriptContext *ctx);

#endif
